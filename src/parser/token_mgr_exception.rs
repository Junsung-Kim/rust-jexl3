// port of: org.apache.commons.jexl3.parser.TokenMgrException
use super::string_parser;

pub const LEXICAL_ERROR: i32 = 0;
pub const STATIC_LEXER_ERROR: i32 = 1;
pub const INVALID_LEXICAL_STATE: i32 = 2;
pub const LOOP_DETECTED: i32 = 3;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TokenMgrException {
    error_code: i32,
    message: Option<String>,
    state: i32,
    current: u16,
    after: Vec<u16>,
    eof: bool,
    line: i32,
    column: i32,
}

impl TokenMgrException {
    // port of: TokenMgrException(String, int)
    pub fn with_message(message: &str, reason: i32) -> Self {
        TokenMgrException {
            error_code: reason,
            message: Some(message.to_string()),
            state: 0,
            current: 0,
            after: Vec::new(),
            eof: false,
            line: 0,
            column: 0,
        }
    }

    // port of: TokenMgrException(boolean, int, int, int, String, int, int)
    pub fn new(eof: bool, lex_state: i32, line: i32, column: i32, after: Vec<u16>, cur_char: i32, reason: i32) -> Self {
        TokenMgrException {
            error_code: reason,
            message: None,
            state: lex_state,
            current: cur_char as u16,
            after,
            eof,
            line,
            column,
        }
    }

    // port of: TokenMgrException.getMessage
    pub fn get_message(&self) -> Vec<u16> {
        if let Some(m) = &self.message {
            return m.encode_utf16().collect();
        }
        let mut msg: Vec<u16> = format!("Lexical error at line {}, column {}.  Encountered: ", self.line, self.column)
            .encode_utf16()
            .collect();
        if self.eof {
            msg.extend("<EOF> ".encode_utf16());
        } else {
            msg.extend(string_parser::escape_string(&[self.current], b'"' as u16));
            msg.extend(format!(" ({}), ", self.current).encode_utf16());
        }
        msg.extend("after : ".encode_utf16());
        msg.extend(string_parser::escape_string(&self.after, b'"' as u16));
        msg
    }

    pub fn get_error_code(&self) -> i32 {
        self.error_code
    }
    pub fn get_line(&self) -> i32 {
        self.line
    }
    pub fn get_column(&self) -> i32 {
        self.column
    }
    pub fn get_after(&self) -> &[u16] {
        &self.after
    }
    pub fn get_state(&self) -> i32 {
        self.state
    }
}
