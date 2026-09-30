// port of: java.util.HashMap
// (JDK 25 sources; also java.util.HashSet, java.util.LinkedHashMap with accessOrder=false,
// java.util.LinkedHashSet.)
//
// JEXL scripts observe iteration order (`for (x : map)`, `toString()`, `keySet()`...), so this is a
// line-by-line port of the JDK algorithm, not a lookalike: spread hash, lazy table allocation,
// `tableSizeFor`, load factor 0.75 thresholds, tail insertion, order-preserving resize split, and
// tree bins (treeify/untreeify/split, red-black balancing, `moveRootToFront`), because a treeified
// bin iterates in its `next` order, which the tree operations rearrange.
//
// Nodes live in an arena (`Vec<Node>`) and link by index; a removed node is `swap_remove`d and the
// node moved into its slot is re-pointed (`relink`). Iteration never depends on arena order.
//
// Ceiling: when two keys in one tree bin have equal hashes, are not mutually Comparable
// (`JavaHash::java_compare` is None or Equal) and have the same class name, Java's `tieBreakOrder`
// compares `System.identityHashCode`, which is nondeterministic run to run. We take
// `identity(a) <= identity(b)` as always true (new key goes left). Pinned by
// `tests/java_hash_map.rs::ceiling_identity_tie_break_is_pinned`. Null keys never hit this: their
// identity hash is 0 and HotSpot never hands out 0, so that tie-break is deterministic and ported.
use std::cmp::Ordering;

/// Java `hashCode`/`equals` (and, for tree bins, `compareTo` and `getClass().getName()`) of a key.
pub trait JavaHash {
    /// `Object.hashCode()`; must be 0 for the Java null key.
    fn java_hash_code(&self) -> i32;
    /// `this.equals(other)`; `this` is the probe key, `other` the stored key, as in `HashMap`.
    fn java_equals(&self, other: &Self) -> bool;
    /// Tree bins: `Some(this.compareTo(other))` when `this` is `class C implements Comparable<C>` and
    /// `other` is also a `C` (Java `comparableClassFor` + `compareComparables`), else `None`.
    fn java_compare(&self, _other: &Self) -> Option<Ordering> {
        None
    }
    /// Tree bins: `getClass().getName()`, compared by `tieBreakOrder` when `java_compare` fails.
    fn java_class_name(&self) -> &str {
        ""
    }
    /// True for the Java `null` key (tree tie-break treats it specially).
    fn java_is_null(&self) -> bool {
        false
    }
}

/// `java.lang.String.hashCode` over UTF-16 code units.
pub fn string_hash_code(utf16: &[u16]) -> i32 {
    utf16.iter().fold(0i32, |h, &c| h.wrapping_mul(31).wrapping_add(i32::from(c)))
}

const NIL: usize = usize::MAX;
const DEFAULT_INITIAL_CAPACITY: usize = 16;
const MAXIMUM_CAPACITY: usize = 1 << 30;
const LOAD_FACTOR: f32 = 0.75;
const TREEIFY_THRESHOLD: usize = 8;
const UNTREEIFY_THRESHOLD: usize = 6;
const MIN_TREEIFY_CAPACITY: usize = 64;

/// `HashMap.hash`: `h ^ (h >>> 16)`.
fn spread<K: JavaHash>(k: &K) -> i32 {
    let h = k.java_hash_code();
    h ^ ((h as u32) >> 16) as i32
}

fn bucket(hash: i32, n: usize) -> usize {
    (hash as u32 as usize) & (n - 1)
}

/// `HashMap.tableSizeFor`.
fn table_size_for(cap: usize) -> usize {
    let cap = cap.min(MAXIMUM_CAPACITY) as u32;
    let n = u32::MAX.wrapping_shr(cap.wrapping_sub(1).leading_zeros()) as i32;
    if n < 0 {
        1
    } else {
        (n as usize + 1).min(MAXIMUM_CAPACITY)
    }
}

/// `compareComparables`, 0 (Equal) when not mutually comparable.
fn compare_comparables<K: JavaHash>(k: &K, x: &K) -> Ordering {
    k.java_compare(x).unwrap_or(Ordering::Equal)
}

/// `TreeNode.tieBreakOrder`; never Equal.
fn tie_break_order<K: JavaHash>(a: &K, b: &K) -> Ordering {
    if !a.java_is_null() && !b.java_is_null() {
        let d = a.java_class_name().cmp(b.java_class_name());
        if d != Ordering::Equal {
            return d;
        }
    }
    // identityHashCode(a) <= identityHashCode(b) ? -1 : 1, with identityHashCode(null) == 0 and
    // every other identity hash > 0. Two non-null keys: see the ceiling in the header.
    if b.java_is_null() && !a.java_is_null() {
        Ordering::Greater
    } else {
        Ordering::Less
    }
}

#[derive(Clone)]
struct Node<K, V> {
    hash: i32,
    key: K,
    value: V,
    next: usize,
    // TreeNode fields; meaningful only while `tree`.
    tree: bool,
    red: bool,
    parent: usize,
    left: usize,
    right: usize,
    prev: usize,
    // LinkedHashMap.Entry fields; meaningful only in linked mode.
    before: usize,
    after: usize,
}

