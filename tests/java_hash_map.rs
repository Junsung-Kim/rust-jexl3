// Replays JVM-generated HashMap/HashSet/LinkedHashMap/LinkedHashSet sequences
// (tools/javagen/HashMapGen.java) and compares iteration order, table capacity and hashCode.
use rust_jexl3::java::hash_map::{string_hash_code, JHashMap, JHashSet, JavaHash};
use std::cmp::Ordering;

#[derive(Clone, Debug)]
enum Key {
    Null,
    Int(i32),
    Long(i64),
    Str(Vec<u16>),
    Double(f64),
}

fn double_to_long_bits(d: f64) -> i64 {
    if d.is_nan() {
        0x7ff8_0000_0000_0000
    } else {
        d.to_bits() as i64
    }
}

impl JavaHash for Key {
    fn java_hash_code(&self) -> i32 {
        match self {
            Key::Null => 0,
            Key::Int(i) => *i,
            Key::Long(v) => (v ^ ((*v as u64) >> 32) as i64) as i32,
            Key::Str(s) => string_hash_code(s),
            Key::Double(d) => {
                let b = double_to_long_bits(*d);
                (b ^ ((b as u64) >> 32) as i64) as i32
            }
        }
    }
    fn java_equals(&self, other: &Self) -> bool {
        match (self, other) {
            (Key::Null, Key::Null) => true,
            (Key::Int(a), Key::Int(b)) => a == b,
            (Key::Long(a), Key::Long(b)) => a == b,
            (Key::Str(a), Key::Str(b)) => a == b,
            (Key::Double(a), Key::Double(b)) => double_to_long_bits(*a) == double_to_long_bits(*b),
            _ => false,
        }
    }
    fn java_compare(&self, other: &Self) -> Option<Ordering> {
        match (self, other) {
            (Key::Int(a), Key::Int(b)) => Some(a.cmp(b)),
            (Key::Long(a), Key::Long(b)) => Some(a.cmp(b)),
            (Key::Str(a), Key::Str(b)) => Some(a.cmp(b)),
            // Double.compare
            (Key::Double(a), Key::Double(b)) => Some(if a < b {
                Ordering::Less
            } else if a > b {
                Ordering::Greater
            } else {
                double_to_long_bits(*a).cmp(&double_to_long_bits(*b))
            }),
            _ => None,
        }
    }
    fn java_class_name(&self) -> &str {
        match self {
            Key::Null => "",
            Key::Int(_) => "java.lang.Integer",
            Key::Long(_) => "java.lang.Long",
            Key::Str(_) => "java.lang.String",
            Key::Double(_) => "java.lang.Double",
        }
    }
    fn java_is_null(&self) -> bool {
        matches!(self, Key::Null)
    }
}

fn parse_key(tok: &str) -> Key {
    let (tag, rest) = tok.split_at(1);
    match tag {
        "N" => Key::Null,
        "I" => Key::Int(rest.parse().unwrap()),
        "L" => Key::Long(rest.parse().unwrap()),
        "D" => Key::Double(f64::from_bits(u64::from_str_radix(rest, 16).unwrap())),
        "S" => Key::Str(
            (0..rest.len() / 4)
                .map(|i| u16::from_str_radix(&rest[i * 4..i * 4 + 4], 16).unwrap())
                .collect(),
        ),
        _ => panic!("bad key {tok}"),
    }
}

enum Coll {
    Map(JHashMap<Key, i32>),
    Set(JHashSet<Key>),
}

struct Case {
    mode: String,
    ctor: String,
    pool: Vec<Key>,
    ops: Vec<String>,
    snaps: Vec<(usize, i32, Vec<usize>)>,
}

