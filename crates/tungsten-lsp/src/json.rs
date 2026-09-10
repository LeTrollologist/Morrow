use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Json>),
    Object(HashMap<String, Json>),
}

impl Json {
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Object(map) => map.get(key),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::String(s) => Some(s.as_str()),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Json::Number(n) => Some(*n as i64),
            _ => None,
        }
    }

    pub fn to_json_string(&self) -> String {
        match self {
            Json::Null => "null".to_string(),
            Json::Bool(b) => b.to_string(),
            Json::Number(n) => {
                if n.fract() == 0.0 {
                    format!("{:.0}", n)
                } else {
                    format!("{}", n)
                }
            }
            Json::String(s) => {
                let escaped = s
                    .replace('\\', "\\\\")
                    .replace('"', "\\\"")
                    .replace('\n', "\\n")
                    .replace('\r', "\\r")
                    .replace('\t', "\\t");
                format!("\"{}\"", escaped)
            }
            Json::Array(arr) => {
                let items: Vec<String> = arr.iter().map(|item| item.to_json_string()).collect();
                format!("[{}]", items.join(","))
            }
            Json::Object(map) => {
                let entries: Vec<String> = map
                    .iter()
                    .map(|(k, v)| format!("\"{}\":{}", k, v.to_json_string()))
                    .collect();
                format!("{{{}}}", entries.join(","))
            }
        }
    }
}

pub struct JsonParser<'a> {
    chars: Vec<char>,
    cursor: usize,
    _marker: std::marker::PhantomData<&'a str>,
}

impl<'a> JsonParser<'a> {
    pub fn parse(input: &'a str) -> Result<Json, String> {
        let mut parser = Self {
            chars: input.chars().collect(),
            cursor: 0,
            _marker: std::marker::PhantomData,
        };
        parser.skip_whitespace();
        let val = parser.parse_value()?;
        parser.skip_whitespace();
        Ok(val)
    }

    fn skip_whitespace(&mut self) {
        while self.cursor < self.chars.len() && self.chars[self.cursor].is_whitespace() {
            self.cursor += 1;
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.cursor).copied()
    }

    fn advance(&mut self) -> Option<char> {
        if self.cursor < self.chars.len() {
            let ch = self.chars[self.cursor];
            self.cursor += 1;
            Some(ch)
        } else {
            None
        }
    }

    fn parse_value(&mut self) -> Result<Json, String> {
        self.skip_whitespace();
        match self.peek() {
            Some('{') => self.parse_object(),
            Some('[') => self.parse_array(),
            Some('"') => self.parse_string().map(Json::String),
            Some('t') | Some('f') => self.parse_bool(),
            Some('n') => self.parse_null(),
            Some(c) if c.is_ascii_digit() || c == '-' => self.parse_number(),
            Some(other) => Err(format!("Unexpected character '{}' at offset {}", other, self.cursor)),
            None => Err("Unexpected end of JSON input".to_string()),
        }
    }

    fn parse_object(&mut self) -> Result<Json, String> {
        self.advance(); // consume '{'
        self.skip_whitespace();
        let mut map = HashMap::new();

        if let Some('}') = self.peek() {
            self.advance();
            return Ok(Json::Object(map));
        }

        loop {
            self.skip_whitespace();
            let key = self.parse_string()?;
            self.skip_whitespace();
            if self.advance() != Some(':') {
                return Err(format!("Expected ':' after key at offset {}", self.cursor));
            }
            let val = self.parse_value()?;
            map.insert(key, val);

            self.skip_whitespace();
            match self.advance() {
                Some(',') => continue,
                Some('}') => break,
                other => return Err(format!("Expected ',' or '}}' in object, got {:?}", other)),
            }
        }

        Ok(Json::Object(map))
    }

    fn parse_array(&mut self) -> Result<Json, String> {
        self.advance(); // consume '['
        self.skip_whitespace();
        let mut arr = Vec::new();

        if let Some(']') = self.peek() {
            self.advance();
            return Ok(Json::Array(arr));
        }

        loop {
            let val = self.parse_value()?;
            arr.push(val);
            self.skip_whitespace();
            match self.advance() {
                Some(',') => continue,
                Some(']') => break,
                other => return Err(format!("Expected ',' or ']' in array, got {:?}", other)),
            }
        }

        Ok(Json::Array(arr))
    }

    fn parse_string(&mut self) -> Result<String, String> {
        if self.advance() != Some('"') {
            return Err("Expected '\"'".to_string());
        }
        let mut s = String::new();
        while let Some(ch) = self.advance() {
            if ch == '"' {
                return Ok(s);
            } else if ch == '\\' {
                match self.advance() {
                    Some('"') => s.push('"'),
                    Some('\\') => s.push('\\'),
                    Some('/') => s.push('/'),
                    Some('b') => s.push('\x08'),
                    Some('f') => s.push('\x0c'),
                    Some('n') => s.push('\n'),
                    Some('r') => s.push('\r'),
                    Some('t') => s.push('\t'),
                    Some(other) => s.push(other),
                    None => return Err("Unterminated escape sequence".to_string()),
                }
            } else {
                s.push(ch);
            }
        }
        Err("Unterminated string literal".to_string())
    }

    fn parse_bool(&mut self) -> Result<Json, String> {
        let mut word = String::new();
        while let Some(c) = self.peek() {
            if c.is_ascii_alphabetic() {
                word.push(c);
                self.advance();
            } else {
                break;
            }
        }
        match word.as_str() {
            "true" => Ok(Json::Bool(true)),
            "false" => Ok(Json::Bool(false)),
            _ => Err(format!("Unknown literal '{}'", word)),
        }
    }

    fn parse_null(&mut self) -> Result<Json, String> {
        let mut word = String::new();
        while let Some(c) = self.peek() {
            if c.is_ascii_alphabetic() {
                word.push(c);
                self.advance();
            } else {
                break;
            }
        }
        if word == "null" {
            Ok(Json::Null)
        } else {
            Err(format!("Unknown literal '{}'", word))
        }
    }

    fn parse_number(&mut self) -> Result<Json, String> {
        let mut s = String::new();
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() || c == '-' || c == '+' || c == '.' || c == 'e' || c == 'E' {
                s.push(c);
                self.advance();
            } else {
                break;
            }
        }
        let num: f64 = s.parse().map_err(|e| format!("Invalid number '{}': {}", s, e))?;
        Ok(Json::Number(num))
    }
}
