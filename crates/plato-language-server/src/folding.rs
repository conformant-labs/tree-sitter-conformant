use lsp_types::{FoldingRange, FoldingRangeKind};
use tree_sitter::{Node, Parser};
use tree_sitter_language::LanguageFn;

extern "C" {
    fn tree_sitter_plato() -> *const ();
}

/// Parse editor syntax using the bundled Plato grammar.
pub fn parse_plato_tree(source: &str) -> tree_sitter::Tree {
    let mut parser = Parser::new();
    let language = unsafe { LanguageFn::from_raw(tree_sitter_plato) };
    parser
        .set_language(&language.into())
        .expect("bundled Plato grammar");
    parser.parse(source, None).expect("Plato parse")
}

/// Collect region and structural folding ranges, independent of indentation.
pub fn plato_folding_ranges(source: &str) -> Vec<FoldingRange> {
    let tree = parse_plato_tree(source);
    let mut ranges = Vec::new();
    collect_folding_ranges(tree.root_node(), source, &mut ranges);
    ranges.sort_by_key(|range| (range.start_line, std::cmp::Reverse(range.end_line)));
    ranges.dedup_by_key(|range| (range.start_line, range.end_line));
    ranges
}

fn collect_folding_ranges(node: Node, source: &str, ranges: &mut Vec<FoldingRange>) {
    let mut cursor = node.walk();
    let children: Vec<_> = node.children(&mut cursor).collect();
    let region = node.kind() == "region";
    let doc = node.kind() == "doc_string";
    // Matched direct delimiters identify models, blocks, enums and multiline
    // argument lists without mistaking braces in strings/comments for syntax.
    let delimiters = children.iter().find_map(|open| {
        let close_kind = match open.kind() {
            "{" => "}",
            "(" => ")",
            "[" => "]",
            _ => return None,
        };
        children
            .iter()
            .rev()
            .find(|close| close.kind() == close_kind && !close.is_missing())
            .map(|close| (*open, *close))
    });
    let bounds = if region {
        children
            .iter()
            .find(|child| child.kind() == "region_end" && !child.is_missing())
            .map(|end| {
                (
                    node.start_position().row,
                    end.start_position().row,
                    Some(FoldingRangeKind::Region),
                )
            })
    } else if doc {
        Some((
            node.start_position().row,
            node.end_position().row,
            Some(FoldingRangeKind::Comment),
        ))
    } else {
        delimiters
            .map(|(open, close)| (open.start_position().row, close.start_position().row, None))
    };
    if let Some((start, closing, kind)) = bounds {
        // Keep the closing delimiter/marker visible for line-only clients.
        if closing > start + 1 {
            ranges.push(FoldingRange {
                start_line: start as u32,
                start_character: None,
                end_line: (closing - 1) as u32,
                end_character: None,
                kind,
                collapsed_text: if region {
                    children
                        .iter()
                        .find(|child| child.kind() == "region_start")
                        .and_then(|start| start.child_by_field_name("name"))
                        .and_then(|name| name.utf8_text(source.as_bytes()).ok())
                        .map(str::to_owned)
                } else {
                    None
                },
            });
        }
    }
    for child in children {
        collect_folding_ranges(child, source, ranges);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unindented_nested_regions_keep_structural_folds() {
        let text = "# region Outer\n\n# region Inner\nmodel Product {\nfields {\nname String\n}\n}\n# endregion\n# endregion\n";
        let ranges = plato_folding_ranges(text);
        let bounds: Vec<_> = ranges.iter().map(|r| (r.start_line, r.end_line)).collect();
        assert_eq!(bounds, [(0, 8), (2, 7), (3, 6), (4, 5)]);
        assert_eq!(ranges[0].collapsed_text.as_deref(), Some("Outer"));
        assert_eq!(ranges[1].kind, Some(FoldingRangeKind::Region));
    }

    #[test]
    fn strings_and_comments_do_not_create_fake_folds() {
        let text = "model A {\nfields {\nname String\n}\n}\n# {\n# }\n\"\"\"# region Fake\n{\n# endregion\n}\"\"\"\nrole Staff\n";
        let ranges = plato_folding_ranges(text);
        assert!(ranges
            .iter()
            .all(|r| r.kind != Some(FoldingRangeKind::Region)));
        assert_eq!(
            ranges
                .iter()
                .filter(|r| r.kind == Some(FoldingRangeKind::Comment))
                .count(),
            1
        );
    }

    #[test]
    fn incomplete_regions_preserve_complete_model_folds() {
        let text = "# region Editing\nmodel A {\nfields {\nname String\n}\n}\n";
        let ranges = plato_folding_ranges(text);
        assert!(ranges.iter().any(|r| r.start_line == 1 && r.end_line == 4));
        assert!(ranges.iter().all(|r| r.end_line < 6));
    }

    #[test]
    fn crlf_unicode_and_single_line_nodes() {
        let ranges = plato_folding_ranges(
            "#region Café 🏠\r\nrole Staff\r\n#endregion\r\nmodel A { fields { name String } }\r\n",
        );
        assert_eq!(ranges.len(), 1);
        assert_eq!((ranges[0].start_line, ranges[0].end_line), (0, 1));
        assert_eq!(ranges[0].collapsed_text.as_deref(), Some("Café 🏠"));
    }
}
