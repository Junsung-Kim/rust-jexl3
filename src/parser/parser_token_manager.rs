// port of: org.apache.commons.jexl3.parser.ParserTokenManager (javacc-generated)
// The DFA/NFA methods and tables are mechanically translated in parser_token_manager_gen.rs;
// this file holds the hand-ported driver (getNextToken, jjFillToken, lexical actions, helpers).
use super::parser_token_manager_gen::{JJNEW_LEX_STATE, JJNEXT_STATES, JJSTR_LITERAL_IMAGES, JJTO_TOKEN};
use super::simple_char_stream::SimpleCharStream;
use super::string_parser;
use super::token::Token;
use super::token_mgr_exception::{TokenMgrException, INVALID_LEXICAL_STATE, LEXICAL_ERROR};

/// Lexical states (ParserConstants)
pub const DEFAULT: i32 = 0;
pub const DOT_ID: i32 = 1;
pub const REGISTERS: i32 = 2;

pub struct ParserTokenManager {
    pub(crate) input_stream: SimpleCharStream,
    pub(crate) cur_char: i32,
    pub(crate) cur_lex_state: i32,
    pub default_lex_state: i32,
    pub(crate) jjnew_state_cnt: i32,
    pub(crate) jjround: i32,
    pub(crate) jjmatched_pos: i32,
    pub(crate) jjmatched_kind: i32,
    pub(crate) jjrounds: [i32; 74],
    pub(crate) jjstate_set: [i32; 148],
    /// A stack of 1 for keeping state to deal with doted identifiers
    dot_lex_state: i32,
}

impl ParserTokenManager {
    // port of: ParserTokenManager(SimpleCharStream)
    pub fn new(stream: SimpleCharStream) -> Self {
        let mut tm = ParserTokenManager {
            input_stream: stream,
            cur_char: 0,
            cur_lex_state: 0,
            default_lex_state: 0,
            jjnew_state_cnt: 0,
            jjround: 0,
            jjmatched_pos: 0,
            jjmatched_kind: 0,
            jjrounds: [0; 74],
            jjstate_set: [0; 148],
            dot_lex_state: DEFAULT,
        };
        tm.re_init_rounds();
        tm
    }

    // port of: ParserTokenManager.ReInit(SimpleCharStream)
    pub fn re_init(&mut self, stream: SimpleCharStream) {
        self.jjmatched_pos = 0;
        self.jjnew_state_cnt = 0;
        self.cur_lex_state = self.default_lex_state;
        self.input_stream = stream;
        self.re_init_rounds();
    }

    // port of: ParserTokenManager.pushDot
    pub fn push_dot(&mut self) {
        self.dot_lex_state = self.cur_lex_state;
        self.cur_lex_state = DOT_ID;
    }

    // port of: ParserTokenManager.popDot
    pub fn pop_dot(&mut self) {
        if self.cur_lex_state == DOT_ID {
            self.cur_lex_state = self.dot_lex_state;
            self.dot_lex_state = self.default_lex_state;
        }
    }

    // port of: ParserTokenManager.SwitchTo
    pub fn switch_to(&mut self, lex_state: i32) -> Result<(), TokenMgrException> {
        if !(0..3).contains(&lex_state) {
            return Err(TokenMgrException::with_message(
                &format!("Error: Ignoring invalid lexical state : {}. State unchanged.", lex_state),
                INVALID_LEXICAL_STATE,
            ));
        }
        self.cur_lex_state = lex_state;
        Ok(())
    }

    // port of: ParserTokenManager.ReInitRounds
    pub(crate) fn re_init_rounds(&mut self) {
        self.jjround = 0x80000001u32 as i32;
        for r in self.jjrounds.iter_mut() {
            *r = 0x80000000u32 as i32;
        }
    }

    // port of: ParserTokenManager.jjCheckNAdd
    pub(crate) fn jj_check_n_add(&mut self, state: i32) {
        if self.jjrounds[state as usize] != self.jjround {
            self.jjstate_set[self.jjnew_state_cnt as usize] = state;
            self.jjnew_state_cnt += 1;
            self.jjrounds[state as usize] = self.jjround;
        }
    }

    // port of: ParserTokenManager.jjAddStates
    pub(crate) fn jj_add_states(&mut self, mut start: i32, end: i32) {
        loop {
            self.jjstate_set[self.jjnew_state_cnt as usize] = JJNEXT_STATES[start as usize];
            self.jjnew_state_cnt += 1;
            let s = start;
            start += 1;
            if s == end {
                break;
            }
        }
    }

    // port of: ParserTokenManager.jjCheckNAddTwoStates
    pub(crate) fn jj_check_n_add_two_states(&mut self, state1: i32, state2: i32) {
        self.jj_check_n_add(state1);
        self.jj_check_n_add(state2);
    }

