// port of: org.apache.commons.jexl3.parser.StringParser
// Strings are UTF-16 code-unit slices, like java.lang.String.

const FIRST_ASCII: u16 = 32;
const SHIFT: i32 = 12;
const BASE10: u16 = 10;
const LAST_ASCII: u16 = 127;
const UCHAR_LEN: usize = 4;

// port of: StringParser.escapeString(String, char)
pub fn escape_string(s: &[u16], delim: u16) -> Vec<u16> {
    let mut strb: Vec<u16> = Vec::with_capacity(s.len() + 2);
    strb.push(delim);
    for &c in s {
        match c {
            0 => continue,
            0x08 => strb.extend("\\b".encode_utf16()),
            0x09 => strb.extend("\\t".encode_utf16()),
            0x0a => strb.extend("\\n".encode_utf16()),
            0x0c => strb.extend("\\f".encode_utf16()),
            0x0d => strb.extend("\\r".encode_utf16()),
            0x22 => strb.extend("\\\"".encode_utf16()),
            0x27 => strb.extend("\\'".encode_utf16()),
            0x5c => strb.extend("\\\\".encode_utf16()),
            _ => {
                if (FIRST_ASCII..=LAST_ASCII).contains(&c) {
                    strb.push(c);
                } else {
                    // convert to \u + 4 hex digits (Integer.toHexString, left-padded)
                    strb.extend(format!("\\u{:04x}", c).encode_utf16());
                }
            }
        }
    }
    strb.push(delim);
    strb
}

// port of: StringParser.unescapeIdentifier(String): drops every backslash
pub fn unescape_identifier(s: &[u16]) -> Vec<u16> {
    s.iter().copied().filter(|&c| c != b'\\' as u16).collect()
}

// port of: StringParser.buildString(CharSequence, boolean)
pub fn build_string(s: &[u16], eatsep: bool) -> Vec<u16> {
    build_string_esc(s, eatsep, true)
}

// port of: StringParser.buildTemplate
pub fn build_template(s: &[u16], eatsep: bool) -> Vec<u16> {
    build_string_esc(s, eatsep, false)
}

// port of: StringParser.buildString(CharSequence, boolean, boolean)
fn build_string_esc(s: &[u16], eatsep: bool, esc: bool) -> Vec<u16> {
    let mut strb = Vec::with_capacity(s.len());
    let sep = if eatsep { s.first().copied().unwrap_or(0) } else { 0 };
    let end = s.len() - if eatsep { 1 } else { 0 };
    let begin = if eatsep { 1 } else { 0 };
    read(&mut strb, s, begin, end, sep, esc);
    strb
}

// port of: StringParser.buildRegex
pub fn build_regex(s: &[u16]) -> Vec<u16> {
    build_string(&s[1..], true)
}

// port of: StringParser.readString
pub fn read_string(strb: &mut Vec<u16>, s: &[u16], index: usize, sep: u16) -> usize {
    read(strb, s, index, s.len(), sep, true)
}

// port of: StringParser.read
fn read(strb: &mut Vec<u16>, s: &[u16], begin: usize, end: usize, sep: u16, esc: bool) -> usize {
    let mut escape = false;
    let mut index = begin;
    while index < end {
        let c = s[index];
        if escape {
            if c == b'u' as u16 && (index + UCHAR_LEN) < end && read_unicode_char(strb, s, index + 1) > 0 {
                index += UCHAR_LEN;
            } else {
                // if not an escape delimiter, keep the escaping
                let not_separator = if sep == 0 { c != b'\'' as u16 && c != b'"' as u16 } else { c != sep };
                if not_separator && c != b'\\' as u16 {
                    if !esc {
                        strb.push(b'\\' as u16);
                        strb.push(c);
                    } else {
                        match c {
                            0x62 => strb.push(0x08), // b
                            0x74 => strb.push(0x09), // t
                            0x6e => strb.push(0x0a), // n
                            0x66 => strb.push(0x0c), // f
                            0x72 => strb.push(0x0d), // r
                            _ => {
                                strb.push(b'\\' as u16);
                                strb.push(c);
                            }
                        }
                    }
                } else {
                    strb.push(c);
                }
            }
            escape = false;
            index += 1;
            continue;
        }
        if c == b'\\' as u16 {
            escape = true;
            index += 1;
            continue;
        }
        strb.push(c);
        if c == sep {
            break;
        }
        index += 1;
    }
    index
}

// port of: StringParser.readUnicodeChar (note: accepts 'a'-'h' / 'A'-'H' like the Java code)
fn read_unicode_char(strb: &mut Vec<u16>, s: &[u16], begin: usize) -> usize {
    let mut xc: u16 = 0;
    let mut bits = SHIFT;
    for offset in 0..UCHAR_LEN {
        let c = s[begin + offset];
        let value: u16 = if (b'0' as u16..=b'9' as u16).contains(&c) {
            c - b'0' as u16
        } else if (b'a' as u16..=b'h' as u16).contains(&c) {
            c - b'a' as u16 + BASE10
        } else if (b'A' as u16..=b'H' as u16).contains(&c) {
            c - b'A' as u16 + BASE10
        } else {
            return 0;
        };
        // char |= int << bits : the int is truncated to 16 bits
        xc |= ((value as i32) << bits) as u16;
        bits -= UCHAR_LEN as i32;
    }
    strb.push(xc);
    UCHAR_LEN
}

// port of: StringParser.escapeIdentifier
pub fn escape_identifier(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, ' ' | '\'' | '"' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}
