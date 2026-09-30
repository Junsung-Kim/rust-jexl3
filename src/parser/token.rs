// port of: org.apache.commons.jexl3.parser.Token (javacc-generated)
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Token {
    /// the token kind (ParserConstants)
    pub kind: i32,
    pub begin_line: i32,
    pub begin_column: i32,
    pub end_line: i32,
    pub end_column: i32,
    /// the token image, in UTF-16 code units like java.lang.String
    pub image: Vec<u16>,
    /// index of the next token in the parser's token arena (Token.next)
    pub next: Option<usize>,
}

impl Token {
    // port of: Token.newToken(int)
    pub fn new_token(kind: i32) -> Token {
        Token { kind, ..Token::default() }
    }

    pub fn image_string(&self) -> String {
        String::from_utf16_lossy(&self.image)
    }
}