fn parse(text: &str) -> Vec<Case> {
    let mut cases: Vec<Case> = Vec::new();
    for line in text.lines() {
        let mut it = line.split(' ');
        match it.next() {
            Some("C") => cases.push(Case {
                mode: it.next().unwrap().to_string(),
                ctor: it.next().unwrap().to_string(),
                pool: Vec::new(),
                ops: Vec::new(),
                snaps: Vec::new(),
            }),
            Some("K") => cases.last_mut().unwrap().pool = it.map(parse_key).collect(),
            Some("O") => cases.last_mut().unwrap().ops = it.map(str::to_string).collect(),
            Some("E") => {
                let cap = it.next().unwrap().parse().unwrap();
                let hash = it.next().unwrap().parse().unwrap();
                let order = it.map(|t| t.parse().unwrap()).collect();
                cases.last_mut().unwrap().snaps.push((cap, hash, order));
            }
            _ => {}
        }
    }
    cases
}

fn pool_index(pool: &[Key], k: &Key) -> usize {
    pool.iter().position(|p| p.java_equals(k)).expect("key not in pool")
}

/// Replays one case; Err describes the first mismatch.
fn replay(case: &Case) -> Result<(), String> {
    let cap: Option<usize> = case.ctor.strip_prefix('N').map(|n| n.parse().unwrap());
    let mut c = match (case.mode.as_str(), cap) {
        ("M", None) => Coll::Map(JHashMap::new()),
        ("M", Some(n)) => Coll::Map(JHashMap::with_capacity(n)),
        ("LM", None) => Coll::Map(JHashMap::new_linked()),
        ("LM", Some(n)) => Coll::Map(JHashMap::linked_with_capacity(n)),
        ("S", None) => Coll::Set(JHashSet::new()),
        ("S", Some(n)) => Coll::Set(JHashSet::with_capacity(n)),
        ("LS", None) => Coll::Set(JHashSet::new_linked()),
        ("LS", Some(n)) => Coll::Set(JHashSet::linked_with_capacity(n)),
        m => panic!("bad mode {m:?}"),
    };
    let linked = case.mode.starts_with('L');
    // reference model for values (order is what the JVM fixture checks)
    let mut model: Vec<Option<i32>> = vec![None; case.pool.len()];
    let mut snaps = case.snaps.iter();
    for (pos, op) in case.ops.iter().enumerate() {
        let value = pos as i32 + 1;
        let (tag, arg) = op.split_at(1);
        let idx = || arg.parse::<usize>().unwrap();
        match (tag, &mut c) {
            ("p", Coll::Map(m)) => {
                let old = m.put(case.pool[idx()].clone(), value);
                if old != model[idx()] {
                    return Err(format!("op {pos} put returned {old:?}, expected {:?}", model[idx()]));
                }
                model[idx()] = Some(value);
            }
            ("p", Coll::Set(s)) => {
                if s.add(case.pool[idx()].clone()) != model[idx()].is_none() {
                    return Err(format!("op {pos} add result"));
                }
                model[idx()] = Some(0);
            }
            ("a", Coll::Map(m)) => {
                let old = m.put_if_absent(case.pool[idx()].clone(), value);
                if old != model[idx()] {
                    return Err(format!("op {pos} putIfAbsent returned {old:?}"));
                }
                model[idx()].get_or_insert(value);
            }
            ("r", Coll::Map(m)) => {
                let old = m.remove(&case.pool[idx()]);
                if old != model[idx()] {
                    return Err(format!("op {pos} remove returned {old:?}"));
                }
                model[idx()] = None;
            }
            ("r", Coll::Set(s)) => {
                if s.remove(&case.pool[idx()]) != model[idx()].is_some() {
                    return Err(format!("op {pos} remove result"));
                }
                model[idx()] = None;
            }
            ("c", Coll::Map(m)) => {
                m.clear();
                model.iter_mut().for_each(|v| *v = None);
            }
            ("c", Coll::Set(s)) => {
                s.clear();
                model.iter_mut().for_each(|v| *v = None);
            }
            ("x", _) => {
                let (md, r) = arg.split_once('_').unwrap();
                let (md, r): (usize, usize) = (md.parse().unwrap(), r.parse().unwrap());
                let mut p = 0;
                let mut keep = |k: &Key, model: &mut Vec<Option<i32>>| {
                    let drop = p % md == r;
                    p += 1;
                    if drop {
                        model[pool_index(&case.pool, k)] = None;
                    }
                    !drop
                };
                match &mut c {
                    Coll::Map(m) => m.retain(|k, _| keep(k, &mut model)),
                    Coll::Set(s) => s.retain(|k| keep(k, &mut model)),
                }
            }
            ("y", Coll::Map(m)) => {
                *m = if linked { JHashMap::linked_copy_of(m) } else { JHashMap::copy_of(m) }
            }
            ("y", Coll::Set(s)) => {
                *s = if linked { JHashSet::linked_copy_of(s) } else { JHashSet::copy_of(s) }
            }
            ("=", _) => {
                let (cap, hash, order) = snaps.next().ok_or("missing snapshot")?;
                let (got_cap, got_hash, got_order, len): (usize, i32, Vec<usize>, usize) = match &c {
                    Coll::Map(m) => (
                        m.capacity(),
                        m.hash_code(|v| *v),
                        m.keys().map(|k| pool_index(&case.pool, k)).collect(),
                        m.len(),
                    ),
                    Coll::Set(s) => (
                        s.capacity(),
                        s.hash_code(),
                        s.iter().map(|k| pool_index(&case.pool, k)).collect(),
                        s.len(),
                    ),
                };
                if got_order != *order || len != order.len() {
                    return Err(format!("op {pos} order\n  java {order:?}\n  rust {got_order:?}"));
                }
                if got_cap != *cap {
                    return Err(format!("op {pos} capacity java {cap} rust {got_cap}"));
                }
                if got_hash != *hash {
                    return Err(format!("op {pos} hashCode java {hash} rust {got_hash}"));
                }
                for (i, k) in case.pool.iter().enumerate() {
                    let got = match &c {
                        Coll::Map(m) => m.get(k).copied(),
                        Coll::Set(s) => s.contains(k).then_some(0),
                    };
                    let want = model[i].map(|v| if matches!(c, Coll::Set(_)) { 0 } else { v });
                    if got != want {
                        return Err(format!("op {pos} get(pool[{i}]) = {got:?}, expected {want:?}"));
                    }
                }
            }
            (t, _) => panic!("bad op {t}"),
        }
    }
    Ok(())
}

