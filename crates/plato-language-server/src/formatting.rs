use crate::{folding::parse_plato_tree, symbols::syntax_children};
use tree_sitter::Node;

fn tokens<'a>(node: Node<'a>, out: &mut Vec<Node<'a>>) {
    if node.child_count() == 0
        || matches!(
            node.kind(),
            "string" | "doc_string" | "comment" | "region_start" | "region_end"
        )
    {
        out.push(node);
    } else {
        for child in syntax_children(node) {
            tokens(child, out);
        }
    }
}

/// Format whitespace only: preserve comments, strings, line breaks and declaration order.
pub fn format_schema_source(source: &str, tab_size: u32, spaces: bool) -> Result<String, String> {
    let tree = parse_plato_tree(source);
    if tree.root_node().has_error() {
        return Err("Fix syntax errors before formatting".into());
    }
    let mut leaves = vec![];
    tokens(tree.root_node(), &mut leaves);
    let unit = if spaces {
        " ".repeat(tab_size.clamp(1, 16) as usize)
    } else {
        "\t".into()
    };
    let newline = if source.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let mut output = Vec::new();
    let mut depth = 0usize;
    for (row, line) in source.lines().enumerate() {
        // The contents of multiline strings are data, including indentation.
        if leaves.iter().any(|n| {
            n.kind() == "doc_string" && n.start_position().row < row && n.end_position().row >= row
        }) {
            output.push(line.to_owned());
            continue;
        }
        let row_tokens: Vec<_> = leaves
            .iter()
            .filter(|n| n.start_position().row == row)
            .collect();
        if row_tokens.is_empty() {
            output.push(String::new());
            continue;
        }
        let first = row_tokens[0];
        let leading_close = usize::from(matches!(first.kind(), "}" | ")" | "]"));
        let mut formatted = unit.repeat(depth.saturating_sub(leading_close));
        let mut previous: Option<Node> = None;
        for node in row_tokens {
            if let Some(prev) = previous {
                let no_space = matches!(node.kind(), "," | "." | ":" | "?" | "!" | ")" | "]")
                    || matches!(prev.kind(), "." | "@" | "(" | "[")
                    || node.kind() == "("
                    || node.kind() == "[";
                if !no_space {
                    formatted.push(' ');
                }
            }
            let value = &source[node.byte_range()];
            formatted.push_str(
                value
                    .split('\n')
                    .next()
                    .unwrap_or("")
                    .trim_end_matches('\r'),
            );
            match node.kind() {
                "{" | "(" | "[" => depth += 1,
                "}" | ")" | "]" => depth = depth.saturating_sub(1),
                _ => {}
            }
            previous = Some(*node);
        }
        output.push(formatted);
    }
    while output.last().is_some_and(String::is_empty) {
        output.pop();
    }
    Ok(output.join(newline) + newline)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn formatting_preserves_regions_comments_and_literal_contents() {
        let source="# region Models\n\n# Keep me\nmodel A {\nfields {\n  \"\"\"Hello\n    keep these spaces\nworld\"\"\"\n  name    String  # inline\n}\n}\n\n# endregion\n";
        let formatted = format_schema_source(source, 2, true).unwrap();
        assert!(formatted.contains("\n  fields {\n"));
        assert!(formatted.contains("\"\"\"Hello\n    keep these spaces\nworld\"\"\""));
        assert!(formatted.contains("    name String # inline"));
        assert!(formatted.starts_with("# region Models\n\n# Keep me"));
        assert_eq!(
            format_schema_source(&formatted, 2, true).unwrap(),
            formatted
        );
    }
    #[test]
    fn formatting_respects_tabs_crlf_and_refuses_broken_syntax() {
        let source = "model A {\r\nfields {\r\nx String\r\n}\r\n}\r\n";
        assert!(format_schema_source(source, 4, false)
            .unwrap()
            .contains("\r\n\t\tx String\r\n"));
        assert!(format_schema_source("model {", 2, true).is_err());
    }
}
