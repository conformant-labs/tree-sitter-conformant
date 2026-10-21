use crate::symbols::{source_range, syntax_children, SchemaIndex};
use serde_json::{json, Value};
use tree_sitter::Node;

/// Prefer runtime validation; Tree-sitter errors are a fallback when it is unavailable.
pub fn schema_diagnostics(index: &SchemaIndex, validation: Result<&Value, &str>) -> Vec<Value> {
    match validation {
        Ok(result)=>result["errors"].as_array().into_iter().flatten().map(|error|{
            let line=error["line"].as_u64().unwrap_or(1).saturating_sub(1) as u32;
            let column=error["column"].as_u64().unwrap_or(1).saturating_sub(1) as u32;
            json!({"range":{"start":{"line":line,"character":column},"end":{"line":line,"character":column}},"severity":1,"source":"Plato runtime","message":error["message"]})
        }).collect(),
        Err(message)=>{
            let mut errors=vec![json!({"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":0}},"severity":2,"source":"Plato","message":message})];
            collect_syntax_errors(index.tree.root_node(),index,&mut errors);
            errors
        }
    }
}
fn collect_syntax_errors(node: Node, index: &SchemaIndex, errors: &mut Vec<Value>) {
    if node.is_error() || node.is_missing() {
        errors.push(json!({"range":source_range(&index.source,node.byte_range()),"severity":1,"source":"Plato syntax","message":if node.is_missing(){format!("Expected {}",node.kind())}else{"Unrecognised Plato syntax".into()}}));
        return;
    }
    for child in syntax_children(node) {
        collect_syntax_errors(child, index, errors);
    }
}
