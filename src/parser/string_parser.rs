// port of: org.apache.commons.jexl3.parser.StringParser
// Strings are UTF-16 code-unit slices, like java.lang.String.

const FIRST_ASCII: u16 = 32;
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
    let _ = UCHAR_LEN;
    strb
}

// port of: StringParser.unescapeIdentifier(String): drops every backslash
pub fn unescape_identifier(s: &[u16]) -> Vec<u16> {
    s.iter().copied().filter(|&c| c != b'\\' as u16).collect()
}
