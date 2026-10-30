//! The engine-owned strict JSON parser — the mod system's one dialect.
//!
//! Mod files are JSON, parsed by this parser and no other. It is
//! deliberately small and deliberately unforgiving, because strictness
//! is the sandbox: a mod file is data, and data that fails the schema
//! never reaches the game.
//!
//! Rules enforced here, before any schema sees the text:
//! - Input over [`MAX_FILE_BYTES`] is refused outright.
//! - Nesting deeper than [`MAX_DEPTH`] is refused.
//! - Arrays/objects longer than [`MAX_COLLECTION_LEN`] are refused.
//! - Duplicate keys in one object are errors, reported at the
//!   second key's line — silent last-wins would let a mod redefine
//!   its own fields and hide it.
//! - Only strict JSON: no comments, no trailing commas, no single
//!   quotes, no unquoted keys, no NaN/Infinity, no leading zeroes.
//! - Every value carries its source line, so schema errors can say
//!   `file:line: message` instead of pointing at the whole file.
//!
//! No dependency. The engine does not need general JSON — it needs
//! one schema, validated hard.

/// Refusal cap: a single mod file may not exceed 1 MiB. Mod files
/// are recipes, not data dumps; anything larger is a mistake or an
/// attempt, and either way it does not load.
pub const MAX_FILE_BYTES: usize = 1 << 20;
/// Refusal cap: nesting depth. Recipes are flat-ish; 32 is generous.
pub const MAX_DEPTH: usize = 32;
/// Refusal cap: elements in one array or entries in one object.
pub const MAX_COLLECTION_LEN: usize = 4096;

/// A parsed JSON value with its source line (1-based).
#[derive(Debug, Clone)]
pub struct Value {
    pub kind: Kind,
    pub line: usize,
}

/// The JSON value kinds. Objects preserve insertion order (a
/// `Vec`, not a map) so duplicate detection and deterministic
/// iteration are both exact.
#[derive(Debug, Clone)]
pub enum Kind {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Value>),
    Obj(Vec<(String, Value)>),
}