fn run_fixture(name: &str) {
    let path = format!("{}/tests/data/java_hash_map/{name}", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&path).unwrap();
    let cases = parse(&text);
    assert!(!cases.is_empty());
    let mut failures = Vec::new();
    for (i, case) in cases.iter().enumerate() {
        if let Err(e) = replay(case) {
            failures.push(format!("case {i} ({} {}): {e}", case.mode, case.ctor));
        }
    }
    let shown: Vec<_> = failures.iter().take(5).collect();
    assert!(failures.is_empty(), "{}/{} cases failed; first:\n{shown:#?}", failures.len(), cases.len());
}

#[test]
fn jvm_small_sequences() {
    run_fixture("small.txt");
}

#[test]
fn jvm_tree_bin_sequences() {
    run_fixture("tree.txt");
}

#[test]
fn string_hash_code_matches_java() {
    let s = |x: &str| string_hash_code(&x.encode_utf16().collect::<Vec<_>>());
    assert_eq!(s(""), 0);
    assert_eq!(s("a"), 97);
    assert_eq!(s("Aa"), s("BB"));
    assert_eq!(s("hello"), 99162322);
    assert_eq!(s("한글"), 54620 * 31 + 44544);
    assert_eq!(s("😀"), 0xD83D * 31 + 0xDE00);
    assert_eq!(s("polygenelubricants"), i32::MIN); // well-known Java hash of MIN_VALUE
}

