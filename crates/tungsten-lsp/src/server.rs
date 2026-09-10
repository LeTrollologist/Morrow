use std::collections::HashMap;
use std::io::{self, BufRead, Read, Write};

use crate::json::{Json, JsonParser};
use crate::protocol::{Diagnostic, Position, Range, TextEdit};
use tungsten_syntax::ast::Item;

pub struct LspServer {
    documents: HashMap<String, String>,
}

impl LspServer {
    pub fn new() -> Self {
        Self {
            documents: HashMap::new(),
        }
    }

    pub fn handle_message(&mut self, json_str: &str) -> Option<String> {
        let root = JsonParser::parse(json_str).ok()?;
        let method = root.get("method").and_then(|m| m.as_str());
        let id = root.get("id").cloned();

        match method {
            Some("initialize") => {
                let resp = self.make_initialize_response(id);
                Some(resp.to_json_string())
            }
            Some("initialized") => None,
            Some("textDocument/didOpen") => {
                if let Some(params) = root.get("params") {
                    if let Some(doc) = params.get("textDocument") {
                        if let (Some(uri), Some(text)) = (doc.get("uri").and_then(|u| u.as_str()), doc.get("text").and_then(|t| t.as_str())) {
                            self.documents.insert(uri.to_string(), text.to_string());
                            let notif = self.publish_diagnostics(uri, text);
                            return Some(notif.to_json_string());
                        }
                    }
                }
                None
            }
            Some("textDocument/didChange") => {
                if let Some(params) = root.get("params") {
                    if let Some(doc) = params.get("textDocument") {
                        if let Some(uri) = doc.get("uri").and_then(|u| u.as_str()) {
                            if let Some(changes) = params.get("contentChanges").and_then(|c| match c { Json::Array(a) => Some(a), _ => None }) {
                                if let Some(first_change) = changes.first() {
                                    if let Some(text) = first_change.get("text").and_then(|t| t.as_str()) {
                                        self.documents.insert(uri.to_string(), text.to_string());
                                        let notif = self.publish_diagnostics(uri, text);
                                        return Some(notif.to_json_string());
                                    }
                                }
                            }
                        }
                    }
                }
                None
            }
            Some("textDocument/hover") => {
                let id = id?;
                if let Some(params) = root.get("params") {
                    if let (Some(doc), Some(pos_json)) = (params.get("textDocument"), params.get("position")) {
                        if let (Some(uri), Some(pos)) = (doc.get("uri").and_then(|u| u.as_str()), Position::from_json(pos_json)) {
                            let resp = self.handle_hover(id, uri, pos);
                            return Some(resp.to_json_string());
                        }
                    }
                }
                let mut resp = HashMap::new();
                resp.insert("jsonrpc".into(), Json::String("2.0".into()));
                resp.insert("id".into(), id);
                resp.insert("result".into(), Json::Null);
                Some(Json::Object(resp).to_json_string())
            }
            Some("textDocument/formatting") => {
                let id = id?;
                if let Some(params) = root.get("params") {
                    if let Some(doc) = params.get("textDocument") {
                        if let Some(uri) = doc.get("uri").and_then(|u| u.as_str()) {
                            let resp = self.handle_formatting(id, uri);
                            return Some(resp.to_json_string());
                        }
                    }
                }
                let mut resp = HashMap::new();
                resp.insert("jsonrpc".into(), Json::String("2.0".into()));
                resp.insert("id".into(), id);
                resp.insert("result".into(), Json::Null);
                Some(Json::Object(resp).to_json_string())
            }
            Some("shutdown") => {
                let mut resp = HashMap::new();
                resp.insert("jsonrpc".into(), Json::String("2.0".into()));
                if let Some(req_id) = id {
                    resp.insert("id".into(), req_id);
                }
                resp.insert("result".into(), Json::Null);
                Some(Json::Object(resp).to_json_string())
            }
            _ => None,
        }
    }

    fn make_initialize_response(&self, id: Option<Json>) -> Json {
        let mut resp = HashMap::new();
        resp.insert("jsonrpc".into(), Json::String("2.0".into()));
        if let Some(req_id) = id {
            resp.insert("id".into(), req_id);
        }

        let mut caps = HashMap::new();
        caps.insert("textDocumentSync".into(), Json::Number(1.0)); // Full sync
        caps.insert("hoverProvider".into(), Json::Bool(true));
        caps.insert("documentFormattingProvider".into(), Json::Bool(true));

        let mut result = HashMap::new();
        result.insert("capabilities".into(), Json::Object(caps));
        resp.insert("result".into(), Json::Object(result));

        Json::Object(resp)
    }

