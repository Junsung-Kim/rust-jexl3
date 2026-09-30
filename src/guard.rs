// Not a port: Java JEXL has nothing like this. A pre-parse check for untrusted source text.
//
// JEXL 3.2.1's parser backtracks exponentially on deeply nested *unterminated* literals -- in the
// jar as in this port, measured: `"8%" + "{" * 13` takes the jar 27 seconds. A caller that parses
// text it does not trust can bound the nesting first, in linear time.

/// The deepest nesting of `(`, `[` and `{` in `source`, not counting brackets inside string
/// literals (`'...'`, `"..."`, `` `...` ``) or comments. An unmatched opening bracket counts: that is
/// the shape that makes the parser slow.
///
/// The count is conservative -- a bracket in a regex literal (`~/[a-z]/`) counts too -- so a limit
/// chosen with it may reject a valid script, never admit a slow one past the limit.
///
/// ```
/// use rust_jexl3::guard::nesting_depth;
/// assert_eq!(nesting_depth("a.b[0] + f(x, {'k': [1, 2]})"), 3);
/// assert_eq!(nesting_depth("'{{{{{{{{'"), 0);
/// // reject before parsing
/// let source = format!("8%{}", "{".repeat(40));
/// assert!(nesting_depth(&source) > 16);
/// ```
pub fn nesting_depth(source: &str) -> usize {
    let mut depth = 0usize;
    let mut deepest = 0usize;
    let mut chars = source.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\'' | '"' | '`' => {
                // a string literal: skip to its closing quote, honouring backslash escapes
                while let Some(d) = chars.next() {
                    if d == '\\' {
                        chars.next();
                    } else if d == c {
                        break;
                    }
                }
            }
            '/' if chars.peek() == Some(&'/') => {
                for d in chars.by_ref() {
                    if d == '\n' {
                        break;
                    }
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                let mut star = false;
                for d in chars.by_ref() {
                    if star && d == '/' {
                        break;
                    }
                    star = d == '*';
                }
            }
            '(' | '[' | '{' => {
                depth += 1;
                deepest = deepest.max(depth);
            }
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    deepest
}

#[cfg(test)]
mod tests {
    use super::nesting_depth;

    #[test]
    fn counts_brackets_outside_literals_and_comments() {
        assert_eq!(nesting_depth(""), 0);
        assert_eq!(nesting_depth("a + b"), 0);
        assert_eq!(nesting_depth("f(x)"), 1);
        assert_eq!(nesting_depth("a.b[0] + f(x, {'k': [1, 2]})"), 3);
        assert_eq!(nesting_depth("\"(((\" + `[[${x}]]` + '{\\'{'"), 0);
        assert_eq!(nesting_depth("// ((((\nf(x) /* [[[[ */"), 1);
        // the shape the parser is slow on: nothing ever closes
        assert_eq!(nesting_depth(&format!("8%{}q8", "{".repeat(13))), 13);
        // closing more than was opened does not go negative
        assert_eq!(nesting_depth(")))(("), 2);
    }
}