/// Keys with equal hashes that are neither mutually Comparable nor distinguishable by class name:
/// Java's `tieBreakOrder` falls back to `System.identityHashCode`, which is nondeterministic, so
/// the JVM's order inside such a treeified bin cannot be reproduced. We pin our deterministic
/// choice (identity(a) <= identity(b) taken as true: the new key goes left) so a change is noticed.
/// That choice equals a JVM whose identity hashes are all equal: `java -XX:+UnlockExperimentalVMOptions
/// -XX:hashCode=2` (constant identity hash) prints this exact order for the same 12 puts into
/// `new HashMap<>(64)`; a default JVM printed `[1, 0, 2, 3, 4, 5, 9, 6, 11, 7, 10, 8]` (JDK 25).
#[test]
fn ceiling_identity_tie_break_is_pinned() {
    #[derive(Clone, Debug, PartialEq)]
    struct Opaque(u32);
    impl JavaHash for Opaque {
        fn java_hash_code(&self) -> i32 {
            0
        }
        fn java_equals(&self, o: &Self) -> bool {
            self.0 == o.0
        }
    }
    let mut m = JHashMap::with_capacity(64);
    for i in 0..12 {
        m.put(Opaque(i), i);
    }
    assert_eq!(m.capacity(), 64);
    let order: Vec<u32> = m.keys().map(|k| k.0).collect();
    assert_eq!(order, PINNED_OPAQUE_ORDER);
    for i in 0..12 {
        assert_eq!(m.get(&Opaque(i)), Some(&i));
    }
}
const PINNED_OPAQUE_ORDER: [u32; 12] = [3, 0, 1, 2, 4, 5, 6, 7, 8, 9, 10, 11];

#[test]
fn map_is_send_sync_and_clone() {
    fn assert_send_sync<T: Send + Sync + Clone>() {}
    assert_send_sync::<JHashMap<Key, i32>>();
    assert_send_sync::<JHashSet<Key>>();
}

/// Key equal by `id` only, so we can see which key object a map keeps.
#[derive(Clone, Debug)]
struct Tagged(i32, &'static str);
impl JavaHash for Tagged {
    fn java_hash_code(&self) -> i32 {
        self.0
    }
    fn java_equals(&self, o: &Self) -> bool {
        self.0 == o.0
    }
}

#[test]
fn put_keeps_original_key_and_api_surface() {
    let mut m: JHashMap<Tagged, i32> = JHashMap::default();
    assert!(m.is_empty() && m.capacity() == 0 && m.get(&Tagged(1, "")).is_none());
    assert_eq!(m.put(Tagged(1, "first"), 10), None);
    assert_eq!(m.put(Tagged(1, "second"), 11), Some(10));
    assert_eq!(m.keys().next().unwrap().1, "first"); // Java keeps the original key
    assert_eq!(m.put_if_absent(Tagged(1, "third"), 12), Some(11));
    *m.get_mut(&Tagged(1, "")).unwrap() += 100;
    assert!(m.get_mut(&Tagged(2, "")).is_none());
    assert_eq!(m.values().copied().collect::<Vec<_>>(), [111]);
    assert!(m.contains_value(&111, |a, b| a == b) && !m.contains_value(&11, |a, b| a == b));
    assert!(m.contains_key(&Tagged(1, "")) && !m.is_empty() && m.len() == 1);
    assert_eq!(m.iter().map(|(k, v)| (k.0, *v)).collect::<Vec<_>>(), [(1, 111)]);
    assert_eq!(m.remove(&Tagged(2, "")), None);

    let mut s: JHashSet<Tagged> = JHashSet::default();
    assert!(s.is_empty());
    assert!(s.add(Tagged(3, "")) && !s.add(Tagged(3, "")) && !s.is_empty());
    // new HashSet<>(Collection) sizing, JDK 19+: newHashMap(max(n, 12)) -> 16 for n <= 12
    for (n, cap) in [(0, 16), (12, 16), (13, 32), (24, 32), (25, 64)] {
        let mut s = JHashSet::from_collection(n);
        let mut l = JHashSet::linked_from_collection(n);
        s.add(Tagged(0, ""));
        l.add(Tagged(0, ""));
        assert_eq!((s.capacity(), l.capacity()), (cap, cap), "n = {n}");
    }
}
