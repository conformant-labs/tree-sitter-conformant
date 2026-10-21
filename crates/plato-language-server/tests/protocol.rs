use lsp_server::Message;
use serde_json::{json, Value};
use std::io::{BufReader, Write};
use std::process::{Command, Stdio};

#[test]
fn folding_tracks_open_change_close_and_shutdown() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_plato-language-server"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    let mut send = |value: Value| {
        let message: Message = serde_json::from_value(value).unwrap();
        message.write(&mut input).unwrap();
        input.flush().unwrap();
    };
    let mut receive = || -> Value {
        loop {
            let message = Message::read(&mut output).unwrap().unwrap();
            if matches!(message, Message::Response(_)) {
                break serde_json::to_value(message).unwrap();
            }
        }
    };
    send(json!({"jsonrpc":"2.0", "id":1, "method":"initialize", "params":{"capabilities":{}}}));
    assert_eq!(
        receive()["result"]["capabilities"]["foldingRangeProvider"],
        true
    );
    send(json!({"jsonrpc":"2.0", "method":"initialized", "params":{}}));
    let uri = "file:///schema.plato";
    send(
        json!({"jsonrpc":"2.0", "method":"textDocument/didOpen", "params":{"textDocument":{"uri":uri,"languageId":"plato","version":1,"text":"# region One\nrole Staff\n# endregion\n"}}}),
    );
    let request = |id| json!({"jsonrpc":"2.0", "id":id, "method":"textDocument/foldingRange", "params":{"textDocument":{"uri":uri}}});
    send(request(2));
    assert_eq!(receive()["result"][0]["collapsedText"], "One");
    send(
        json!({"jsonrpc":"2.0", "method":"textDocument/didChange", "params":{"textDocument":{"uri":uri,"version":2}, "contentChanges":[{"text":"model A {\nfields {\nname String\n}\n}\n"}]}}),
    );
    send(request(3));
    assert_eq!(receive()["result"].as_array().unwrap().len(), 2);
    for (id, method, extra) in [
        (30, "textDocument/hover", json!({})),
        (31, "textDocument/definition", json!({})),
        (
            32,
            "textDocument/references",
            json!({"context":{"includeDeclaration":true}}),
        ),
        (33, "textDocument/prepareRename", json!({})),
        (34, "textDocument/rename", json!({"newName":"title"})),
        (35, "textDocument/completion", json!({})),
        (
            36,
            "textDocument/formatting",
            json!({"options":{"tabSize":2,"insertSpaces":true}}),
        ),
    ] {
        let mut params = json!({"textDocument":{"uri":uri},"position":{"line":2,"character":2}});
        params
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        send(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}));
        let response = receive();
        assert!(response.get("error").is_none(), "{method}: {response}");
        assert!(!response["result"].is_null(), "{method}: {response}");
    }

    send(json!({"jsonrpc":"2.0", "id":4, "method":"textDocument/codeAction", "params":{}}));
    assert_eq!(receive()["error"]["code"], -32601);
    send(json!({"jsonrpc":"2.0", "id":5, "method":"textDocument/foldingRange", "params":{}}));
    assert_eq!(receive()["error"]["code"], -32602);
    send(
        json!({"jsonrpc":"2.0", "method":"textDocument/didClose", "params":{"textDocument":{"uri":uri}}}),
    );
    send(request(6));
    assert_eq!(receive()["result"], json!([]));
    send(json!({"jsonrpc":"2.0", "id":7, "method":"shutdown", "params":null}));
    assert_eq!(receive()["result"], Value::Null);
    send(json!({"jsonrpc":"2.0", "method":"exit"}));
    drop(input);
    assert!(child.wait().unwrap().success());
}
