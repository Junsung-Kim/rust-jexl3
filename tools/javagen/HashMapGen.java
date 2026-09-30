// Oracle generator for src/java/hash_map.rs (java.util.HashMap / HashSet / LinkedHashMap / LinkedHashSet).
//
//   java --add-opens java.base/java.util=ALL-UNNAMED tools/javagen/HashMapGen.java tests/data/java_hash_map
//
// Writes small.txt (random short sequences) and tree.txt (collision-heavy sequences that treeify,
// untreeify and split tree bins). Deterministic seed. Format, one case per block:
//   C <mode> <ctor>        mode: M HashMap, LM LinkedHashMap, S HashSet, LS LinkedHashSet
//                          ctor: D (no-arg) or N<n> (initialCapacity n)
//   K <key>...             key pool; I<int> L<long> S<utf16 hex, 4 digits each> D<raw long bits hex> N(null)
//   O <op>...              p<i> put/add pool key i (value = op position), a<i> putIfAbsent, r<i> remove,
//                          c clear, x<m>_<r> iterator-remove every entry whose position % m == r,
//                          y copy (new HashMap<>(map) / new LinkedHashMap<>(map) / new HashSet<>(set) / ...),
//                          = snapshot (one E line each, in order)
//   E <capacity> <hashCode> <pool index in iteration order>...
import java.io.*;
import java.lang.reflect.Field;
import java.util.*;

public class HashMapGen {
    static final Random R = new Random(20260930L);
    static Field TABLE, SETMAP;

    public static void main(String[] args) throws Exception {
        TABLE = HashMap.class.getDeclaredField("table");
        TABLE.setAccessible(true);
        SETMAP = HashSet.class.getDeclaredField("map");
        SETMAP.setAccessible(true);
        File dir = new File(args[0]);
        dir.mkdirs();
        try (PrintWriter w = writer(new File(dir, "small.txt"))) {
            for (int i = 0; i < 20000; i++) small(w);
        }
        try (PrintWriter w = writer(new File(dir, "tree.txt"))) {
            for (int i = 0; i < 700; i++) tree(w);
        }
    }

    static PrintWriter writer(File f) throws IOException {
        return new PrintWriter(new BufferedWriter(new OutputStreamWriter(new FileOutputStream(f), "UTF-8")));
    }

    // ---------------- key pools ----------------

    static int randInt(int lo, int hi) { return lo + R.nextInt(hi - lo + 1); }

    static String randString(int kind) {
        int len = R.nextInt(6);
        StringBuilder sb = new StringBuilder();
        for (int i = 0; i < len; i++) {
            switch (kind) {
                case 0: sb.append("abcXYZ019"
                        .charAt(R.nextInt(9))); break;
                case 1: sb.append((char) (0xAC00 + R.nextInt(11172))); break; // Hangul
                case 2: sb.appendCodePoint(0x1F600 + R.nextInt(80)); break;   // emoji (surrogate pair)
                default: sb.append((char) R.nextInt(0xD800)); break;
            }
        }
        return sb.toString();
    }

    /** Strings of "Aa"/"BB"/"C#" blocks: all of a given block count share one hashCode. */
    static List<Object> collidingStrings(int blocks, int max) {
        String[] b = {"Aa", "BB", "C#"};
        List<Object> out = new ArrayList<>();
        int total = 1;
        for (int i = 0; i < blocks; i++) total *= 3;
        for (int n = 0; n < total && out.size() < max; n++) {
            StringBuilder sb = new StringBuilder();
            for (int i = 0, m = n; i < blocks; i++, m /= 3) sb.append(b[m % 3]);
            out.add(sb.toString());
        }
        return out;
    }

    static Long longWithHash(int h) {
        long hi = R.nextInt();
        return (hi << 32) | ((hi ^ h) & 0xffffffffL);
    }

    static Double doubleWithHash(int h) {
        long hi = 0x3FF00000L + R.nextInt(0x100000); // finite, never NaN
        if (R.nextBoolean()) hi |= 0x80000000L;       // negative too
        return Double.longBitsToDouble((hi << 32) | ((hi ^ h) & 0xffffffffL));
    }

    /** Two-char strings with hashCode h (h >= 0). */
    static String stringWithHash(int h) {
        int c0 = R.nextInt(Math.min(h / 31, 2000) + 1);
        return "" + (char) c0 + (char) (h - 31 * c0);
    }

    static final double[] SPECIAL = {0.0, -0.0, Double.NaN, Double.POSITIVE_INFINITY, Double.NEGATIVE_INFINITY,
            1.0, -1.0, 0.5, 2.0, 1e300, Double.MIN_VALUE, Double.MAX_VALUE, 3.0, 100.0};