/// `java.util.HashMap`, or `java.util.LinkedHashMap` (insertion order) when created linked.
#[derive(Clone)]
pub struct JHashMap<K, V> {
    nodes: Vec<Node<K, V>>,
    table: Vec<usize>,
    /// Java `threshold`; before allocation it holds the initial capacity (0 = default).
    threshold: i32,
    linked: bool,
    head: usize,
    tail: usize,
}

impl<K: JavaHash + Clone, V: Clone> Default for JHashMap<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: JavaHash + Clone, V: Clone> JHashMap<K, V> {
    fn empty(threshold: usize, linked: bool) -> Self {
        JHashMap { nodes: Vec::new(), table: Vec::new(), threshold: threshold as i32, linked, head: NIL, tail: NIL }
    }

    /// `new HashMap<>()`
    pub fn new() -> Self {
        Self::empty(0, false)
    }
    /// `new HashMap<>(initialCapacity)`
    pub fn with_capacity(initial: usize) -> Self {
        Self::empty(table_size_for(initial), false)
    }
    /// `new LinkedHashMap<>()`
    pub fn new_linked() -> Self {
        Self::empty(0, true)
    }
    /// `new LinkedHashMap<>(initialCapacity)`
    pub fn linked_with_capacity(initial: usize) -> Self {
        Self::empty(table_size_for(initial), true)
    }
    /// `new HashMap<>(other)`
    pub fn copy_of(other: &Self) -> Self {
        Self::copied(other, false)
    }
    /// `new LinkedHashMap<>(other)`
    pub fn linked_copy_of(other: &Self) -> Self {
        Self::copied(other, true)
    }

