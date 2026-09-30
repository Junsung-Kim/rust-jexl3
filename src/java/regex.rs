// port of: java.util.regex.Pattern (subset used by JEXL)
//! STUB - not implemented yet (TDD red phase).

pub const UNIX_LINES: i32 = 0x01;
pub const CASE_INSENSITIVE: i32 = 0x02;
pub const COMMENTS: i32 = 0x04;
pub const MULTILINE: i32 = 0x08;
pub const LITERAL: i32 = 0x10;
pub const DOTALL: i32 = 0x20;
pub const UNICODE_CASE: i32 = 0x40;
pub const CANON_EQ: i32 = 0x80;
pub const UNICODE_CHARACTER_CLASS: i32 = 0x100;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PatternSyntaxException {
    desc: String,
    pattern: String,
    index: i32,
}

impl PatternSyntaxException {
    pub fn get_message(&self) -> String { String::new() }
    pub fn get_description(&self) -> &str { &self.desc }
    pub fn get_index(&self) -> i32 { self.index }
    pub fn get_pattern(&self) -> &str { &self.pattern }
}

#[derive(Clone, Debug)]
pub enum JavaRegexError {
    Syntax(PatternSyntaxException),
    IllegalArgument(String),
    IndexOutOfBounds(String),
}

#[derive(Clone, Debug)]
pub struct Pattern {
    src: String,
}

impl std::fmt::Display for Pattern {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.src)
    }
}

impl Pattern {
    pub fn compile(regex: &str) -> Result<Pattern, PatternSyntaxException> {
        Pattern::compile_flags(regex, 0)
    }
    pub fn compile_flags(regex: &str, _flags: i32) -> Result<Pattern, PatternSyntaxException> {
        Ok(Pattern { src: regex.to_string() })
    }
    pub fn pattern(&self) -> &str { &self.src }
    pub fn matches(&self, _input: &str) -> bool { false }
    pub fn find(&self, _input: &str) -> bool { false }
    pub fn split(&self, input: &str, _limit: i32) -> Vec<String> { vec![input.to_string()] }
    pub fn replace_all(&self, input: &str, _r: &str) -> Result<String, JavaRegexError> {
        Ok(input.to_string())
    }
    pub fn replace_first(&self, input: &str, _r: &str) -> Result<String, JavaRegexError> {
        Ok(input.to_string())
    }
}

pub fn string_matches(_i: &str, _r: &str) -> Result<bool, PatternSyntaxException> { Ok(false) }
pub fn string_split(i: &str, _r: &str, _l: i32) -> Result<Vec<String>, PatternSyntaxException> {
    Ok(vec![i.to_string()])
}
pub fn string_replace_all(i: &str, _r: &str, _p: &str) -> Result<String, JavaRegexError> {
    Ok(i.to_string())
}
pub fn string_replace_first(i: &str, _r: &str, _p: &str) -> Result<String, JavaRegexError> {
    Ok(i.to_string())
}
pub fn quote(s: &str) -> String { s.to_string() }
pub fn quote_replacement(s: &str) -> String { s.to_string() }