impl Value {
    pub fn as_obj(&self) -> Option<&[(String, Value)]> {
        match &self.kind {
            Kind::Obj(entries) => Some(entries),
            _ => None,
        }
    }
    pub fn as_arr(&self) -> Option<&[Value]> {
        match &self.kind {
            Kind::Arr(items) => Some(items),
            _ => None,
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        match &self.kind {
            Kind::Str(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_num(&self) -> Option<f64> {
        match &self.kind {
            Kind::Num(n) => Some(*n),
            _ => None,
        }
    }
    pub fn as_bool(&self) -> Option<bool> {
        match &self.kind {
            Kind::Bool(b) => Some(*b),
            _ => None,
        }
    }
    /// Look a field up by name. First match wins; the parser already
    /// refused duplicates, so there is at most one.
    pub fn field(&self, name: &str) -> Option<&Value> {
        self.as_obj()?
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v)
    }
    pub fn type_name(&self) -> &'static str {
        match &self.kind {
            Kind::Null => "null",
            Kind::Bool(_) => "boolean",
            Kind::Num(_) => "number",
            Kind::Str(_) => "string",
            Kind::Arr(_) => "array",
            Kind::Obj(_) => "object",
        }
    }
}

/// A parse failure: line (1-based) + plain reason.
#[derive(Debug, Clone)]
pub struct ParseError {
    pub line: usize,
    pub message: String,
}

impl ParseError {
    fn new(line: usize, message: impl Into<String>) -> ParseError {
        ParseError {
            line,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
    line: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }
    fn bump(&mut self) -> Option<u8> {
        let b = self.peek()?;
        self.pos += 1;
        if b == b'\n' {
            self.line += 1;
        }
        Some(b)
    }
    fn err(&self, message: impl Into<String>) -> ParseError {
        ParseError::new(self.line, message)
    }
    fn skip_ws(&mut self) {
        while let Some(b) = self.peek() {
            match b {
                b' ' | b'\t' | b'\r' | b'\n' => {
                    self.bump();
                }
                _ => break,
            }
        }
    }
    fn expect(&mut self, b: u8) -> Result<(), ParseError> {
        match self.bump() {
            Some(got) if got == b => Ok(()),
            Some(got) => Err(self.err(format!(
                "expected '{}', found '{}'",
                b as char, got as char
            ))),
            None => Err(self.err(format!("expected '{}', found end of input", b as char))),
        }
    }
    fn literal(&mut self, word: &str) -> Result<(), ParseError> {
        for want in word.bytes() {
            match self.bump() {
                Some(got) if got == want => {}
                _ => return Err(self.err(format!("invalid literal, expected '{word}'"))),
            }
        }
        Ok(())
    }

    fn parse_value(&mut self, depth: usize) -> Result<Value, ParseError> {
        if depth > MAX_DEPTH {
            return Err(self.err(format!(
                "nesting deeper than {MAX_DEPTH} is refused"
            )));
        }
        self.skip_ws();
        let line = self.line;
        let kind = match self.peek() {
            Some(b'{') => {
                self.bump();
                let mut entries: Vec<(String, Value)> = Vec::new();
                self.skip_ws();
                if self.peek() == Some(b'}') {
                    self.bump();
                } else {
                    loop {
                        self.skip_ws();
                        let key_line = self.line;
                        let key = self.parse_string()?;
                        if entries.iter().any(|(k, _)| k == &key) {
                            return Err(ParseError::new(
                                key_line,
                                format!("duplicate key '{key}'"),
                            ));
                        }
                        self.skip_ws();
                        self.expect(b':')?;
                        let value = self.parse_value(depth + 1)?;
                        entries.push((key, value));
                        if entries.len() > MAX_COLLECTION_LEN {
                            return Err(self.err(format!(
                                "object longer than {MAX_COLLECTION_LEN} entries is refused"
                            )));
                        }
                        self.skip_ws();
                        match self.bump() {
                            Some(b',') => continue,
                            Some(b'}') => break,
                            _ => {
                                return Err(self.err(
                                    "expected ',' or '}' in object",
                                ))
                            }
                        }
                    }
                }
                Kind::Obj(entries)
            }
            Some(b'[') => {
                self.bump();
                let mut items: Vec<Value> = Vec::new();
                self.skip_ws();
                if self.peek() == Some(b']') {
                    self.bump();
                } else {
                    loop {
                        let value = self.parse_value(depth + 1)?;
                        items.push(value);
                        if items.len() > MAX_COLLECTION_LEN {
                            return Err(self.err(format!(
                                "array longer than {MAX_COLLECTION_LEN} items is refused"
                            )));
                        }
                        self.skip_ws();
                        match self.bump() {
                            Some(b',') => continue,
                            Some(b']') => break,
                            _ => {
                                return Err(self.err(
                                    "expected ',' or ']' in array",
                                ))
                            }
                        }
                    }
                }
                Kind::Arr(items)
            }
            Some(b'"') => Kind::Str(self.parse_string()?),
            Some(b't') => {
                self.literal("true")?;
                Kind::Bool(true)
            }
            Some(b'f') => {
                self.literal("false")?;
                Kind::Bool(false)
            }
            Some(b'n') => {
                self.literal("null")?;
                Kind::Null
            }
            Some(b'-') | Some(b'0'..=b'9') => Kind::Num(self.parse_number()?),
            Some(b) => {
                return Err(self.err(format!(
                    "unexpected character '{}' (strict JSON only: no comments, \
                     no trailing commas, no single quotes)",
                    b as char
                )))
            }
            None => return Err(self.err("unexpected end of input")),
        };
        Ok(Value { kind, line })
    }

    fn parse_string(&mut self) -> Result<String, ParseError> {
        self.expect(b'"')?;
        let mut out = String::new();
        loop {
            match self.bump() {
                None => return Err(self.err("unterminated string")),
                Some(b'"') => break,
                Some(b'\\') => match self.bump() {
                    Some(b'"') => out.push('"'),
                    Some(b'\\') => out.push('\\'),
                    Some(b'/') => out.push('/'),
                    Some(b'b') => out.push('\u{0008}'),
                    Some(b'f') => out.push('\u{000C}'),
                    Some(b'n') => out.push('\n'),
                    Some(b'r') => out.push('\r'),
                    Some(b't') => out.push('\t'),
                    Some(b'u') => {
                        let hi = self.parse_hex4()?;
                        if (0xD800..0xDC00).contains(&hi) {
                            // High surrogate: a low surrogate must follow.
                            if self.bump() != Some(b'\\') || self.bump() != Some(b'u') {
                                return Err(self.err(
                                    "unpaired high surrogate in string escape",
                                ));
                            }
                            let lo = self.parse_hex4()?;
                            if !(0xDC00..0xE000).contains(&lo) {
                                return Err(self.err(
                                    "unpaired high surrogate in string escape",
                                ));
                            }
                            let c = 0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00);
                            out.push(char::from_u32(c).unwrap_or('\u{FFFD}'));
                        } else if (0xDC00..0xE000).contains(&hi) {
                            return Err(self.err(
                                "unpaired low surrogate in string escape",
                            ));
                        } else {
                            out.push(char::from_u32(hi).unwrap_or('\u{FFFD}'));
                        }
                    }
                    _ => return Err(self.err("invalid string escape")),
                },
                Some(b) if b < 0x20 => {
                    return Err(self.err("unescaped control character in string"))
                }
                Some(b) => {
                    // Copy UTF-8 through byte by byte; the input was
                    // validated as UTF-8 before parsing started.
                    out.push(b as char);
                    // Multi-byte sequences: the bytes above 0x7F arrive
                    // one at a time and each is pushed as a char, which
                    // would corrupt non-ASCII text — so instead, collect
                    // the full sequence here.
                    if b >= 0x80 {
                        // Rewind the mistake: pop the pushed char and
                        // decode the sequence properly.
                        out.pop();
                        let need = if b >= 0xF0 {
                            3
                        } else if b >= 0xE0 {
                            2
                        } else {
                            1
                        };
                        let mut seq = vec![b];
                        for _ in 0..need {
                            match self.bump() {
                                Some(c) => seq.push(c),
                                None => {
                                    return Err(self.err(
                                        "truncated UTF-8 in string",
                                    ))
                                }
                            }
                        }
                        match std::str::from_utf8(&seq) {
                            Ok(s) => out.push_str(s),
                            Err(_) => {
                                return Err(self.err("invalid UTF-8 in string"))
                            }
                        }
                    }
                }
            }
        }
        Ok(out)
    }

    fn parse_hex4(&mut self) -> Result<u32, ParseError> {
        let mut v: u32 = 0;
        for _ in 0..4 {
            let d = match self.bump() {
                Some(b'0'..=b'9') => (self.bytes[self.pos - 1] - b'0') as u32,
                Some(b'a'..=b'f') => (self.bytes[self.pos - 1] - b'a' + 10) as u32,
                Some(b'A'..=b'F') => (self.bytes[self.pos - 1] - b'A' + 10) as u32,
                _ => return Err(self.err("invalid \\u escape")),
            };
            v = v * 16 + d;
        }
        Ok(v)
    }

    fn parse_number(&mut self) -> Result<f64, ParseError> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.bump();
        }
        match self.peek() {
            Some(b'0') => {
                self.bump();
            }
            Some(b'1'..=b'9') => {
                while matches!(self.peek(), Some(b'0'..=b'9')) {
                    self.bump();
                }
            }
            _ => return Err(self.err("invalid number")),
        }
        if self.peek() == Some(b'.') {
            self.bump();
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(self.err("invalid number: digit expected after '.'"));
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.bump();
            }
        }
        if matches!(self.peek(), Some(b'e') | Some(b'E')) {
            self.bump();
            if matches!(self.peek(), Some(b'+') | Some(b'-')) {
                self.bump();
            }
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(self.err("invalid number: digit expected in exponent"));
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.bump();
            }
        }
        let text = std::str::from_utf8(&self.bytes[start..self.pos])
            .map_err(|_| self.err("invalid number encoding"))?;
        text.parse::<f64>()
            .map_err(|_| self.err("number out of range"))
            .and_then(|n| {
                if n.is_finite() {
                    Ok(n)
                } else {
                    Err(self.err("number out of range"))
                }
            })
    }
}