    /// `putMapEntries(m, false)` into a fresh map: pre-size to `tableSizeFor(ceil(s / 0.75))`,
    /// then put in `other`'s iteration order.
    // ponytail: no `putAll` into a non-empty map (its repeated-resize branch); add it with a JVM
    // fixture op if a caller needs `Map.putAll`.
    fn copied(other: &Self, linked: bool) -> Self {
        let mut m = Self::empty(0, linked);
        if !other.is_empty() {
            let t = (other.len() as f64 / f64::from(LOAD_FACTOR)).ceil() as usize;
            m.threshold = table_size_for(t) as i32;
        }
        for (k, v) in other.iter() {
            m.put_val(k.clone(), v.clone(), false);
        }
        m
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
    /// Current table length (0 before the first insertion), for tests.
    pub fn capacity(&self) -> usize {
        self.table.len()
    }

    pub fn get(&self, k: &K) -> Option<&V> {
        let e = self.get_node(k);
        (e != NIL).then(|| &self.nodes[e].value)
    }
    pub fn get_mut(&mut self, k: &K) -> Option<&mut V> {
        let e = self.get_node(k);
        (e != NIL).then(|| &mut self.nodes[e].value)
    }
    pub fn contains_key(&self, k: &K) -> bool {
        self.get_node(k) != NIL
    }

    /// `put`: returns the previous value; an existing mapping keeps its original key object.
    pub fn put(&mut self, k: K, v: V) -> Option<V> {
        self.put_val(k, v, false)
    }
    /// `putIfAbsent` for non-null values: an existing value is left alone and returned.
    /// (Java also replaces an existing `null` value; a caller with nullable `V` checks
    /// `get` for its null first and uses `put`, which changes no order.)
    pub fn put_if_absent(&mut self, k: K, v: V) -> Option<V> {
        self.put_val(k, v, true)
    }
    pub fn remove(&mut self, k: &K) -> Option<V> {
        let e = self.remove_node(k, true);
        (e != NIL).then(|| self.free(e).0)
    }
    /// `clear`: keeps the table length, like Java.
    pub fn clear(&mut self) {
        self.nodes.clear();
        self.table.iter_mut().for_each(|b| *b = NIL);
        self.head = NIL;
        self.tail = NIL;
    }

    /// Entries in Java iteration order.
    pub fn iter(&self) -> Iter<'_, K, V> {
        Iter { map: self, cur: self.first_node() }
    }
    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.iter().map(|(k, _)| k)
    }
    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.iter().map(|(_, v)| v)
    }

    /// Removes entries for which `f` is false, visiting in iteration order with `Iterator.remove`
    /// semantics (`removeNode(..., movable = false)`: tree bins are neither untreeified nor re-rooted).
    pub fn retain(&mut self, mut f: impl FnMut(&K, &V) -> bool) {
        let mut cur = self.first_node();
        while cur != NIL {
            let mut next = self.next_node(cur);
            let n = &self.nodes[cur];
            if !f(&n.key, &n.value) {
                let key = n.key.clone();
                let e = self.remove_node(&key, false);
                if e != NIL {
                    let moved = self.free(e).1;
                    if moved != NIL && moved == next {
                        next = e; // `next` was the last arena slot and moved into `e`
                    }
                }
            }
            cur = next;
        }
    }

    /// `containsValue`; `eq(probe, stored)` plays `value.equals(v)`.
    pub fn contains_value(&self, v: &V, eq: impl Fn(&V, &V) -> bool) -> bool {
        self.values().any(|x| eq(v, x))
    }

    /// `AbstractMap.hashCode`: sum of `key.hashCode() ^ value.hashCode()`.
    pub fn hash_code(&self, value_hash: impl Fn(&V) -> i32) -> i32 {
        self.iter().fold(0i32, |h, (k, v)| h.wrapping_add(k.java_hash_code() ^ value_hash(v)))
    }

    // ---------------- iteration ----------------

    fn first_node(&self) -> usize {
        if self.linked {
            self.head
        } else {
            self.first_in_table_from(0)
        }
    }

    fn first_in_table_from(&self, i: usize) -> usize {
        self.table.get(i..).and_then(|t| t.iter().copied().find(|&b| b != NIL)).unwrap_or(NIL)
    }

    /// `HashIterator.nextNode` / `LinkedHashIterator.nextNode`.
    fn next_node(&self, e: usize) -> usize {
        let n = &self.nodes[e];
        if self.linked {
            n.after
        } else if n.next != NIL {
            n.next
        } else {
            self.first_in_table_from(bucket(n.hash, self.table.len()) + 1)
        }
    }

    // ---------------- arena ----------------

    /// `newNode` / `newTreeNode` (LinkedHashMap links it last).
    fn new_node(&mut self, hash: i32, key: K, value: V, next: usize, tree: bool) -> usize {
        let x = self.nodes.len();
        self.nodes.push(Node {
            hash,
            key,
            value,
            next,
            tree,
            red: false,
            parent: NIL,
            left: NIL,
            right: NIL,
            prev: NIL,
            before: self.tail,
            after: NIL,
        });
        if self.linked {
            if self.tail == NIL {
                self.head = x;
            } else {
                self.nodes[self.tail].after = x;
            }
            self.tail = x;
        }
        x
    }

    /// Drops an unlinked node; returns its value and the index of the node moved into its slot
    /// (NIL when none moved).
    fn free(&mut self, e: usize) -> (V, usize) {
        let last = self.nodes.len() - 1;
        let removed = self.nodes.swap_remove(e);
        if e == last {
            return (removed.value, NIL);
        }
        self.relink(last, e);
        (removed.value, last)
    }

    /// Re-points every link to the node that moved from arena slot `from` to `to`.
    fn relink(&mut self, from: usize, to: usize) {
        let n = &self.nodes[to];
        let (next, tree, parent, left, right, prev) = (n.next, n.tree, n.parent, n.left, n.right, n.prev);
        let (before, after) = (n.before, n.after);
        let b = bucket(n.hash, self.table.len());
        if tree && prev != NIL {
            self.nodes[prev].next = to;
        } else if self.table[b] == from {
            self.table[b] = to;
        } else {
            let mut q = self.table[b];
            while self.nodes[q].next != from {
                q = self.nodes[q].next;
            }
            self.nodes[q].next = to;
        }
        if tree {
            if next != NIL {
                self.nodes[next].prev = to;
            }
            if parent != NIL {
                let p = &mut self.nodes[parent];
                if p.left == from {
                    p.left = to;
                } else {
                    p.right = to;
                }
            }
            if left != NIL {
                self.nodes[left].parent = to;
            }
            if right != NIL {
                self.nodes[right].parent = to;
            }
        }
        if self.linked {
            if before == NIL {
                self.head = to;
            } else {
                self.nodes[before].after = to;
            }
            if after == NIL {
                self.tail = to;
            } else {
                self.nodes[after].before = to;
            }
        }
    }

    /// LinkedHashMap `afterNodeRemoval`.
    fn unlink_linked(&mut self, e: usize) {
        let (b, a) = (self.nodes[e].before, self.nodes[e].after);
        if b == NIL {
            self.head = a;
        } else {
            self.nodes[b].after = a;
        }
        if a == NIL {
            self.tail = b;
        } else {
            self.nodes[a].before = b;
        }
    }

    // ---------------- HashMap core ----------------

    fn matches(&self, e: usize, hash: i32, k: &K) -> bool {
        self.nodes[e].hash == hash && k.java_equals(&self.nodes[e].key)
    }

    /// `getNode`
    fn get_node(&self, k: &K) -> usize {
        let n = self.table.len();
        if n == 0 {
            return NIL;
        }
        let hash = spread(k);
        let first = self.table[bucket(hash, n)];
        if first == NIL || self.matches(first, hash, k) {
            return first;
        }
        let mut e = self.nodes[first].next;
        if e != NIL && self.nodes[first].tree {
            return self.get_tree_node(first, hash, k);
        }
        while e != NIL {
            if self.matches(e, hash, k) {
                return e;
            }
            e = self.nodes[e].next;
        }
        NIL
    }

    /// Stores into an existing node (`putVal`'s `e != null` branch).
    fn update(&mut self, e: usize, value: V, only_if_absent: bool) -> Option<V> {
        let slot = &mut self.nodes[e].value;
        Some(if only_if_absent { slot.clone() } else { std::mem::replace(slot, value) })
    }

    /// `putVal`
    fn put_val(&mut self, key: K, value: V, only_if_absent: bool) -> Option<V> {
        let hash = spread(&key);
        if self.table.is_empty() {
            self.resize();
        }
        let i = bucket(hash, self.table.len());
        let mut p = self.table[i];
        if p == NIL {
            self.table[i] = self.new_node(hash, key, value, NIL, false);
        } else if self.matches(p, hash, &key) {
            return self.update(p, value, only_if_absent);
        } else if self.nodes[p].tree {
            if let Some((e, value)) = self.put_tree_val(p, hash, key, value) {
                return self.update(e, value, only_if_absent);
            }
        } else {
            let mut bin_count = 0;
            loop {
                let e = self.nodes[p].next;
                if e == NIL {
                    self.nodes[p].next = self.new_node(hash, key, value, NIL, false);
                    if bin_count >= TREEIFY_THRESHOLD - 1 {
                        self.treeify_bin(hash);
                    }
                    break;
                }
                if self.matches(e, hash, &key) {
                    return self.update(e, value, only_if_absent);
                }
                p = e;
                bin_count += 1;
            }
        }
        if self.nodes.len() as i64 > i64::from(self.threshold) {
            self.resize();
        }
        None
    }

    /// `resize`
    fn resize(&mut self) {
        let old_cap = self.table.len();
        let old_thr = self.threshold;
        let mut new_thr = 0i32;
        let new_cap;
        if old_cap > 0 {
            if old_cap >= MAXIMUM_CAPACITY {
                self.threshold = i32::MAX;
                return;
            }
            new_cap = old_cap << 1;
            if new_cap < MAXIMUM_CAPACITY && old_cap >= DEFAULT_INITIAL_CAPACITY {
                new_thr = old_thr << 1;
            }
        } else if old_thr > 0 {
            new_cap = old_thr as usize;
        } else {
            new_cap = DEFAULT_INITIAL_CAPACITY;
            new_thr = (LOAD_FACTOR * DEFAULT_INITIAL_CAPACITY as f32) as i32;
        }
        if new_thr == 0 {
            let ft = new_cap as f32 * LOAD_FACTOR;
            new_thr = if new_cap < MAXIMUM_CAPACITY && ft < MAXIMUM_CAPACITY as f32 { ft as i32 } else { i32::MAX };
        }
        self.threshold = new_thr;
        let old = std::mem::replace(&mut self.table, vec![NIL; new_cap]);
        for (j, &e) in old.iter().enumerate() {
            if e == NIL {
                continue;
            }
            if self.nodes[e].next == NIL {
                self.table[bucket(self.nodes[e].hash, new_cap)] = e;
            } else if self.nodes[e].tree {
                self.split(e, j, old_cap);
            } else {
                // preserve order
                let (mut lo_head, mut lo_tail, mut hi_head, mut hi_tail) = (NIL, NIL, NIL, NIL);
                let mut e = e;
                while e != NIL {
                    let next = self.nodes[e].next;
                    let (head, tail) = if (self.nodes[e].hash as u32 as usize) & old_cap == 0 {
                        (&mut lo_head, &mut lo_tail)
                    } else {
                        (&mut hi_head, &mut hi_tail)
                    };
                    if *tail == NIL {
                        *head = e;
                    } else {
                        self.nodes[*tail].next = e;
                    }
                    *tail = e;
                    e = next;
                }
                if lo_tail != NIL {
                    self.nodes[lo_tail].next = NIL;
                    self.table[j] = lo_head;
                }
                if hi_tail != NIL {
                    self.nodes[hi_tail].next = NIL;
                    self.table[j + old_cap] = hi_head;
                }
            }
        }
    }

    /// `treeifyBin`
    fn treeify_bin(&mut self, hash: i32) {
        let n = self.table.len();
        if n < MIN_TREEIFY_CAPACITY {
            self.resize();
            return;
        }
        let hd = self.table[bucket(hash, n)];
        let (mut e, mut tl) = (hd, NIL);
        while e != NIL {
            let x = &mut self.nodes[e];
            x.tree = true;
            x.prev = tl;
            tl = e;
            e = x.next;
        }
        if hd != NIL {
            self.treeify(hd);
        }
    }

    /// `removeNode(hash(key), key, null, false, movable)`: unlinks and returns the node (still in
    /// the arena; the caller frees it), or NIL.
    fn remove_node(&mut self, k: &K, movable: bool) -> usize {
        let n = self.table.len();
        if n == 0 {
            return NIL;
        }
        let hash = spread(k);
        let index = bucket(hash, n);
        let mut p = self.table[index];
        if p == NIL {
            return NIL;
        }
        let mut node = NIL;
        if self.matches(p, hash, k) {
            node = p;
        } else {
            let mut e = self.nodes[p].next;
            if e != NIL {
                if self.nodes[p].tree {
                    node = self.get_tree_node(p, hash, k);
                } else {
                    while e != NIL {
                        if self.matches(e, hash, k) {
                            node = e;
                            break;
                        }
                        p = e;
                        e = self.nodes[e].next;
                    }
                }
            }
        }
        if node == NIL {
            return NIL;
        }
        if self.nodes[node].tree {
            self.remove_tree_node(node, movable);
        } else if node == p {
            self.table[index] = self.nodes[node].next;
        } else {
            self.nodes[p].next = self.nodes[node].next;
        }
        if self.linked {
            self.unlink_linked(node);
        }
        node
    }

    // ---------------- TreeNode ----------------

    fn root(&self, mut r: usize) -> usize {
        while self.nodes[r].parent != NIL {
            r = self.nodes[r].parent;
        }
        r
    }

    fn is_red(&self, x: usize) -> bool {
        x != NIL && self.nodes[x].red
    }

    fn move_root_to_front(&mut self, root: usize) {
        let n = self.table.len();
        if root == NIL || n == 0 {
            return;
        }
        let index = bucket(self.nodes[root].hash, n);
        let first = self.table[index];
        if root != first {
            self.table[index] = root;
            let (rp, rn) = (self.nodes[root].prev, self.nodes[root].next);
            if rn != NIL {
                self.nodes[rn].prev = rp;
            }
            if rp != NIL {
                self.nodes[rp].next = rn;
            }
            if first != NIL {
                self.nodes[first].prev = root;
            }
            self.nodes[root].next = first;
            self.nodes[root].prev = NIL;
        }
    }

    /// `find(h, k, kc)` from `p`.
    fn find(&self, mut p: usize, h: i32, k: &K) -> usize {
        while p != NIL {
            let n = &self.nodes[p];
            let (pl, pr) = (n.left, n.right);
            p = if n.hash > h {
                pl
            } else if n.hash < h {
                pr
            } else if k.java_equals(&n.key) {
                return p;
            } else if pl == NIL {
                pr
            } else if pr == NIL {
                pl
            } else {
                match compare_comparables(k, &n.key) {
                    Ordering::Less => pl,
                    Ordering::Greater => pr,
                    Ordering::Equal => {
                        let q = self.find(pr, h, k);
                        if q != NIL {
                            return q;
                        }
                        pl
                    }
                }
            };
        }
        NIL
    }

    fn get_tree_node(&self, e: usize, h: i32, k: &K) -> usize {
        self.find(self.root(e), h, k)
    }

    /// Direction of `k` (hash `h`) relative to node `p` for insertion; Equal never returned.
    fn tree_dir(&self, h: i32, k: &K, p: usize) -> Ordering {
        let n = &self.nodes[p];
        match n.hash.cmp(&h) {
            Ordering::Greater => Ordering::Less,
            Ordering::Less => Ordering::Greater,
            Ordering::Equal => match compare_comparables(k, &n.key) {
                Ordering::Equal => tie_break_order(k, &n.key),
                d => d,
            },
        }
    }

    /// Links `x` as the `dir` child of `xp`.
    fn attach(&mut self, xp: usize, x: usize, dir: Ordering) {
        self.nodes[x].parent = xp;
        if dir == Ordering::Greater {
            self.nodes[xp].right = x;
        } else {
            self.nodes[xp].left = x;
        }
    }

    fn child(&self, p: usize, dir: Ordering) -> usize {
        if dir == Ordering::Greater {
            self.nodes[p].right
        } else {
            self.nodes[p].left
        }
    }

    /// `treeify` the bin whose `next` list starts at `head`.
    fn treeify(&mut self, head: usize) {
        let mut root = NIL;
        let mut x = head;
        while x != NIL {
            let next = self.nodes[x].next;
            self.nodes[x].left = NIL;
            self.nodes[x].right = NIL;
            if root == NIL {
                self.nodes[x].parent = NIL;
                self.nodes[x].red = false;
                root = x;
            } else {
                let h = self.nodes[x].hash;
                let mut p = root;
                loop {
                    let dir = self.tree_dir(h, &self.nodes[x].key, p);
                    let xp = p;
                    p = self.child(xp, dir);
                    if p == NIL {
                        self.attach(xp, x, dir);
                        root = self.balance_insertion(root, x);
                        break;
                    }
                }
            }
            x = next;
        }
        self.move_root_to_front(root);
    }

    /// `untreeify`: the bin becomes plain nodes in the same `next` order.
    fn untreeify(&mut self, head: usize) -> usize {
        let mut q = head;
        while q != NIL {
            self.nodes[q].tree = false;
            q = self.nodes[q].next;
        }
        head
    }

    /// `putTreeVal`: Some((existing node, value back)) when the key is present.
    fn put_tree_val(&mut self, first: usize, h: i32, k: K, v: V) -> Option<(usize, V)> {
        let root = self.root(first);
        let mut searched = false;
        let mut p = root;
        loop {
            let n = &self.nodes[p];
            let dir = match n.hash.cmp(&h) {
                Ordering::Greater => Ordering::Less,
                Ordering::Less => Ordering::Greater,
                Ordering::Equal if k.java_equals(&n.key) => return Some((p, v)),
                Ordering::Equal => match compare_comparables(&k, &n.key) {
                    Ordering::Equal => {
                        if !searched {
                            searched = true;
                            for ch in [n.left, n.right] {
                                let q = if ch == NIL { NIL } else { self.find(ch, h, &k) };
                                if q != NIL {
                                    return Some((q, v));
                                }
                            }
                        }
                        tie_break_order(&k, &self.nodes[p].key)
                    }
                    d => d,
                },
            };
            let xp = p;
            p = self.child(xp, dir);
            if p == NIL {
                let xpn = self.nodes[xp].next;
                let x = self.new_node(h, k, v, xpn, true);
                self.attach(xp, x, dir);
                self.nodes[xp].next = x;
                self.nodes[x].prev = xp;
                if xpn != NIL {
                    self.nodes[xpn].prev = x;
                }
                let r = self.balance_insertion(root, x);
                self.move_root_to_front(r);
                return None;
            }
        }
    }

    /// `removeTreeNode` for node `p`.
    fn remove_tree_node(&mut self, p: usize, movable: bool) {
        let n = self.table.len();
        let index = bucket(self.nodes[p].hash, n);
        let mut first = self.table[index];
        let mut root = first;
        let (succ, pred) = (self.nodes[p].next, self.nodes[p].prev);
        if pred == NIL {
            self.table[index] = succ;
            first = succ;
        } else {
            self.nodes[pred].next = succ;
        }
        if succ != NIL {
            self.nodes[succ].prev = pred;
        }
        if first == NIL {
            return;
        }
        root = self.root(root);
        if movable && {
            let r = &self.nodes[root];
            r.right == NIL || r.left == NIL || self.nodes[r.left].left == NIL
        } {
            self.table[index] = self.untreeify(first); // too small
            return;
        }
        let (pl, pr) = (self.nodes[p].left, self.nodes[p].right);
        let replacement = if pl != NIL && pr != NIL {
            let mut s = pr;
            while self.nodes[s].left != NIL {
                s = self.nodes[s].left;
            }
            let c = self.nodes[s].red;
            self.nodes[s].red = self.nodes[p].red;
            self.nodes[p].red = c; // swap colors
            let sr = self.nodes[s].right;
            let pp = self.nodes[p].parent;
            if s == pr {
                // p was s's direct parent
                self.nodes[p].parent = s;
                self.nodes[s].right = p;
            } else {
                let sp = self.nodes[s].parent;
                self.nodes[p].parent = sp;
                if sp != NIL {
                    if s == self.nodes[sp].left {
                        self.nodes[sp].left = p;
                    } else {
                        self.nodes[sp].right = p;
                    }
                }
                self.nodes[s].right = pr;
                self.nodes[pr].parent = s;
            }
            self.nodes[p].left = NIL;
            self.nodes[p].right = sr;
            if sr != NIL {
                self.nodes[sr].parent = p;
            }
            self.nodes[s].left = pl;
            self.nodes[pl].parent = s;
            self.nodes[s].parent = pp;
            if pp == NIL {
                root = s;
            } else if p == self.nodes[pp].left {
                self.nodes[pp].left = s;
            } else {
                self.nodes[pp].right = s;
            }
            if sr != NIL { sr } else { p }
        } else if pl != NIL {
            pl
        } else if pr != NIL {
            pr
        } else {
            p
        };
        if replacement != p {
            let pp = self.nodes[p].parent;
            self.nodes[replacement].parent = pp;
            if pp == NIL {
                root = replacement;
                self.nodes[replacement].red = false;
            } else if p == self.nodes[pp].left {
                self.nodes[pp].left = replacement;
            } else {
                self.nodes[pp].right = replacement;
            }
            let x = &mut self.nodes[p];
            x.left = NIL;
            x.right = NIL;
            x.parent = NIL;
        }
        let r = if self.nodes[p].red { root } else { self.balance_deletion(root, replacement) };
        if replacement == p {
            // detach
            let pp = self.nodes[p].parent;
            self.nodes[p].parent = NIL;
            if pp != NIL {
                if p == self.nodes[pp].left {
                    self.nodes[pp].left = NIL;
                } else if p == self.nodes[pp].right {
                    self.nodes[pp].right = NIL;
                }
            }
        }
        if movable {
            self.move_root_to_front(r);
        }
    }

    /// `split` a tree bin at old index `index` during resize (`bit` = old capacity).
    fn split(&mut self, b: usize, index: usize, bit: usize) {
        let (mut lo_head, mut lo_tail, mut hi_head, mut hi_tail) = (NIL, NIL, NIL, NIL);
        let (mut lc, mut hc) = (0, 0);
        let mut e = b;
        while e != NIL {
            let next = self.nodes[e].next;
            self.nodes[e].next = NIL;
            let lo = (self.nodes[e].hash as u32 as usize) & bit == 0;
            let (head, tail, count) =
                if lo { (&mut lo_head, &mut lo_tail, &mut lc) } else { (&mut hi_head, &mut hi_tail, &mut hc) };
            self.nodes[e].prev = *tail;
            if *tail == NIL {
                *head = e;
            } else {
                self.nodes[*tail].next = e;
            }
            *tail = e;
            *count += 1;
            e = next;
        }
        for (head, count, other, at) in [(lo_head, lc, hi_head, index), (hi_head, hc, lo_head, index + bit)] {
            if head == NIL {
                continue;
            }
            if count <= UNTREEIFY_THRESHOLD {
                self.table[at] = self.untreeify(head);
            } else {
                self.table[at] = head;
                if other != NIL {
                    // (else is already treeified)
                    self.treeify(head);
                }
            }
        }
    }

    // ---------------- red-black tree, adapted from CLR (as in the JDK) ----------------

    fn rotate_left(&mut self, mut root: usize, p: usize) -> usize {
        let r = if p == NIL { NIL } else { self.nodes[p].right };
        if r != NIL {
            let rl = self.nodes[r].left;
            self.nodes[p].right = rl;
            if rl != NIL {
                self.nodes[rl].parent = p;
            }
            let pp = self.nodes[p].parent;
            self.nodes[r].parent = pp;
            if pp == NIL {
                root = r;
                self.nodes[r].red = false;
            } else if self.nodes[pp].left == p {
                self.nodes[pp].left = r;
            } else {
                self.nodes[pp].right = r;
            }
            self.nodes[r].left = p;
            self.nodes[p].parent = r;
        }
        root
    }

    fn rotate_right(&mut self, mut root: usize, p: usize) -> usize {
        let l = if p == NIL { NIL } else { self.nodes[p].left };
        if l != NIL {
            let lr = self.nodes[l].right;
            self.nodes[p].left = lr;
            if lr != NIL {
                self.nodes[lr].parent = p;
            }
            let pp = self.nodes[p].parent;
            self.nodes[l].parent = pp;
            if pp == NIL {
                root = l;
                self.nodes[l].red = false;
            } else if self.nodes[pp].right == p {
                self.nodes[pp].right = l;
            } else {
                self.nodes[pp].left = l;
            }
            self.nodes[l].right = p;
            self.nodes[p].parent = l;
        }
        root
    }

    fn parent_of(&self, x: usize) -> usize {
        if x == NIL {
            NIL
        } else {
            self.nodes[x].parent
        }
    }

    fn balance_insertion(&mut self, mut root: usize, mut x: usize) -> usize {
        self.nodes[x].red = true;
        loop {
            let mut xp = self.nodes[x].parent;
            if xp == NIL {
                self.nodes[x].red = false;
                return x;
            }
            let mut xpp = self.nodes[xp].parent;
            if !self.nodes[xp].red || xpp == NIL {
                return root;
            }
            let (xppl, xppr) = (self.nodes[xpp].left, self.nodes[xpp].right);
            let left_side = xp == xppl;
            let uncle = if left_side { xppr } else { xppl };
            if self.is_red(uncle) {
                self.nodes[uncle].red = false;
                self.nodes[xp].red = false;
                self.nodes[xpp].red = true;
                x = xpp;
                continue;
            }
            let inner = if left_side { self.nodes[xp].right } else { self.nodes[xp].left };
            if x == inner {
                x = xp;
                root = if left_side { self.rotate_left(root, x) } else { self.rotate_right(root, x) };
                xp = self.nodes[x].parent;
                xpp = self.parent_of(xp);
            }
            if xp != NIL {
                self.nodes[xp].red = false;
                if xpp != NIL {
                    self.nodes[xpp].red = true;
                    root = if left_side { self.rotate_right(root, xpp) } else { self.rotate_left(root, xpp) };
                }
            }
        }
    }

    fn balance_deletion(&mut self, mut root: usize, mut x: usize) -> usize {
        loop {
            if x == NIL || x == root {
                return root;
            }
            let mut xp = self.nodes[x].parent;
            if xp == NIL {
                self.nodes[x].red = false;
                return x;
            }
            if self.nodes[x].red {
                self.nodes[x].red = false;
                return root;
            }
            // `left_side`: x is xp's left child; the JDK's two symmetric branches, mirrored here
            // by swapping which child/rotation is "near" and "far".
            let left_side = self.nodes[xp].left == x;
            let sib = |m: &Self, xp: usize| {
                if xp == NIL {
                    NIL
                } else if left_side {
                    m.nodes[xp].right
                } else {
                    m.nodes[xp].left
                }
            };
            let rot_toward_x = |m: &mut Self, root: usize, p: usize| {
                if left_side {
                    m.rotate_left(root, p)
                } else {
                    m.rotate_right(root, p)
                }
            };
            let rot_away_x = |m: &mut Self, root: usize, p: usize| {
                if left_side {
                    m.rotate_right(root, p)
                } else {
                    m.rotate_left(root, p)
                }
            };
            let mut w = sib(self, xp);
            if self.is_red(w) {
                self.nodes[w].red = false;
                self.nodes[xp].red = true;
                root = rot_toward_x(self, root, xp);
                xp = self.nodes[x].parent;
                w = sib(self, xp);
            }
            if w == NIL {
                x = xp;
                continue;
            }
            let (near, far) = if left_side {
                (self.nodes[w].left, self.nodes[w].right)
            } else {
                (self.nodes[w].right, self.nodes[w].left)
            };
            if !self.is_red(far) && !self.is_red(near) {
                self.nodes[w].red = true;
                x = xp;
                continue;
            }
            if !self.is_red(far) {
                if near != NIL {
                    self.nodes[near].red = false;
                }
                self.nodes[w].red = true;
                root = rot_away_x(self, root, w);
                xp = self.nodes[x].parent;
                w = sib(self, xp);
            }
            if w != NIL {
                self.nodes[w].red = xp != NIL && self.nodes[xp].red;
                let far = if left_side { self.nodes[w].right } else { self.nodes[w].left };
                if far != NIL {
                    self.nodes[far].red = false;
                }
            }
            if xp != NIL {
                self.nodes[xp].red = false;
                root = rot_toward_x(self, root, xp);
            }
            x = root;
        }
    }
}

