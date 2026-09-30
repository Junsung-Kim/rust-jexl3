// port of: org.apache.commons.jexl3.parser.ParseException (javacc-generated)

/// The javacc ParseException. The parser only reads `after`/`line`/`column` off it, and
/// Parser.parse recomputes the error token itself, so the generated no-arg form is enough.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ParseException {
    after: String,
    line: i32,
    column: i32,
}

impl ParseException {
    // port of: ParseException()
    pub fn new() -> Self {
        ParseException { after: String::new(), line: -1, column: -1 }
    }
    pub fn get_line(&self) -> i32 {
        self.line
    }
    pub fn get_column(&self) -> i32 {
        self.column
    }
    pub fn get_after(&self) -> &str {
        &self.after
    }
}