    pub fn compute_diagnostics(&self, text: &str) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // 1. Syntax Parsing
        let ast = match tungsten_syntax::parse(text) {
            Ok(prog) => prog,
            Err(err_msg) => {
                diagnostics.push(Diagnostic {
                    range: Range::new(Position::new(0, 0), Position::new(0, 5)),
                    severity: 1, // Error
                    message: format!("Syntax error: {}", err_msg),
                    source: "tungsten".into(),
                });
                return diagnostics;
            }
        };

        // 2. Type Checking & Refinement/Effect Solver
        if let Err(type_errors) = tungsten_typeck::check(&ast) {
            for terr in type_errors {
                let start_line = if terr.span.line > 0 { terr.span.line as u32 - 1 } else { 0 };
                let start_col = if terr.span.column > 0 { terr.span.column as u32 - 1 } else { 0 };
                let end_col = start_col + 5;

                diagnostics.push(Diagnostic {
                    range: Range::new(Position::new(start_line, start_col), Position::new(start_line, end_col)),
                    severity: 1, // Error
                    message: terr.message,
                    source: "tungsten".into(),
                });
            }
        }

        diagnostics
    }

    fn publish_diagnostics(&self, uri: &str, text: &str) -> Json {
        let diags = self.compute_diagnostics(text);

        let mut params = HashMap::new();
        params.insert("uri".into(), Json::String(uri.to_string()));
        let diag_arr: Vec<Json> = diags.iter().map(|d| d.to_json()).collect();
        params.insert("diagnostics".into(), Json::Array(diag_arr));

        let mut notif = HashMap::new();
        notif.insert("jsonrpc".into(), Json::String("2.0".into()));
        notif.insert("method".into(), Json::String("textDocument/publishDiagnostics".into()));
        notif.insert("params".into(), Json::Object(params));

        Json::Object(notif)
    }

    fn handle_hover(&self, id: Json, uri: &str, pos: Position) -> Json {
        let text = self.documents.get(uri).cloned().unwrap_or_default();
        let word = Self::extract_word_at(&text, pos.line as usize, pos.character as usize);

        let hover_text = if let Some(ref w) = word {
            self.find_hover_info(&text, w)
        } else {
            None
        };

        let mut resp = HashMap::new();
        resp.insert("jsonrpc".into(), Json::String("2.0".into()));
        resp.insert("id".into(), id);

        if let Some(contents) = hover_text {
            let mut result = HashMap::new();
            let mut contents_obj = HashMap::new();
            contents_obj.insert("kind".into(), Json::String("markdown".into()));
            contents_obj.insert("value".into(), Json::String(contents));
            result.insert("contents".into(), Json::Object(contents_obj));
            resp.insert("result".into(), Json::Object(result));
        } else {
            resp.insert("result".into(), Json::Null);
        }

        Json::Object(resp)
    }

    fn extract_word_at(text: &str, target_line: usize, target_col: usize) -> Option<String> {
        let line = text.lines().nth(target_line)?;
        let chars: Vec<char> = line.chars().collect();
        if target_col >= chars.len() {
            return None;
        }

        let mut start = target_col;
        while start > 0 && (chars[start - 1].is_ascii_alphanumeric() || chars[start - 1] == '_') {
            start -= 1;
        }

        let mut end = target_col;
        while end < chars.len() && (chars[end].is_ascii_alphanumeric() || chars[end] == '_') {
            end += 1;
        }

        if start == end {
            None
        } else {
            Some(chars[start..end].iter().collect())
        }
    }

    fn find_hover_info(&self, text: &str, word: &str) -> Option<String> {
        let ast = tungsten_syntax::parse(text).ok()?;
        for item in &ast.items {
            match item {
                Item::TypeAlias(alias) if alias.name == word => {
                    let mut info = format!("```tungsten\ntype {} = ...\n```", alias.name);
                    match &alias.target {
                        tungsten_syntax::ast::TypeExpr::Refined { base, min, max, inclusive, .. } => {
                            let bound = if *inclusive { format!("[{}..={}]", min, max) } else { format!("[{}..{}]", min, max) };
                            info.push_str(&format!("\n\n**Refinement bound:** `{}` (base `{}`)", bound, base));
                        }
                        _ => {}
                    }
                    return Some(info);
                }
                Item::Struct(st) if st.name == word => {
                    let field_desc: Vec<String> = st.fields.iter().map(|f| format!("  {}: ...", f.name)).collect();
                    return Some(format!("```tungsten\nstruct {} {{\n{}\n}}\n```", st.name, field_desc.join("\n")));
                }
                Item::Fn(f) if f.name == word => {
                    let param_strs: Vec<String> = f.params.iter().map(|p| format!("{}: ...", p.name)).collect();
                    let mut sig = format!("```tungsten\nfn {}({})", f.name, param_strs.join(", "));
                    if !f.yields_effects.is_empty() {
                        sig.push_str(&format!(" yields [{}]", f.yields_effects.join(", ")));
                    }
                    sig.push_str("\n```");
                    if !f.yields_effects.is_empty() {
                        sig.push_str(&format!("\n\n*Algebraic Effect Row:* `{}`", f.yields_effects.join(", ")));
                    }
                    return Some(sig);
                }
                Item::Effect(eff) if eff.name == word => {
                    return Some(format!("```tungsten\neffect {}\n```\n\n*Algebraic Effect Capability*", eff.name));
                }
                _ => {}
            }
        }

        // Standard prelude types hover
        match word {
            "Percentage" => Some("```tungsten\ntype Percentage = u8(0..=100);\n```\n\n*Standard refinement:* Valid percentage [0..=100]".into()),
            "Port" => Some("```tungsten\ntype Port = u16(1..=65535);\n```\n\n*Standard refinement:* TCP/UDP port [1..=65535]".into()),
            "NonZeroU32" => Some("```tungsten\ntype NonZeroU32 = u32(1..=4294967295);\n```\n\n*Standard refinement:* Non-zero integer [1..=4294967295]".into()),
            "Db" => Some("```tungsten\neffect Db\n```\n\n*Algebraic Effect:* Database query capability".into()),
            "IOError" => Some("```tungsten\neffect IOError\n```\n\n*Algebraic Effect:* I/O error capability".into()),
            _ => None,
        }
    }

    fn handle_formatting(&self, id: Json, uri: &str) -> Json {
        let mut resp = HashMap::new();
        resp.insert("jsonrpc".into(), Json::String("2.0".into()));
        resp.insert("id".into(), id);

        if let Some(text) = self.documents.get(uri) {
            if let Ok(formatted) = tungsten_syntax::format_source(text) {
                let lines_count = text.lines().count().max(1) as u32;
                let last_line_len = text.lines().last().map(|l| l.len()).unwrap_or(0) as u32;

                let edit = TextEdit {
                    range: Range::new(Position::new(0, 0), Position::new(lines_count, last_line_len)),
                    new_text: formatted,
                };

                let edits = vec![edit.to_json()];
                resp.insert("result".into(), Json::Array(edits));
                return Json::Object(resp);
            }
        }

        resp.insert("result".into(), Json::Null);
        Json::Object(resp)
    }

    pub fn run_stdio(&mut self) -> io::Result<()> {
        let stdin = io::stdin();
        let mut stdout = io::stdout();
        let mut reader = io::BufReader::new(stdin.lock());

        loop {
            let mut line = String::new();
            let mut content_length: Option<usize> = None;

            loop {
                line.clear();
                let bytes = reader.read_line(&mut line)?;
                if bytes == 0 {
                    return Ok(()); // EOF
                }
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    break; // Headers ended
                }
                if trimmed.to_lowercase().starts_with("content-length:") {
                    let parts: Vec<&str> = trimmed.split(':').collect();
                    if parts.len() >= 2 {
                        if let Ok(len) = parts[1].trim().parse::<usize>() {
                            content_length = Some(len);
                        }
                    }
                }
            }

            if let Some(len) = content_length {
                let mut body = vec![0u8; len];
                reader.read_exact(&mut body)?;
                let body_str = String::from_utf8_lossy(&body);

                if let Some(response) = self.handle_message(&body_str) {
                    let header = format!("Content-Length: {}\r\n\r\n", response.len());
                    stdout.write_all(header.as_bytes())?;
                    stdout.write_all(response.as_bytes())?;
                    stdout.flush()?;
                }
            }
        }
    }
}