/// Iterator over a `JHashMap` in Java order.
pub struct Iter<'a, K, V> {
    map: &'a JHashMap<K, V>,
    cur: usize,
}

impl<'a, K: JavaHash + Clone, V: Clone> Iterator for Iter<'a, K, V> {
    type Item = (&'a K, &'a V);
    fn next(&mut self) -> Option<Self::Item> {
        if self.cur == NIL {
            return None;
        }
        let n = &self.map.nodes[self.cur];
        self.cur = self.map.next_node(self.cur);
        Some((&n.key, &n.value))
    }
}

/// `java.util.HashSet`, or `java.util.LinkedHashSet` when created linked.
#[derive(Clone)]
pub struct JHashSet<K> {
    map: JHashMap<K, ()>,
}

impl<K: JavaHash + Clone> Default for JHashSet<K> {
    fn default() -> Self {
        Self::new()
    }
}

/// `HashMap.calculateHashMapCapacity(Math.max(n, 12))`: the JDK 19+ `HashSet(Collection)` /
/// `LinkedHashSet(Collection)` sizing (JDK 8-18 used `max((int)(n/.75f) + 1, 16)`).
fn collection_capacity(n: usize) -> usize {
    (n.max(12) as f64 / f64::from(LOAD_FACTOR)).ceil() as usize
}