    static Object randomKey(int family) {
        switch (family) {
            case 0: return randInt(-40, 40);
            case 1: return R.nextInt();
            case 2: { int[] s = {4, 5, 6, 7, 8, 10, 16, 20}; return randInt(-30, 30) << s[R.nextInt(s.length)]; }
            case 3: return R.nextBoolean() ? (long) randInt(-40, 40) : R.nextLong();
            case 4: return longWithHash(randInt(-5, 5));
            case 5: return randString(R.nextInt(4));
            case 6: return collidingStrings(randInt(1, 4), 81).get(R.nextInt(3));
            case 7: return R.nextBoolean() ? SPECIAL[R.nextInt(SPECIAL.length)] : (R.nextBoolean() ? (double) randInt(-20, 20) : R.nextDouble() * 100);
            case 8: return doubleWithHash(randInt(0, 5));
            case 9: return null;
            default: { // cross-type same hash
                int h = randInt(0, 3);
                switch (R.nextInt(4)) {
                    case 0: return h;
                    case 1: return (long) h;
                    case 2: return stringWithHash(h);
                    default: return doubleWithHash(h);
                }
            }
        }
    }

    static List<Object> dedupe(List<Object> keys) {
        return new ArrayList<>(new LinkedHashSet<>(keys));
    }

    static List<Object> smallPool() {
        int size = randInt(1, 18);
        int mix = R.nextInt(4); // 0: one family, else mixture
        int fam = R.nextInt(11);
        List<Object> keys = new ArrayList<>();
        for (int i = 0; i < size; i++) keys.add(randomKey(mix == 0 ? fam : R.nextInt(11)));
        return dedupe(keys);
    }

    static List<Object> treePool() {
        List<Object> keys = new ArrayList<>();
        int families = randInt(1, 3);
        for (int f = 0; f < families; f++) {
            int n = randInt(10, 90);
            switch (R.nextInt(8)) {
                case 0: { int step = new int[]{64, 128, 256, 1 << 16}[R.nextInt(4)];
                          for (int i = 0; i < n; i++) keys.add(randInt(-200, 200) * step); break; }
                case 1: keys.addAll(collidingStrings(randInt(3, 5), n)); break;
                case 2: { int h = randInt(-3, 3); for (int i = 0; i < n; i++) keys.add(longWithHash(h)); break; }
                case 3: { int h = randInt(0, 3); for (int i = 0; i < n; i++) keys.add(doubleWithHash(h)); break; }
                case 4: { int h = randInt(0, 2000); // cross-type same hash
                          for (int i = 0; i < n; i++) {
                              switch (R.nextInt(4)) {
                                  case 0: keys.add(h); break;
                                  case 1: keys.add((long) h); break;
                                  case 2: keys.add(stringWithHash(h)); break;
                                  default: keys.add(doubleWithHash(h)); break;
                              }
                          }
                          break; }
                case 5: { // hash-0 family incl. null
                          keys.add(null); keys.add(0); keys.add(0L); keys.add(""); keys.add(0.0);
                          for (int i = 0; i < n; i++) {
                              switch (R.nextInt(3)) {
                                  case 0: keys.add(longWithHash(0)); break;
                                  case 1: keys.add(doubleWithHash(0)); break;
                                  default: keys.add("\0".repeat(randInt(1, 6))); break;
                              }
                          }
                          break; }
                case 6: // same bucket, different hashes, mixed types
                    for (int i = 0; i < n; i++) {
                        int h = randInt(-100, 100) * 64;
                        keys.add(R.nextBoolean() ? (Object) h : (Object) (long) h);
                    }
                    break;
                default: for (int i = 0; i < n; i++) keys.add(randomKey(R.nextInt(11))); break;
            }
        }
        Collections.shuffle(keys, R);
        return dedupe(keys);
    }

    // ---------------- cases ----------------

    static final class Case {
        final String mode;
        final List<Object> pool;
        final Map<Object, Integer> index = new HashMap<>();
        final StringBuilder ops = new StringBuilder();
        final List<String> snaps = new ArrayList<>();
        Map<Object, Integer> map;
        Set<Object> set;
        int op;

        Case(String mode, int cap, List<Object> pool) {
            this.mode = mode;
            this.pool = pool;
            for (int i = 0; i < pool.size(); i++) index.put(pool.get(i), i);
            switch (mode) {
                case "M": map = cap < 0 ? new HashMap<>() : new HashMap<>(cap); break;
                case "LM": map = cap < 0 ? new LinkedHashMap<>() : new LinkedHashMap<>(cap); break;
                case "S": set = cap < 0 ? new HashSet<>() : new HashSet<>(cap); break;
                default: set = cap < 0 ? new LinkedHashSet<>() : new LinkedHashSet<>(cap); break;
            }
        }

        void emit(String s) { ops.append(' ').append(s); op++; }

        void put(int i) {
            emit("p" + i);
            if (map != null) map.put(pool.get(i), op); else set.add(pool.get(i));
        }

