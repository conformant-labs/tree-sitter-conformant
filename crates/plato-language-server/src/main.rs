mod folding;

use lsp_server::{Connection, ErrorCode, Message, Response};
use lsp_types::{
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    FoldingRangeParams,
};
use serde_json::json;
use std::collections::HashMap;

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (connection, threads) = Connection::stdio();
    connection.initialize(json!({
        "textDocumentSync": { "openClose": true, "change": 1 },
        "foldingRangeProvider": true
    }))?;
    let mut documents = HashMap::<String, String>::new();
    for message in &connection.receiver {
        match message {
            Message::Request(request) => {
                if connection.handle_shutdown(&request)? {
                    break;
                }
                let response = if request.method == "textDocument/foldingRange" {
                    match serde_json::from_value::<FoldingRangeParams>(request.params) {
                        Ok(params) => {
                            let ranges = documents
                                .get(params.text_document.uri.as_str())
                                .map(|text| folding::plato_folding_ranges(text))
                                .unwrap_or_default();
                            Response::new_ok(request.id, ranges)
                        }
                        Err(error) => Response::new_err(
                            request.id,
                            ErrorCode::InvalidParams as i32,
                            error.to_string(),
                        ),
                    }
                } else {
                    Response::new_err(
                        request.id,
                        ErrorCode::MethodNotFound as i32,
                        "Unsupported Plato request".into(),
                    )
                };
                connection.sender.send(Message::Response(response))?;
            }
            Message::Notification(notification) => match notification.method.as_str() {
                "textDocument/didOpen" => {
                    if let Ok(params) =
                        serde_json::from_value::<DidOpenTextDocumentParams>(notification.params)
                    {
                        documents.insert(
                            params.text_document.uri.to_string(),
                            params.text_document.text,
                        );
                    }
                }
                "textDocument/didChange" => {
                    if let Ok(params) =
                        serde_json::from_value::<DidChangeTextDocumentParams>(notification.params)
                    {
                        if let Some(change) = params.content_changes.into_iter().last() {
                            // FULL sync is advertised, so incremental ranges are not accepted.
                            if change.range.is_none() {
                                documents.insert(params.text_document.uri.to_string(), change.text);
                            }
                        }
                    }
                }
                "textDocument/didClose" => {
                    if let Ok(params) =
                        serde_json::from_value::<DidCloseTextDocumentParams>(notification.params)
                    {
                        documents.remove(params.text_document.uri.as_str());
                    }
                }
                "exit" => break,
                _ => {}
            },
            Message::Response(_) => {}
        }
    }
    drop(connection);
    threads.join()?;
    Ok(())
}
