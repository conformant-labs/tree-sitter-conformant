use crate::folding::parse_plato_tree;
use lsp_types::{Position, Range};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use tree_sitter::{Node, Tree};

#[derive(Clone, Debug)]
pub struct SchemaSymbol {
    pub key: String,
    pub name: String,
    pub category: String,
    pub owner: String,
    pub target: String,
    pub detail: String,
    pub doc: String,
    pub span: Option<std::ops::Range<usize>>,
}

#[derive(Clone, Debug)]
pub struct SymbolUse {
    pub key: String,
    pub span: std::ops::Range<usize>,
    pub declaration: bool,
}

/// A schema file is an independent namespace, including implicit reverse relationships.
pub struct SchemaIndex {
    pub source: String,
    pub tree: Tree,
    pub symbols: Vec<SchemaSymbol>,
    pub uses: Vec<SymbolUse>,
    parents: HashMap<String, Vec<String>>,
}

pub fn syntax_children(node: Node) -> Vec<Node> {
    node.children(&mut node.walk()).collect()
}
fn text<'a>(node: Node, source: &'a str) -> &'a str {
    &source[node.byte_range()]
}
fn ancestor<'a>(mut node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    loop {
        if node.kind() == kind {
            return Some(node);
        }
        node = node.parent()?;
    }
}
fn named(node: Node, source: &str) -> String {
    node.child_by_field_name("name")
        .map(|n| text(n, source).to_owned())
        .unwrap_or_default()
}
fn symbol_key(category: &str, owner: &str, name: &str) -> String {
    format!("{category}:{owner}:{name}")
}
fn previous_doc(node: Node, source: &str) -> String {
    let mut sibling = node.prev_named_sibling();
    let mut docs = vec![];
    while let Some(item) = sibling {
        if item.kind() == "doc_string" {
            docs.push(text(item, source).trim_matches('"').trim().to_owned());
        } else if item.kind() != "comment" {
            break;
        }
        sibling = item.prev_named_sibling();
    }
    docs.reverse();
    docs.join("\n\n")
}

/// Translate byte offsets to LSP UTF-16 positions, including non-ASCII text.
pub fn source_position(source: &str, byte: usize) -> Position {
    let mut byte = byte.min(source.len());
    while !source.is_char_boundary(byte) {
        byte -= 1;
    }
    let prefix = &source[..byte];
    Position::new(
        prefix.bytes().filter(|b| *b == b'\n').count() as u32,
        prefix
            .rsplit('\n')
            .next()
            .unwrap_or("")
            .encode_utf16()
            .count() as u32,
    )
}

pub fn position_offset(source: &str, position: Position) -> usize {
    let mut offset = 0;
    for (row, line) in source.split_inclusive('\n').enumerate() {
        if row == position.line as usize {
            let mut units = 0;
            for (index, ch) in line.char_indices() {
                if units + ch.len_utf16() > position.character as usize {
                    return offset + index;
                }
                units += ch.len_utf16();
                if units == position.character as usize {
                    return offset + index + ch.len_utf8();
                }
            }
            return offset + line.trim_end_matches(['\r', '\n']).len();
        }
        offset += line.len();
    }
    source.len()
}

pub fn source_range(source: &str, span: std::ops::Range<usize>) -> Range {
    Range::new(
        source_position(source, span.start),
        source_position(source, span.end),
    )
}

impl SchemaIndex {
    pub fn new(source: String, schema: Option<&Value>) -> Self {
        let tree = parse_plato_tree(&source);
        let mut index = Self {
            source,
            tree,
            symbols: vec![],
            uses: vec![],
            parents: HashMap::new(),
        };
        let tree = index.tree.clone();
        index.collect_declarations(tree.root_node());
        index.add_implicit_fields(schema);
        index.collect_uses(tree.root_node());
        index
    }

