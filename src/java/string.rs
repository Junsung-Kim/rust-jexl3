// port of: java.lang.String (UTF-16 value semantics: length, charAt, compareTo, hashCode count code units)
use std::cmp::Ordering;
use std::fmt;
use std::sync::Arc;

/// An immutable Java string: a shared slice of UTF-16 code units (lone surrogates allowed).
#[derive(Clone, PartialEq, Eq, Hash, Default)]
pub struct JString(Arc<[u16]>);

impl JString {
    pub fn new(units: Vec<u16>) -> JString {
        JString(Arc::from(units))
    }
    pub fn from_units(units: &[u16]) -> JString {
        JString(Arc::from(units))
    }
    pub fn empty() -> JString {
        JString(Arc::from(Vec::new()))
    }
    pub fn units(&self) -> &[u16] {
        &self.0
    }
    /// String.length()
    pub fn len(&self) -> usize {
        self.0.len()
    }
    /// String.isEmpty()
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    /// Lossy conversion (lone surrogates become U+FFFD).
    pub fn to_rust(&self) -> String {
        String::from_utf16_lossy(&self.0)
    }
    /// String.charAt (None when out of bounds)
    pub fn char_at(&self, i: usize) -> Option<u16> {
        self.0.get(i).copied()
    }
    /// String.hashCode
    pub fn hash_code(&self) -> i32 {
        let mut h: i32 = 0;
        for &c in self.0.iter() {
            h = h.wrapping_mul(31).wrapping_add(c as i32);
        }
        h
    }
    /// String.compareTo: lexicographic on code units, then length.
    pub fn compare_to(&self, other: &JString) -> i32 {
        let (a, b) = (&self.0, &other.0);
        let n = a.len().min(b.len());
        for k in 0..n {
            if a[k] != b[k] {
                return a[k] as i32 - b[k] as i32;
            }
        }
        a.len() as i32 - b.len() as i32
    }
    pub fn concat(&self, other: &JString) -> JString {
        let mut v = Vec::with_capacity(self.len() + other.len());
        v.extend_from_slice(&self.0);
        v.extend_from_slice(&other.0);
        JString::new(v)
    }
    pub fn starts_with(&self, prefix: &JString) -> bool {
        self.0.starts_with(&prefix.0)
    }
    pub fn ends_with(&self, suffix: &JString) -> bool {
        self.0.ends_with(&suffix.0)
    }
    /// String.indexOf(String, from)
    pub fn index_of(&self, s: &JString, from: i32) -> i32 {
        let from = from.max(0) as usize;
        let (h, n) = (&self.0, &s.0);
        if n.is_empty() {
            return if from <= h.len() { from as i32 } else { h.len() as i32 };
        }
        if n.len() > h.len() {
            return -1;
        }
        let last = h.len() - n.len();
        let mut k = from;
        while k <= last {
            if &h[k..k + n.len()] == &n[..] {
                return k as i32;
            }
            k += 1;
        }
        -1
    }
    /// String.lastIndexOf(String)
    pub fn last_index_of(&self, s: &JString) -> i32 {
        let (h, n) = (&self.0, &s.0);
        if n.len() > h.len() {
            return -1;
        }
        let mut k = h.len() - n.len();
        loop {
            if &h[k..k + n.len()] == &n[..] {
                return k as i32;
            }
            if k == 0 {
                return -1;
            }
            k -= 1;
        }
    }
    pub fn substring(&self, begin: usize, end: usize) -> JString {
        JString::from_units(&self.0[begin..end])
    }
    pub fn eq_str(&self, s: &str) -> bool {
        self.0.iter().copied().eq(s.encode_utf16())
    }
}

/// Builds a Java String from pieces (ASCII literals and UTF-16 fragments).
#[derive(Default)]
pub struct JStringBuilder(Vec<u16>);

impl JStringBuilder {
    pub fn new() -> Self {
        JStringBuilder(Vec::new())
    }
    pub fn str(&mut self, s: &str) -> &mut Self {
        self.0.extend(s.encode_utf16());
        self
    }
    pub fn units(&mut self, u: &[u16]) -> &mut Self {
        self.0.extend_from_slice(u);
        self
    }
    pub fn jstr(&mut self, s: &JString) -> &mut Self {
        self.0.extend_from_slice(s.units());
        self
    }
    pub fn build(&self) -> JString {
        JString::from_units(&self.0)
    }
}

impl From<&str> for JString {
    fn from(s: &str) -> JString {
        JString::new(s.encode_utf16().collect())
    }
}

impl From<String> for JString {
    fn from(s: String) -> JString {
        JString::from(s.as_str())
    }
}

impl From<&String> for JString {
    fn from(s: &String) -> JString {
        JString::from(s.as_str())
    }
}

impl fmt::Display for JString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_rust())
    }
}

impl fmt::Debug for JString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.to_rust())
    }
}

impl PartialOrd for JString {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for JString {
    fn cmp(&self, other: &Self) -> Ordering {
        self.compare_to(other).cmp(&0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_semantics() {
        // values checked against java.lang.String on JDK 25
        let s = JString::from("a😀한");
        assert_eq!(s.len(), 4);
        assert_eq!(s.char_at(1), Some(0xd83d));
        assert_eq!(JString::from("😀").hash_code(), 1772899);
        assert_eq!(JString::from("한글").hash_code(), 1737764);
        assert_eq!(JString::from("abc").compare_to(&JString::from("abd")), -1);
        assert_eq!(JString::from("ab").compare_to(&JString::from("abc")), -1);
        // U+FFFF sorts after a surrogate pair in UTF-16 order (unlike code point order)
        assert!(JString::from("\u{ffff}").compare_to(&JString::from("😀")) > 0);
        assert_eq!(JString::from("abcabc").index_of(&JString::from("c"), 3), 5);
        assert_eq!(JString::from("abcabc").last_index_of(&JString::from("ab")), 3);
        assert_eq!(JString::from("abc").index_of(&JString::from(""), 7), 3);
    }
}
