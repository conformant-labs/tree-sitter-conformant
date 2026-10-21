mod diagnostics;
mod folding;
mod formatting;
mod runtime;
mod symbols;

use lsp_server::{Connection, ErrorCode, Message, Notification, Response};
use lsp_types::{
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams, Position,
};
use serde_json::{json, Value};
use std::{collections::HashMap, path::PathBuf};
use symbols::{source_position, source_range, SchemaIndex};

struct OpenSchema {
    index: SchemaIndex,
    version: i32,
    runtime_valid: Option<bool>,
}
struct PlatoServer {
    documents: HashMap<String, OpenSchema>,
    runtimes: HashMap<PathBuf, runtime::SchemaRuntime>,
    runtime_path: Option<PathBuf>,
}
impl PlatoServer {
    fn validate(&mut self, uri: &str, source: &str) -> Result<Value, String> {
        let directory=self.runtime_path.clone().or_else(|| url::Url::parse(uri).ok()?.to_file_path().ok().and_then(|p|runtime::find_schema_runtime(&p)))
            .ok_or("Runtime validation unavailable: open a Plato monorepo or set lsp.plato.initialization_options.schemaRuntimePath to packages/plato-schema")?;
        if !self.runtimes.contains_key(&directory) {
            self.runtimes.insert(
                directory.clone(),
                runtime::SchemaRuntime::start(&directory)?,
            );
        }
        let result = self.runtimes.get_mut(&directory).unwrap().validate(source);
        if result.is_err() {
            self.runtimes.remove(&directory);
        }
        result
    }
    fn update(
        &mut self,
        uri: String,
        source: String,
        version: i32,
        connection: &Connection,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if self
            .documents
            .get(&uri)
            .is_some_and(|doc| doc.version >= version)
        {
            return Ok(());
        }
        let validation = self.validate(&uri, &source);
        let index = SchemaIndex::new(
            source,
            validation.as_ref().ok().and_then(|v| v.get("schema")),
        );
        let errors =
            diagnostics::schema_diagnostics(&index, validation.as_ref().map_err(String::as_str));
        connection
            .sender
            .send(Message::Notification(Notification::new(
                "textDocument/publishDiagnostics".into(),
                json!({"uri":uri,"version":version,"diagnostics":errors}),
            )))?;
        let runtime_valid = validation
            .as_ref()
            .ok()
            .map(|result| result["errors"].as_array().is_some_and(Vec::is_empty));
        self.documents.insert(
            uri,
            OpenSchema {
                index,
                version,
                runtime_valid,
            },
        );
        Ok(())
    }
    fn request(&mut self, method: &str, params: Value) -> Result<Value, (i32, String)> {
        let invalid = |message: &str| (ErrorCode::InvalidParams as i32, message.to_owned());
        let supported = [
            "textDocument/foldingRange",
            "textDocument/completion",
            "textDocument/hover",
            "textDocument/definition",
            "textDocument/references",
            "textDocument/prepareRename",
            "textDocument/rename",
            "textDocument/formatting",
        ];
        if !supported.contains(&method) {
            return Err((
                ErrorCode::MethodNotFound as i32,
                "Unsupported Plato request".into(),
            ));
        }
        let uri = params["textDocument"]["uri"]
            .as_str()
            .ok_or_else(|| invalid("Missing document URI"))?;
        let Some(doc) = self.documents.get(uri) else {
            return Ok(
                if matches!(
                    method,
                    "textDocument/hover" | "textDocument/definition" | "textDocument/prepareRename"
                ) {
                    Value::Null
                } else {
                    json!([])
                },
            );
        };
        let position = if matches!(
            method,
            "textDocument/foldingRange" | "textDocument/formatting"
        ) {
            Position::default()
        } else {
            serde_json::from_value(params["position"].clone())
                .map_err(|_| invalid("Missing or invalid position"))?
        };
        let index = &doc.index;
        match method {
            "textDocument/foldingRange"=>Ok(json!(folding::plato_folding_ranges(&index.source))),
            "textDocument/hover"=>Ok(index.hover(position)),
            "textDocument/definition"=>Ok(index.definition(position,uri)),
            "textDocument/completion"=>Ok(index.completions(position)),
            "textDocument/references"=>Ok(index.references(position,uri,params["context"]["includeDeclaration"].as_bool().unwrap_or(false))),
            "textDocument/prepareRename"=>Ok(index.at(position).filter(|(s,_)|s.span.is_some()).map(|(s,u)|json!({"range":source_range(&index.source,u.span.clone()),"placeholder":s.name})).unwrap_or(Value::Null)),
            "textDocument/rename"=>{
                let name=params["newName"].as_str().ok_or_else(||invalid("Missing newName"))?;
                if doc.runtime_valid == Some(false) {
                    return Err((ErrorCode::InvalidRequest as i32, "Fix schema validation errors before renaming".into()));
                }
                let edit = index.rename(position,name,uri,doc.version).map_err(|e|(ErrorCode::InvalidRequest as i32,e))?;
                let validate_rename = doc.runtime_valid == Some(true);
                let mut candidate = index.source.clone();
                let mut replacements: Vec<(usize, usize)> = edit["documentChanges"][0]["edits"].as_array().unwrap().iter().map(|edit| {
                    let range: lsp_types::Range = serde_json::from_value(edit["range"].clone()).unwrap();
                    (symbols::position_offset(&index.source, range.start), symbols::position_offset(&index.source, range.end))
                }).collect();
                replacements.sort_unstable_by_key(|a| std::cmp::Reverse(a.0));
                for (start,end) in replacements { candidate.replace_range(start..end,name); }
                if validate_rename {
                    let result = self.validate(uri, &candidate).map_err(|error| (ErrorCode::InternalError as i32,error))?;
                    if let Some(error) = result["errors"].as_array().and_then(|errors|errors.first()) {
                        return Err((ErrorCode::InvalidRequest as i32, format!("Rename would invalidate the schema: {}", error["message"].as_str().unwrap_or("validation error"))));
                    }
                }
                Ok(edit)
            }
            "textDocument/formatting"=>{
                let tab_size=params["options"]["tabSize"].as_u64().ok_or_else(||invalid("Missing tabSize"))? as u32;
                let spaces=params["options"]["insertSpaces"].as_bool().ok_or_else(||invalid("Missing insertSpaces"))?;
                let formatted=formatting::format_schema_source(&index.source,tab_size,spaces).map_err(|e|invalid(&e))?;
                if formatted==index.source {return Ok(json!([]));}
                Ok(json!([{"range":{"start":{"line":0,"character":0},"end":source_position(&index.source,index.source.len())},"newText":formatted}]))
            }
            _=>unreachable!(),
        }
    }
}
fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (connection, threads) = Connection::stdio();
    let initialization=connection.initialize(json!({
        "positionEncoding":"utf-16","textDocumentSync":{"openClose":true,"change":1},"foldingRangeProvider":true,
        "completionProvider":{"triggerCharacters":[".","@"]},"hoverProvider":true,"definitionProvider":true,
        "referencesProvider":true,"renameProvider":{"prepareProvider":true},"documentFormattingProvider":true
    }))?;
    let mut server = PlatoServer {
        documents: HashMap::new(),
        runtimes: HashMap::new(),
        runtime_path: initialization["initializationOptions"]["schemaRuntimePath"]
            .as_str()
            .map(PathBuf::from),
    };
    for message in &connection.receiver {
        match message {
            Message::Request(request) => {
                if connection.handle_shutdown(&request)? {
                    break;
                }
                let response = match server.request(&request.method, request.params) {
                    Ok(value) => Response::new_ok(request.id, value),
                    Err((code, message)) => Response::new_err(request.id, code, message),
                };
                connection.sender.send(Message::Response(response))?;
            }
            Message::Notification(notification) => match notification.method.as_str() {
                "textDocument/didOpen" => {
                    if let Ok(params) =
                        serde_json::from_value::<DidOpenTextDocumentParams>(notification.params)
                    {
                        server.update(
                            params.text_document.uri.to_string(),
                            params.text_document.text,
                            params.text_document.version,
                            &connection,
                        )?;
                    }
                }
                "textDocument/didChange" => {
                    if let Ok(params) =
                        serde_json::from_value::<DidChangeTextDocumentParams>(notification.params)
                    {
                        if let Some(change) = params
                            .content_changes
                            .into_iter()
                            .last()
                            .filter(|c| c.range.is_none())
                        {
                            server.update(
                                params.text_document.uri.to_string(),
                                change.text,
                                params.text_document.version,
                                &connection,
                            )?;
                        }
                    }
                }
                "textDocument/didClose" => {
                    if let Ok(params) =
                        serde_json::from_value::<DidCloseTextDocumentParams>(notification.params)
                    {
                        server.documents.remove(params.text_document.uri.as_str());
                        connection
                            .sender
                            .send(Message::Notification(Notification::new(
                                "textDocument/publishDiagnostics".into(),
                                json!({"uri":params.text_document.uri,"diagnostics":[]}),
                            )))?;
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
