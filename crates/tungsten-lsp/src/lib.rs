pub mod json;
pub mod protocol;
pub mod server;

pub use server::LspServer;

#[cfg(test)]
mod tests {
    use super::*;
    use json::JsonParser;

    #[test]
    fn test_lsp_initialize() {
        let mut server = LspServer::new();
        let init_req = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#;
        let resp = server.handle_message(init_req).expect("should handle initialize");
        let parsed = JsonParser::parse(&resp).expect("valid json response");
        assert_eq!(parsed.get("id").unwrap().as_i64(), Some(1));
        assert!(parsed.get("result").unwrap().get("capabilities").is_some());
    }

    #[test]
    fn test_lsp_diagnostics_on_open() {
        let mut server = LspServer::new();
        let code_with_error = r#"
        type Health = u8(0..=100);
        fn test_bad() {
            let bad = 150 as Health;
        }
        "#;
        let did_open_msg = format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"file:///test.tg","text":"{}"}}}}}}"#,
            code_with_error.replace('\n', "\\n").replace('"', "\\\"")
        );

        let notif = server.handle_message(&did_open_msg).expect("should emit diagnostics notification");
        let parsed = JsonParser::parse(&notif).expect("valid json notif");
        assert_eq!(parsed.get("method").unwrap().as_str(), Some("textDocument/publishDiagnostics"));
        let diags = parsed.get("params").unwrap().get("diagnostics").unwrap();
        match diags {
            json::Json::Array(arr) => {
                assert!(!arr.is_empty(), "Should publish diagnostics for refinement violation");
                let first_msg = arr[0].get("message").unwrap().as_str().unwrap();
                assert!(first_msg.contains("150 is outside allowable range"));
            }
            _ => panic!("Expected array of diagnostics"),
        }
    }

    #[test]
    fn test_lsp_hover() {
        let mut server = LspServer::new();
        let code = "type Health = u8(0..=100);\nfn main() {}\n";
        let did_open = format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"file:///main.tg","text":"{}"}}}}}}"#,
            code.replace('\n', "\\n")
        );
        server.handle_message(&did_open);

        let hover_req = r#"{"jsonrpc":"2.0","id":2,"method":"textDocument/hover","params":{"textDocument":{"uri":"file:///main.tg"},"position":{"line":0,"character":6}}}"#;
        let resp = server.handle_message(hover_req).expect("should handle hover");
        let parsed = JsonParser::parse(&resp).expect("valid hover response");
        let contents = parsed.get("result").unwrap().get("contents").unwrap().get("value").unwrap().as_str().unwrap();
        assert!(contents.contains("Health"));
        assert!(contents.contains("[0..=100]"));
    }
}