    fn add_symbol(
        &mut self,
        namespace: (&str, &str),
        name: String,
        target: String,
        detail: String,
        doc: String,
        span: Option<std::ops::Range<usize>>,
    ) {
        let (category, owner) = namespace;
        let key = symbol_key(category, owner, &name);
        if let Some(span) = &span {
            self.uses.push(SymbolUse {
                key: key.clone(),
                span: span.clone(),
                declaration: true,
            });
        }
        self.symbols.push(SchemaSymbol {
            key,
            name,
            category: category.into(),
            owner: owner.into(),
            target,
            detail,
            doc,
            span,
        });
    }

    fn collect_declarations(&mut self, node: Node) {
        let category = match node.kind() {
            "model_declaration" => "model",
            "role_declaration" => "role",
            "enum_declaration" => "enum",
            "scalar_declaration" => "scalar",
            "view_declaration" => "view",
            "model_field" => "field",
            "action" => "action",
            _ => "",
        };
        if !category.is_empty() {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = text(name_node, &self.source).to_owned();
                let owner = if matches!(category, "field" | "action") {
                    ancestor(node, "model_declaration")
                        .map(|m| named(m, &self.source))
                        .unwrap_or_default()
                } else {
                    String::new()
                };
                let field_type = node.child_by_field_name("type");
                let target = field_type
                    .map(|n| text(n, &self.source).trim_end_matches("[]").to_owned())
                    .unwrap_or_default();
                let detail = if category == "field" {
                    format!(
                        "{owner}.{name}: {}{}",
                        field_type.map(|n| text(n, &self.source)).unwrap_or(""),
                        if syntax_children(node).iter().any(|n| n.kind() == "?") {
                            "?"
                        } else {
                            ""
                        }
                    )
                } else {
                    text(node, &self.source)
                        .split('{')
                        .next()
                        .unwrap_or("")
                        .trim()
                        .to_owned()
                };
                self.add_symbol(
                    (category, &owner),
                    name.clone(),
                    target.clone(),
                    detail,
                    previous_doc(node, &self.source),
                    Some(name_node.byte_range()),
                );
                if category == "model" {
                    let parents = syntax_children(node)
                        .iter()
                        .find(|n| n.kind() == "extends_clause")
                        .map(|n| {
                            syntax_children(*n)
                                .iter()
                                .filter(|c| c.kind() == "type_identifier")
                                .map(|n| text(*n, &self.source).to_owned())
                                .collect()
                        })
                        .unwrap_or_default();
                    self.parents.insert(name.clone(), parents);
                }
                if category == "field" {
                    for attribute in syntax_children(node) {
                        if attribute.kind() != "attribute"
                            || named(attribute, &self.source) != "relation"
                        {
                            continue;
                        }
                        if let Some(arguments) = syntax_children(attribute)
                            .into_iter()
                            .find(|n| n.kind() == "arguments")
                        {
                            if let Some(value) = syntax_children(arguments)
                                .into_iter()
                                .find(|n| n.kind() == "string")
                            {
                                let relation = text(value, &self.source)
                                    .trim_matches(['\'', '"'])
                                    .to_owned();
                                self.add_symbol(
                                    ("field", &target),
                                    relation.clone(),
                                    owner.clone(),
                                    format!("{target}.{relation}: {owner}[]"),
                                    format!("Reverse relationship from {owner}.{name}"),
                                    Some(value.start_byte() + 1..value.end_byte() - 1),
                                );
                            }
                        }
                    }
                }
            }
        }
        if node.kind() == "enum_body" {
            let owner = if let Some(field) = ancestor(node, "model_field") {
                format!(
                    "{}.{}",
                    ancestor(field, "model_declaration")
                        .map(|m| named(m, &self.source))
                        .unwrap_or_default(),
                    named(field, &self.source)
                )
            } else {
                ancestor(node, "enum_declaration")
                    .map(|e| named(e, &self.source))
                    .unwrap_or_default()
            };
            for member in syntax_children(node)
                .into_iter()
                .filter(|n| matches!(n.kind(), "enum_member" | "string"))
            {
                let quoted = member.kind() == "string";
                let span = if quoted {
                    member.start_byte() + 1..member.end_byte() - 1
                } else {
                    member.byte_range()
                };
                let name = self.source[span.clone()].to_owned();
                self.add_symbol(
                    ("value", &owner),
                    name.clone(),
                    String::new(),
                    format!("{owner}.{name}"),
                    previous_doc(member, &self.source),
                    Some(span),
                );
            }
        }
        for child in syntax_children(node) {
            self.collect_declarations(child);
        }
    }

    fn add_implicit_fields(&mut self, schema: Option<&Value>) {
        let models: Vec<_> = self
            .symbols
            .iter()
            .filter(|s| s.category == "model")
            .map(|s| s.name.clone())
            .collect();
        for model in models {
            for (name, target) in [
                ("id", "ID"),
                ("createdAt", "DateTime"),
                ("updatedAt", "DateTime"),
                ("sortOrder", "Float"),
            ] {
                if self.field(&model, name).is_none() {
                    self.add_symbol(
                        ("field", &model),
                        name.into(),
                        target.into(),
                        format!("{model}.{name}: {target}"),
                        "Built-in field".into(),
                        None,
                    );
                }
            }
        }
        let tree = self.tree.clone();
        fn derived(index: &mut SchemaIndex, node: Node) {
            if node.kind() == "derives_clause" {
                if let (Some(model), Some(base)) = (
                    ancestor(node, "model_declaration"),
                    node.child_by_field_name("base"),
                ) {
                    let owner = named(model, &index.source);
                    let target = text(base, &index.source).to_owned();
                    index.add_symbol(
                        ("field", &owner),
                        "base".into(),
                        target.clone(),
                        format!("{owner}.base: {target}"),
                        "Underlying record of this derived model".into(),
                        None,
                    );
                }
            }
            for child in syntax_children(node) {
                derived(index, child);
            }
        }
        derived(self, tree.root_node());
        if let Some(models) = schema.and_then(|s| s["models"].as_array()) {
            for model in models {
                let owner = model["name"].as_str().unwrap_or("");
                if let Some(fields) = model["fields"].as_array() {
                    for field in fields {
                        let name = field["name"].as_str().unwrap_or("");
                        if self.field(owner, name).is_none() {
                            let target = field["targetModel"]
                                .as_str()
                                .or(field["targetEnum"].as_str())
                                .or(field["type"].as_str())
                                .unwrap_or("");
                            self.add_symbol(
                                ("field", owner),
                                name.into(),
                                target.into(),
                                format!("{owner}.{name}: {target}"),
                                field["doc"].as_str().unwrap_or("Generated field").into(),
                                None,
                            );
                        }
                    }
                }
            }
        }
    }

    pub fn symbol(&self, key: &str) -> Option<&SchemaSymbol> {
        self.symbols.iter().find(|s| s.key == key)
    }
    fn global(&self, category: &str, name: &str) -> Option<&SchemaSymbol> {
        self.symbols
            .iter()
            .find(|s| s.category == category && s.owner.is_empty() && s.name == name)
    }
    fn field(&self, owner: &str, name: &str) -> Option<&SchemaSymbol> {
        let mut pending = vec![owner.to_owned()];
        let mut visited = HashSet::new();
        while let Some(model) = pending.pop() {
            if !visited.insert(model.clone()) {
                continue;
            }
            if let Some(field) = self
                .symbols
                .iter()
                .find(|s| s.category == "field" && s.owner == model && s.name == name)
            {
                return Some(field);
            }
            if let Some(parents) = self.parents.get(&model) {
                pending.extend(parents.iter().cloned());
            }
        }
        None
    }

    fn view_context(&self, node: Node) -> Option<String> {
        let mut chain = vec![];
        let mut current = Some(node);
        while let Some(item) = current {
            if item.kind() == "view_field" {
                chain.push(item);
            }
            current = item.parent();
        }
        chain.reverse();
        let first = chain.first()?.child_by_field_name("field")?;
        let mut model = text(first, &self.source).to_owned();
        for item in chain.iter().skip(1) {
            let field = item.child_by_field_name("field")?;
            model = self
                .field(&model, text(field, &self.source))?
                .target
                .clone();
        }
        Some(model)
    }

    fn context(&self, node: Node) -> Option<String> {
        ancestor(node, "model_declaration")
            .map(|n| named(n, &self.source))
            .or_else(|| self.view_context(node))
    }

    fn chain_symbols<'a>(&self, node: Node<'a>) -> Vec<(Node<'a>, String)> {
        let mut result = vec![];
        let mut model = self.context(node).unwrap_or_default();
        let mut first = true;
        for part in syntax_children(node)
            .into_iter()
            .filter(|n| n.kind() == "identifier")
        {
            let name = text(part, &self.source);
            let symbol = if first {
                self.field(&model, name)
                    .or_else(|| self.global("role", name))
                    .or_else(|| self.global("model", name))
            } else {
                self.field(&model, name)
            };
            if let Some(symbol) = symbol {
                result.push((part, symbol.key.clone()));
                model = if symbol.category == "model" {
                    symbol.name.clone()
                } else {
                    symbol.target.clone()
                };
            } else {
                break;
            }
            first = false;
        }
        result
    }

    fn enum_owner(&self, node: Node) -> Option<String> {
        let field = if let Some(attr) =
            ancestor(node, "attribute").filter(|n| named(*n, &self.source) == "default")
        {
            ancestor(attr, "model_field")
                .and_then(|f| self.field(&self.context(f)?, &named(f, &self.source)))
        } else if let Some(assignment) = ancestor(node, "assignment") {
            let target = assignment.child_by_field_name("target")?;
            self.chain_symbols(target)
                .last()
                .and_then(|(_, key)| self.symbol(key))
        } else if let Some(binary) = ancestor(node, "binary_expression") {
            let left = binary.child_by_field_name("left")?;
            if left.kind() != "reference" {
                return None;
            }
            self.chain_symbols(left)
                .last()
                .and_then(|(_, key)| self.symbol(key))
        } else {
            None
        }?;
        Some(if field.target.starts_with("enum") {
            format!("{}.{}", field.owner, field.name)
        } else {
            field.target.clone()
        })
    }

    fn collect_uses(&mut self, node: Node) {
        if matches!(node.kind(), "reference" | "assignment_target") {
            for (part, key) in self.chain_symbols(node) {
                self.add_use(key, part.byte_range());
            }
        }
        if matches!(
            node.kind(),
            "type_identifier" | "parameter" | "field_identifier" | "string" | "identifier"
        ) {
            let already = self
                .uses
                .iter()
                .any(|u| u.span.start >= node.start_byte() && u.span.end <= node.end_byte());
            if !already {
                let name = text(node, &self.source).trim_matches(['\'', '"']);
                let parent = node.parent();
                let mut symbol = None;
                if node.kind() == "type_identifier" {
                    let role_context = parent.is_some_and(|p| p.kind() == "view_declaration");
                    symbol = if role_context {
                        self.global("role", name)
                    } else {
                        self.global("model", name)
                            .or_else(|| self.global("enum", name))
                            .or_else(|| self.global("scalar", name))
                    };
                } else if parent.is_some_and(|p| p.kind() == "view_field") {
                    let parent = parent.unwrap();
                    if parent.child_by_field_name("field") == Some(node) {
                        let outer = parent.parent().and_then(|p| ancestor(p, "view_field"));
                        symbol = if let Some(outer) = outer {
                            self.view_context(outer).and_then(|m| self.field(&m, name))
                        } else {
                            self.global("model", name)
                        };
                    }
                } else if node.kind() == "parameter"
                    || parent.is_some_and(|p| p.kind() == "with_clause")
                {
                    symbol = self.context(node).and_then(|m| self.field(&m, name));
                }
                if symbol.is_none() && matches!(node.kind(), "string" | "identifier") {
                    if let Some(owner) = self.enum_owner(node) {
                        symbol = self
                            .symbols
                            .iter()
                            .find(|s| s.category == "value" && s.owner == owner && s.name == name);
                    }
                }
                if let Some(symbol) = symbol {
                    let key = symbol.key.clone();
                    let span = if node.kind() == "string" {
                        node.start_byte() + 1..node.end_byte() - 1
                    } else {
                        node.byte_range()
                    };
                    self.add_use(key, span);
                }
            }
        }
        for child in syntax_children(node) {
            self.collect_uses(child);
        }
    }

    fn add_use(&mut self, key: String, span: std::ops::Range<usize>) {
        if !self.uses.iter().any(|u| u.span == span) {
            self.uses.push(SymbolUse {
                key,
                span,
                declaration: false,
            });
        }
    }
    pub fn at(&self, position: Position) -> Option<(&SchemaSymbol, &SymbolUse)> {
        let offset = position_offset(&self.source, position);
        let usage = self
            .uses
            .iter()
            .find(|u| u.span.start <= offset && offset < u.span.end)
            .or_else(|| self.uses.iter().find(|u| u.span.end == offset))?;
        Some((self.symbol(&usage.key)?, usage))
    }
    pub fn hover(&self, position: Position) -> Value {
        self.at(position).map(|(symbol,usage)|json!({"contents":{"kind":"markdown","value":format!("```plato\n{}\n```\n\n{}",symbol.detail,symbol.doc)},"range":source_range(&self.source,usage.span.clone())})).unwrap_or(Value::Null)
    }
    pub fn definition(&self, position: Position, uri: &str) -> Value {
        self.at(position)
            .and_then(|(s, _)| s.span.clone())
            .map(|span| json!({"uri":uri,"range":source_range(&self.source,span)}))
            .unwrap_or(Value::Null)
    }
    pub fn references(&self, position: Position, uri: &str, include_declaration: bool) -> Value {
        let Some((symbol, _)) = self.at(position) else {
            return json!([]);
        };
        json!(self
            .uses
            .iter()
            .filter(|u| u.key == symbol.key && (include_declaration || !u.declaration))
            .map(|u| json!({"uri":uri,"range":source_range(&self.source,u.span.clone())}))
            .collect::<Vec<_>>())
    }
    pub fn rename(
        &self,
        position: Position,
        new_name: &str,
        uri: &str,
        version: i32,
    ) -> Result<Value, String> {
        let (symbol, _) = self
            .at(position)
            .ok_or("No resolved schema symbol at this position")?;
        if symbol.span.is_none() {
            return Err("Built-in fields cannot be renamed".into());
        }
        if self.tree.root_node().has_error() {
            return Err("Fix syntax errors before renaming".into());
        }
        if !regex::Regex::new(r"^[A-Za-z_$][A-Za-z0-9_$]*$")
            .unwrap()
            .is_match(new_name)
            || [
                "true", "false", "null", "model", "enum", "role", "view", "scalar", "fields",
                "actions", "reads", "by", "where", "with", "extends", "derives", "create",
                "update", "delete",
            ]
            .contains(&new_name)
        {
            return Err("Choose a non-keyword Plato identifier".into());
        }
        if self.symbols.iter().any(|s| {
            s.key != symbol.key
                && s.name == new_name
                && s.owner == symbol.owner
                && (s.category == symbol.category
                    || s.owner.is_empty() && s.category != "role" && symbol.category != "role")
        }) {
            return Err("A symbol with that name already exists in this scope".into());
        }
        let edits: Vec<_> = self
            .uses
            .iter()
            .filter(|u| u.key == symbol.key)
            .map(|u| json!({"range":source_range(&self.source,u.span.clone()),"newText":new_name}))
            .collect();
        Ok(
            json!({"documentChanges":[{"textDocument":{"uri":uri,"version":version},"edits":edits}]}),
        )
    }

    pub fn completions(&self, position: Position) -> Value {
        let offset = position_offset(&self.source, position);
        let prefix = &self.source[..offset];
        let line = prefix.rsplit('\n').next().unwrap_or("");
        let node = self
            .tree
            .root_node()
            .descendant_for_byte_range(offset.saturating_sub(1), offset)
            .unwrap_or(self.tree.root_node());
        if ancestor(node, "comment").is_some() || ancestor(node, "doc_string").is_some() {
            return json!([]);
        }
        let mut model = self.context(node);
        let path = regex::Regex::new(
            r"([A-Za-z_$][A-Za-z0-9_$]*(?:\.[A-Za-z_$][A-Za-z0-9_$]*)*)\.([A-Za-z0-9_$]*)$",
        )
        .unwrap();
        let mut candidates: Vec<&SchemaSymbol> = vec![];
        if let Some(captures) = path.captures(prefix) {
            for (i, name) in captures[1].split('.').enumerate() {
                let symbol = model
                    .as_ref()
                    .and_then(|m| self.field(m, name))
                    .or_else(|| (i == 0).then(|| self.global("model", name)).flatten());
                model = symbol.map(|s| {
                    if s.category == "model" {
                        s.name.clone()
                    } else {
                        s.target.clone()
                    }
                });
            }
            if let Some(model) = model {
                candidates.extend(self.fields_for(&model));
            }
        } else if let Some(owner) = self.enum_owner(node) {
            candidates.extend(
                self.symbols
                    .iter()
                    .filter(|s| s.category == "value" && s.owner == owner),
            );
            if let Some(model) = model {
                candidates.extend(self.fields_for(&model));
            }
        } else {
            if let Some(model) = model {
                candidates.extend(self.fields_for(&model));
            }
            candidates.extend(
                self.symbols
                    .iter()
                    .filter(|s| s.owner.is_empty() && s.category != "view"),
            );
        }
        let mut items:Vec<Value>=candidates.into_iter().map(|s|json!({"label":s.name,"kind":match s.category.as_str(){"field"=>5,"model"=>7,"enum"=>13,"value"=>20,_=>6},"detail":s.detail,"documentation":{"kind":"markdown","value":s.doc}})).collect();
        if !line.contains('.') {
            let annotations = [
                "name", "title", "icon", "comments", "unique", "index", "relation", "computed",
                "default", "sequence", "layout", "where", "orderBy", "count", "sum", "avg", "min",
                "max", "limit",
            ];
            if regex::Regex::new(r"@[A-Za-z]*$").unwrap().is_match(line) {
                items = annotations
                    .iter()
                    .map(|name| json!({"label":name,"kind":14}))
                    .collect();
            } else {
                for name in [
                    "String", "Int", "Float", "Decimal", "Number", "Boolean", "Date", "DateTime",
                    "Image", "File", "File[]", "ID", "Identity", "Document", "Group", "model",
                    "fields", "actions", "reads", "view", "enum", "role", "scalar", "create",
                    "update", "delete", "by", "with", "where", "base",
                ] {
                    items.push(json!({"label":name,"kind":14}));
                }
            }
        }
        items.sort_by(|a, b| a["label"].as_str().cmp(&b["label"].as_str()));
        items.dedup_by(|a, b| a["label"] == b["label"]);
        json!(items)
    }
    fn fields_for(&self, model: &str) -> Vec<&SchemaSymbol> {
        self.symbols
            .iter()
            .filter(|s| {
                s.category == "field" && self.field(model, &s.name).is_some_and(|f| f.key == s.key)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const SOURCE: &str = r#"
role Customer
"""A company."""
model Supplier {
 fields { """Company display name.""" name String }
 reads { Customer }
 actions { update edit(id) with (name?) by Customer }
}
model Other { fields { name String } }
model Appointment {
 fields { supplier Supplier @relation("appointments") }
 reads { supplier.name == "name" && Customer }
}
view Active for Customer {
 Supplier @orderBy(name) {
 name
 appointments { supplier { name } }
 }
}
"#;
    fn point(source: &str, needle: &str, shift: usize) -> Position {
        source_position(source, source.find(needle).unwrap() + shift)
    }
    #[test]
    fn navigation_hover_references_and_rename_share_field_identity() {
        let index = SchemaIndex::new(SOURCE.into(), None);
        let position = point(SOURCE, "supplier.name", 9);
        let (symbol, _) = index.at(position).unwrap();
        assert_eq!((&*symbol.owner, &*symbol.name), ("Supplier", "name"));
        assert!(index.hover(position)["contents"]["value"]
            .as_str()
            .unwrap()
            .contains("Company display name"));
        assert!(!index.definition(position, "file:///a.plato").is_null());
        assert_eq!(
            index
                .references(position, "file:///a.plato", true)
                .as_array()
                .unwrap()
                .len(),
            6
        );
        let rename = index
            .rename(position, "companyName", "file:///a.plato", 4)
            .unwrap();
        assert_eq!(rename["documentChanges"][0]["textDocument"]["version"], 4);
        assert_eq!(
            rename["documentChanges"][0]["edits"]
                .as_array()
                .unwrap()
                .len(),
            6
        );
        assert!(index
            .rename(position, "true", "file:///a.plato", 4)
            .is_err());
    }
    #[test]
    fn reverse_relationships_resolve_to_the_relation_declaration() {
        let index = SchemaIndex::new(SOURCE.into(), None);
        let position = point(SOURCE, "appointments {", 3);
        let (symbol, _) = index.at(position).unwrap();
        assert_eq!(symbol.owner, "Supplier");
        assert_eq!(symbol.target, "Appointment");
        assert_eq!(
            index
                .references(position, "file:///a.plato", true)
                .as_array()
                .unwrap()
                .len(),
            2
        );
    }
    #[test]
    fn same_named_role_and_model_are_distinct() {
        let source="role Customer\nmodel Customer { fields { name String } reads { Customer } }\nmodel Sale { fields { buyer Customer } }\nview V for Customer { Customer { name } }";
        let index = SchemaIndex::new(source.into(), None);
        let role = point(source, "role Customer", 6);
        assert_eq!(
            index
                .references(role, "file:///a", true)
                .as_array()
                .unwrap()
                .len(),
            3
        );
        let model = point(source, "model Customer", 7);
        assert_eq!(
            index
                .references(model, "file:///a", true)
                .as_array()
                .unwrap()
                .len(),
            3
        );
    }
    #[test]
    fn inheritance_base_enum_values_and_utf16_are_resolved() {
        let source="# 🏠 café\nmodel Parent { fields { name String } }\nmodel Child extends Parent { fields { state enum { Open Closed } } actions { update close(id) { state = \"Closed\" } } }\nmodel Portal derives Child { fields { name String = base.name } }";
        let index = SchemaIndex::new(source.into(), None);
        assert_eq!(
            index.at(point(source, "base.name", 6)).unwrap().0.owner,
            "Parent"
        );
        let value = point(source, "\"Closed\"", 2);
        assert_eq!(index.at(value).unwrap().0.category, "value");
        assert_eq!(
            index
                .references(value, "file:///a", true)
                .as_array()
                .unwrap()
                .len(),
            2
        );
        for offset in source.char_indices().map(|(i, _)| i) {
            assert_eq!(
                position_offset(source, source_position(source, offset)),
                offset
            );
        }
    }
    #[test]
    fn completion_follows_relationship_paths_in_incomplete_input() {
        let source = SOURCE.replace("supplier.name ==", "supplier.na ==");
        let index = SchemaIndex::new(source.clone(), None);
        let items = index.completions(point(&source, "supplier.na", 11));
        assert!(items
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["label"] == "name"));
        assert!(!items
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["label"] == "Other"));
    }
    #[test]
    fn rename_refuses_collisions_and_generated_fields() {
        let index = SchemaIndex::new(SOURCE.into(), None);
        assert!(index
            .rename(point(SOURCE, "with (name", 7), "id", "file:///a", 1)
            .is_err());
        assert!(index
            .rename(point(SOURCE, "edit(id", 6), "key", "file:///a", 1)
            .is_err());
    }
}
