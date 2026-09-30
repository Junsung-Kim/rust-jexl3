// Minimal JSON reader/writer shared by the tests and the harness runner (included via #[path]).
// Strings are kept as UTF-16 so lone surrogates written by the Java oracle survive a round trip.
#![allow(dead_code)]

#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Num(String),
    Str(Vec<u16>),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Obj(kv) => kv.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    pub fn str16(&self) -> Option<&[u16]> {
        match self {
            Json::Str(s) => Some(s),
            _ => None,
        }
    }
    pub fn string(&self) -> Option<String> {
        self.str16().map(String::from_utf16_lossy)
    }
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Json::Num(n) => n.parse().ok(),
            _ => None,
        }
    }
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Json::Bool(b) => Some(*b),
            _ => None,
        }
    }
    pub fn arr(&self) -> Option<&[Json]> {
        match self {
            Json::Arr(a) => Some(a),
            _ => None,
        }
    }
    pub fn str(s: &str) -> Json {
        Json::Str(s.encode_utf16().collect())
    }
}

pub fn parse(s: &str) -> Result<Json, String> {
    let b = s.as_bytes();
    let mut i = 0;
    let v = value(b, &mut i)?;
    ws(b, &mut i);
    if i != b.len() {
        return Err(format!("trailing json at {}", i));
    }
    Ok(v)
}

fn ws(b: &[u8], i: &mut usize) {
    while *i < b.len() && matches!(b[*i], b' ' | b'\t' | b'\r' | b'\n') {
        *i += 1;
    }
}

fn value(b: &[u8], i: &mut usize) -> Result<Json, String> {
    ws(b, i);
    match b.get(*i) {
        None => Err("eof".into()),
        Some(b'{') => {
            *i += 1;
            let mut kv = Vec::new();
            ws(b, i);
            if b.get(*i) == Some(&b'}') {
                *i += 1;
                return Ok(Json::Obj(kv));
            }
            loop {
                ws(b, i);
                let k = String::from_utf16_lossy(&string(b, i)?);
                ws(b, i);
                expect(b, i, b':')?;
                let v = value(b, i)?;
                kv.push((k, v));
                ws(b, i);
                if b.get(*i) == Some(&b',') {
                    *i += 1;
                    continue;
                }
                expect(b, i, b'}')?;
                return Ok(Json::Obj(kv));
            }
        }
        Some(b'[') => {
            *i += 1;
            let mut a = Vec::new();
            ws(b, i);
            if b.get(*i) == Some(&b']') {
                *i += 1;
                return Ok(Json::Arr(a));
            }
            loop {
                a.push(value(b, i)?);
                ws(b, i);
                if b.get(*i) == Some(&b',') {
                    *i += 1;
                    continue;
                }
                expect(b, i, b']')?;
                return Ok(Json::Arr(a));
            }
        }
        Some(b'"') => Ok(Json::Str(string(b, i)?)),
        Some(b't') => {
            *i += 4;
            Ok(Json::Bool(true))
        }
        Some(b'f') => {
            *i += 5;
            Ok(Json::Bool(false))
        }
        Some(b'n') => {
            *i += 4;
            Ok(Json::Null)
        }
        Some(_) => {
            let st = *i;
            while *i < b.len() && b"+-0123456789.eE".contains(&b[*i]) {
                *i += 1;
            }
            if st == *i {
                return Err(format!("bad json at {}", st));
            }
            Ok(Json::Num(String::from_utf8_lossy(&b[st..*i]).into_owned()))
        }
    }
}

fn expect(b: &[u8], i: &mut usize, c: u8) -> Result<(), String> {
    if b.get(*i) != Some(&c) {
        return Err(format!("expected {} at {}", c as char, i));
    }
    *i += 1;
    Ok(())
}

fn string(b: &[u8], i: &mut usize) -> Result<Vec<u16>, String> {
    expect(b, i, b'"')?;
    let mut out: Vec<u16> = Vec::new();
    loop {
        let c = *b.get(*i).ok_or("eof in string")?;
        *i += 1;
        match c {
            b'"' => return Ok(out),
            b'\\' => {
                let e = *b.get(*i).ok_or("eof in escape")?;
                *i += 1;
                match e {
                    b'n' => out.push(10),
                    b't' => out.push(9),
                    b'r' => out.push(13),
                    b'b' => out.push(8),
                    b'f' => out.push(12),
                    b'u' => {
                        let h = std::str::from_utf8(&b[*i..*i + 4]).map_err(|e| e.to_string())?;
                        out.push(u16::from_str_radix(h, 16).map_err(|e| e.to_string())?);
                        *i += 4;
                    }
                    other => out.push(other as u16),
                }
            }
            _ => {
                // raw UTF-8: decode one scalar
                let st = *i - 1;
                let len = match c {
                    0x00..=0x7f => 1,
                    0xc0..=0xdf => 2,
                    0xe0..=0xef => 3,
                    _ => 4,
                };
                let s = std::str::from_utf8(&b[st..st + len]).map_err(|e| e.to_string())?;
                out.extend(s.encode_utf16());
                *i = st + len;
            }
        }
    }
}

pub fn quote(out: &mut String, s: &[u16]) {
    out.push('"');
    for &c in s {
        match c {
            0x22 => out.push_str("\\\""),
            0x5c => out.push_str("\\\\"),
            0x20..=0x7e => out.push(c as u8 as char),
            _ => out.push_str(&format!("\\u{:04x}", c)),
        }
    }
    out.push('"');
}

pub fn write(out: &mut String, v: &Json) {
    match v {
        Json::Null => out.push_str("null"),
        Json::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Json::Num(n) => out.push_str(n),
        Json::Str(s) => quote(out, s),
        Json::Arr(a) => {
            out.push('[');
            for (n, e) in a.iter().enumerate() {
                if n > 0 {
                    out.push(',');
                }
                write(out, e);
            }
            out.push(']');
        }
        Json::Obj(kv) => {
            out.push('{');
            for (n, (k, e)) in kv.iter().enumerate() {
                if n > 0 {
                    out.push(',');
                }
                quote(out, &k.encode_utf16().collect::<Vec<_>>());
                out.push(':');
                write(out, e);
            }
            out.push('}');
        }
    }
}

pub fn to_string(v: &Json) -> String {
    let mut s = String::new();
    write(&mut s, v);
    s
}
