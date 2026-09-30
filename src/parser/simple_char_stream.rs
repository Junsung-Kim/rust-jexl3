// port of: org.apache.commons.jexl3.parser.SimpleCharStream (javacc-generated, with AbstractCharStream
// and StringProvider). The Java class reads through a ring buffer; this port keeps the whole source in
// memory, which is observably identical: line/column of each char are computed once, in reading order,
// exactly as AbstractCharStream.internalUpdateLineColumn does (tab size 1).

/// Java's IOException("PGCC end of stream"): the only failure a string-backed stream can raise.
#[derive(Debug, Clone, Copy)]
pub struct EndOfStream;

pub struct SimpleCharStream {
    buffer: Vec<u16>,
    line: Vec<i32>,
    column: Vec<i32>,
    /// index of the current char (-1 before the first read)
    bufpos: isize,
    /// number of chars read so far (line/column known for [0, max_read))
    max_read: usize,
    token_begin: isize,
    line_no: i32,
    column_no: i32,
    prev_char_is_cr: bool,
    prev_char_is_lf: bool,
}

impl SimpleCharStream {
    // port of: SimpleCharStream(Provider, 1, 1)
    pub fn new(src: &[u16]) -> Self {
        SimpleCharStream {
            buffer: src.to_vec(),
            line: vec![0; src.len()],
            column: vec![0; src.len()],
            bufpos: -1,
            max_read: 0,
            token_begin: 0,
            line_no: 1,
            column_no: 0,
            prev_char_is_cr: false,
            prev_char_is_lf: false,
        }
    }

    // port of: AbstractCharStream.internalUpdateLineColumn (tab size 1)
    fn update_line_column(&mut self, c: u16) {
        self.column_no += 1;
        if self.prev_char_is_lf {
            self.prev_char_is_lf = false;
            self.column_no = 1;
            self.line_no += 1;
        } else if self.prev_char_is_cr {
            self.prev_char_is_cr = false;
            if c == b'\n' as u16 {
                self.prev_char_is_lf = true;
            } else {
                self.column_no = 1;
                self.line_no += 1;
            }
        }
        match c {
            0x0d => self.prev_char_is_cr = true,
            0x0a => self.prev_char_is_lf = true,
            // tab: column_no-- ; column_no += tabSize - (column_no % tabSize) with tabSize 1 is a no-op
            _ => {}
        }
        let p = self.bufpos as usize;
        self.line[p] = self.line_no;
        self.column[p] = self.column_no;
    }

    // port of: AbstractCharStream.readChar
    pub fn read_char(&mut self) -> Result<i32, EndOfStream> {
        let next = (self.bufpos + 1) as usize;
        if next >= self.buffer.len() {
            // fillBuff: --bufpos; backup(0); if (tokenBegin == -1) tokenBegin = bufpos; throw
            if self.token_begin == -1 {
                self.token_begin = self.bufpos;
            }
            return Err(EndOfStream);
        }
        self.bufpos += 1;
        let c = self.buffer[next];
        if next >= self.max_read {
            self.max_read = next + 1;
            self.update_line_column(c);
        }
        Ok(c as i32)
    }

    // port of: AbstractCharStream.beginToken
    pub fn begin_token(&mut self) -> Result<i32, EndOfStream> {
        self.token_begin = -1;
        let c = self.read_char()?;
        self.token_begin = self.bufpos;
        Ok(c)
    }

    fn at(v: &[i32], p: isize) -> i32 {
        if p < 0 {
            0
        } else {
            v.get(p as usize).copied().unwrap_or(0)
        }
    }

    pub fn get_begin_column(&self) -> i32 {
        Self::at(&self.column, self.token_begin)
    }
    pub fn get_begin_line(&self) -> i32 {
        Self::at(&self.line, self.token_begin)
    }
    pub fn get_end_column(&self) -> i32 {
        Self::at(&self.column, self.bufpos)
    }
    pub fn get_end_line(&self) -> i32 {
        Self::at(&self.line, self.bufpos)
    }

    // port of: AbstractCharStream.backup
    pub fn backup(&mut self, amount: i32) {
        self.bufpos -= amount as isize;
    }

    // port of: AbstractCharStream.getImage
    pub fn get_image(&self) -> Vec<u16> {
        if self.token_begin < 0 || self.bufpos < self.token_begin {
            return Vec::new();
        }
        self.buffer[self.token_begin as usize..=self.bufpos as usize].to_vec()
    }
}