/// Parse a mod file's text. `text` must already be UTF-8 (read as a
/// String by the caller); non-UTF-8 bytes fail before this point.
pub fn parse(text: &str) -> Result<Value, ParseError> {
    if text.len() > MAX_FILE_BYTES {
        return Err(ParseError::new(
            1,
            format!("file larger than {MAX_FILE_BYTES} bytes is refused"),
        ));
    }
    let mut p = Parser {
        bytes: text.as_bytes(),
        pos: 0,
        line: 1,
    };
    let value = p.parse_value(0)?;
    p.skip_ws();
    if p.pos != p.bytes.len() {
        return Err(p.err("trailing content after the top-level value"));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_object_and_numbers() {
        let v = parse(r#"{"a": 1, "b": [true, null, "x"]}"#).unwrap();
        assert_eq!(v.field("a").unwrap().as_num(), Some(1.0));
        assert_eq!(v.field("b").unwrap().as_arr().unwrap().len(), 3);
    }

    #[test]
    fn duplicate_key_is_an_error() {
        let e = parse(r#"{"a": 1, "a": 2}"#).unwrap_err();
        assert!(e.message.contains("duplicate key"), "{}", e.message);
        assert_eq!(e.line, 1);
    }

    #[test]
    fn trailing_comma_is_an_error() {
        assert!(parse(r#"{"a": 1,}"#).is_err());
        assert!(parse(r#"[1, 2,]"#).is_err());
    }

    #[test]
    fn deep_nesting_is_refused() {
        let mut s = String::new();
        for _ in 0..40 {
            s.push('[');
        }
        assert!(parse(&s).is_err());
    }

    #[test]
    fn line_numbers_track() {
        let v = parse("{\n  \"a\": 1,\n  \"b\": 2\n}").unwrap();
        assert_eq!(v.field("b").unwrap().line, 3);
    }
}