        void putIfAbsent(int i) {
            if (map == null) { put(i); return; }
            emit("a" + i);
            map.putIfAbsent(pool.get(i), op);
        }

        void remove(int i) {
            emit("r" + i);
            if (map != null) map.remove(pool.get(i)); else set.remove(pool.get(i));
        }

        void clear() {
            emit("c");
            if (map != null) map.clear(); else set.clear();
        }

        void retain(int m, int r) {
            emit("x" + m + "_" + r);
            Iterator<?> it = map != null ? map.entrySet().iterator() : set.iterator();
            for (int pos = 0; it.hasNext(); pos++) {
                it.next();
                if (pos % m == r) it.remove();
            }
        }

        void copy() {
            emit("y");
            switch (mode) {
                case "M": map = new HashMap<>(map); break;
                case "LM": map = new LinkedHashMap<>(map); break;
                case "S": set = new HashSet<>(set); break;
                default: set = new LinkedHashSet<>(set); break;
            }
        }

        void snap() throws Exception {
            emit("=");
            HashMap<?, ?> backing = map != null ? (HashMap<?, ?>) map : (HashMap<?, ?>) SETMAP.get(set);
            Object[] tab = (Object[]) TABLE.get(backing);
            StringBuilder sb = new StringBuilder("E ");
            sb.append(tab == null ? 0 : tab.length).append(' ');
            sb.append(map != null ? map.hashCode() : set.hashCode());
            Iterable<?> it = map != null ? map.keySet() : set;
            for (Object k : it) sb.append(' ').append(index.get(k));
            snaps.add(sb.toString());
        }

        void randomOp(double pPut) throws Exception {
            double x = R.nextDouble();
            int k = R.nextInt(pool.size());
            if (x < pPut) put(k);
            else if (x < pPut + 0.10) putIfAbsent(k);
            else if (x < pPut + 0.32) remove(k);
            else if (x < pPut + 0.34) clear();
            else if (x < pPut + 0.37) { int m = randInt(1, 5); retain(m, R.nextInt(m)); }
            else if (x < pPut + 0.40) copy();
            else put(k);
        }

        void write(PrintWriter w, String ctor) {
            w.println("C " + mode + " " + ctor);
            StringBuilder k = new StringBuilder("K");
            for (Object o : pool) k.append(' ').append(encode(o));
            w.println(k);
            w.println("O" + ops);
            for (String s : snaps) w.println(s);
        }
    }

    static String encode(Object o) {
        if (o == null) return "N";
        if (o instanceof Integer) return "I" + o;
        if (o instanceof Long) return "L" + o;
        if (o instanceof Double) return "D" + Long.toHexString(Double.doubleToRawLongBits((Double) o));
        String s = (String) o;
        StringBuilder sb = new StringBuilder("S");
        for (int i = 0; i < s.length(); i++) sb.append(String.format("%04x", (int) s.charAt(i)));
        return sb.toString();
    }

    static String pickMode(double hashBias) {
        double x = R.nextDouble();
        if (x < hashBias * 0.6) return "M";
        if (x < hashBias) return "S";
        return R.nextBoolean() ? "LM" : "LS";
    }

    static int pickCap() {
        double x = R.nextDouble();
        if (x < 0.3) return -1;
        if (x < 0.9) return R.nextInt(101);
        return R.nextInt(3000);
    }

    static String ctor(int cap) { return cap < 0 ? "D" : "N" + cap; }

    static void small(PrintWriter w) throws Exception {
        int cap = pickCap();
        Case c = new Case(pickMode(0.7), cap, smallPool());
        int n = randInt(1, 25);
        for (int i = 0; i < n; i++) {
            c.randomOp(0.45);
            if (R.nextInt(8) == 0) c.snap();
        }
        c.snap();
        c.write(w, ctor(cap));
    }

    static void tree(PrintWriter w) throws Exception {
        int cap = R.nextInt(3) == 0 ? -1 : new int[]{0, 1, 16, 32, 64, 100, 128, 256, 1000}[R.nextInt(9)];
        List<Object> pool = treePool();
        Case c = new Case(pickMode(0.85), cap, pool);
        List<Integer> order = new ArrayList<>();
        for (int i = 0; i < pool.size(); i++) order.add(i);
        Collections.shuffle(order, R);
        int fill = randInt(pool.size() / 2, pool.size());
        for (int i = 0; i < fill; i++) c.put(order.get(i));
        c.snap();
        // remove down (untreeify on movable remove)
        Collections.shuffle(order, R);
        int rem = randInt(0, pool.size());
        for (int i = 0; i < rem; i++) c.remove(order.get(i));
        c.snap();
        // mixed tail
        int n = randInt(0, 2 * pool.size());
        for (int i = 0; i < n; i++) {
            c.randomOp(0.5);
            if (R.nextInt(60) == 0) c.snap();
        }
        c.snap();
        c.write(w, ctor(cap));
    }
}