impl<K: JavaHash + Clone> JHashSet<K> {
    /// `new HashSet<>()`
    pub fn new() -> Self {
        JHashSet { map: JHashMap::new() }
    }
    /// `new HashSet<>(initialCapacity)`
    pub fn with_capacity(initial: usize) -> Self {
        JHashSet { map: JHashMap::with_capacity(initial) }
    }
    /// `new LinkedHashSet<>()` (= `LinkedHashMap(16, .75f)`, same table as the default).
    pub fn new_linked() -> Self {
        JHashSet { map: JHashMap::new_linked() }
    }
    /// `new LinkedHashSet<>(initialCapacity)`
    pub fn linked_with_capacity(initial: usize) -> Self {
        JHashSet { map: JHashMap::linked_with_capacity(initial) }
    }
    /// Empty set sized like `new HashSet<>(c)` for a collection of `len` elements; add them next.
    pub fn from_collection(len: usize) -> Self {
        Self::with_capacity(collection_capacity(len))
    }
    /// Empty set sized like `new LinkedHashSet<>(c)` for a collection of `len` elements.
    pub fn linked_from_collection(len: usize) -> Self {
        Self::linked_with_capacity(collection_capacity(len))
    }
    /// `new HashSet<>(other)`
    pub fn copy_of(other: &Self) -> Self {
        let mut s = Self::from_collection(other.len());
        for k in other.iter() {
            s.add(k.clone());
        }
        s
    }
    /// `new LinkedHashSet<>(other)`
    pub fn linked_copy_of(other: &Self) -> Self {
        let mut s = Self::linked_from_collection(other.len());
        for k in other.iter() {
            s.add(k.clone());
        }
        s
    }
    /// `add`: true when newly added.
    pub fn add(&mut self, k: K) -> bool {
        self.map.put(k, ()).is_none()
    }
    pub fn contains(&self, k: &K) -> bool {
        self.map.contains_key(k)
    }
    pub fn remove(&mut self, k: &K) -> bool {
        self.map.remove(k).is_some()
    }
    pub fn len(&self) -> usize {
        self.map.len()
    }
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
    pub fn clear(&mut self) {
        self.map.clear()
    }
    /// Current table length (0 before the first insertion), for tests.
    pub fn capacity(&self) -> usize {
        self.map.capacity()
    }
    /// Elements in Java iteration order.
    pub fn iter(&self) -> impl Iterator<Item = &K> {
        self.map.keys()
    }
    /// `Iterator.remove` semantics, see `JHashMap::retain`.
    pub fn retain(&mut self, mut f: impl FnMut(&K) -> bool) {
        self.map.retain(|k, _| f(k))
    }
    /// `AbstractSet.hashCode`: sum of element hash codes.
    pub fn hash_code(&self) -> i32 {
        self.map.hash_code(|_| 0)
    }
}
