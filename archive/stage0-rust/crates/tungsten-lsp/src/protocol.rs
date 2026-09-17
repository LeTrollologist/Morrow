use crate::json::Json;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

impl Position {
    pub fn new(line: u32, character: u32) -> Self {
        Self { line, character }
    }

    pub fn to_json(&self) -> Json {
        let mut map = HashMap::new();
        map.insert("line".into(), Json::Number(self.line as f64));
        map.insert("character".into(), Json::Number(self.character as f64));
        Json::Object(map)
    }

    pub fn from_json(json: &Json) -> Option<Self> {
        let line = json.get("line")?.as_i64()? as u32;
        let character = json.get("character")?.as_i64()? as u32;
        Some(Self::new(line, character))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

impl Range {
    pub fn new(start: Position, end: Position) -> Self {
        Self { start, end }
    }

    pub fn to_json(&self) -> Json {
        let mut map = HashMap::new();
        map.insert("start".into(), self.start.to_json());
        map.insert("end".into(), self.end.to_json());
        Json::Object(map)
    }
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub range: Range,
    pub severity: u32, // 1 = Error, 2 = Warning
    pub message: String,
    pub source: String,
}

impl Diagnostic {
    pub fn to_json(&self) -> Json {
        let mut map = HashMap::new();
        map.insert("range".into(), self.range.to_json());
        map.insert("severity".into(), Json::Number(self.severity as f64));
        map.insert("message".into(), Json::String(self.message.clone()));
        map.insert("source".into(), Json::String(self.source.clone()));
        Json::Object(map)
    }
}

#[derive(Debug, Clone)]
pub struct TextEdit {
    pub range: Range,
    pub new_text: String,
}

impl TextEdit {
    pub fn to_json(&self) -> Json {
        let mut map = HashMap::new();
        map.insert("range".into(), self.range.to_json());
        map.insert("newText".into(), Json::String(self.new_text.clone()));
        Json::Object(map)
    }
}