    // port of: ParserTokenManager.jjCheckNAddStates
    pub(crate) fn jj_check_n_add_states(&mut self, mut start: i32, end: i32) {
        loop {
            self.jj_check_n_add(JJNEXT_STATES[start as usize]);
            let s = start;
            start += 1;
            if s == end {
                break;
            }
        }
    }

    // port of: ParserTokenManager.jjFillToken
    fn jj_fill_token(&mut self) -> Token {
        let im = JJSTR_LITERAL_IMAGES[self.jjmatched_kind as usize];
        let image = match im {
            Some(s) => s.encode_utf16().collect(),
            None => self.input_stream.get_image(),
        };
        Token {
            kind: self.jjmatched_kind,
            begin_line: self.input_stream.get_begin_line(),
            begin_column: self.input_stream.get_begin_column(),
            end_line: self.input_stream.get_end_line(),
            end_column: self.input_stream.get_end_column(),
            image,
            next: None,
        }
    }

    // port of: ParserTokenManager.TokenLexicalActions
    fn token_lexical_actions(&mut self, matched_token: &mut Token) {
        match self.jjmatched_kind {
            9..=22 | 24..=26 | 43 | 45 | 47 | 49 | 51 | 53 | 55 | 57 | 77 | 79 | 81 | 89 | 104 | 105 | 106 => self.pop_dot(),
            36 | 37 => self.push_dot(),
            90 => matched_token.image = string_parser::unescape_identifier(&matched_token.image),
            _ => {}
        }
    }

    // port of: ParserTokenManager.getNextToken
    pub fn get_next_token(&mut self) -> Result<Token, TokenMgrException> {
        let mut cur_pos: i32 = 0;
        'eof_loop: loop {
            match self.input_stream.begin_token() {
                Ok(c) => self.cur_char = c,
                Err(_) => {
                    self.jjmatched_kind = 0;
                    self.jjmatched_pos = -1;
                    return Ok(self.jj_fill_token());
                }
            }
            // all three lexical states skip the same white space: ' ', '\t', '\n', '\f', '\r'
            self.input_stream.backup(0);
            while self.cur_char <= 32 && (0x100003600i64 & (1i64 << self.cur_char)) != 0 {
                match self.input_stream.begin_token() {
                    Ok(c) => self.cur_char = c,
                    Err(_) => continue 'eof_loop,
                }
            }
            self.jjmatched_kind = 0x7fffffff;
            self.jjmatched_pos = 0;
            match self.cur_lex_state {
                0 => cur_pos = self.jj_move_string_literal_dfa0_0(),
                1 => cur_pos = self.jj_move_string_literal_dfa0_1(),
                2 => cur_pos = self.jj_move_string_literal_dfa0_2(),
                _ => {}
            }
            if self.jjmatched_kind != 0x7fffffff {
                if self.jjmatched_pos + 1 < cur_pos {
                    self.input_stream.backup(cur_pos - self.jjmatched_pos - 1);
                }
                let kind = self.jjmatched_kind;
                if (JJTO_TOKEN[(kind >> 6) as usize] & (1i64 << (kind & 0o77))) != 0 {
                    let mut matched_token = self.jj_fill_token();
                    self.token_lexical_actions(&mut matched_token);
                    if JJNEW_LEX_STATE[kind as usize] != -1 {
                        self.cur_lex_state = JJNEW_LEX_STATE[kind as usize];
                    }
                    return Ok(matched_token);
                } else {
                    if JJNEW_LEX_STATE[kind as usize] != -1 {
                        self.cur_lex_state = JJNEW_LEX_STATE[kind as usize];
                    }
                    continue 'eof_loop;
                }
            }
            let mut error_line = self.input_stream.get_end_line();
            let mut error_column = self.input_stream.get_end_column();
            let mut error_after = Vec::new();
            let mut eof_seen = false;
            match self.input_stream.read_char() {
                Ok(_) => self.input_stream.backup(1),
                Err(_) => {
                    eof_seen = true;
                    error_after = if cur_pos <= 1 { Vec::new() } else { self.input_stream.get_image() };
                    if self.cur_char == '\n' as i32 || self.cur_char == '\r' as i32 {
                        error_line += 1;
                        error_column = 0;
                    } else {
                        error_column += 1;
                    }
                }
            }
            if !eof_seen {
                self.input_stream.backup(1);
                error_after = if cur_pos <= 1 { Vec::new() } else { self.input_stream.get_image() };
            }
            return Err(TokenMgrException::new(
                eof_seen,
                self.cur_lex_state,
                error_line,
                error_column,
                error_after,
                self.cur_char,
                LEXICAL_ERROR,
            ));
        }
    }
}
