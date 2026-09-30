package rustjexl.oracle;

import java.util.ArrayList;
import java.util.List;
import java.util.Map;

/**
 * Registry of the named test host objects. Every class here has a Rust twin in
 * harness/src/hosts.rs with identical observable behavior; keep them in sync.
 */
public final class Hosts {
    private Hosts() {}

    /** Stands for "JSON null": a non-null object, so {@code x == null} is false. */
    public static final class JsonNull {
        public static final JsonNull INSTANCE = new JsonNull();
        @Override public String toString() { return "null"; }
        @Override public boolean equals(Object o) { return o instanceof JsonNull; }
        @Override public int hashCode() { return 7; }
    }

    /** Namespace object shaped like the primary consumer's (neutral names, synthetic semantics). */
    public static final class Ns {
        public boolean isNull(Object o) { return o == null || o instanceof JsonNull; }
        public int castToInt(Object o) {
            if (o instanceof Number) return ((Number) o).intValue();
            return Integer.parseInt(String.valueOf(o).trim());
        }
        public String joinWithPipe(Object... args) {
            StringBuilder b = new StringBuilder();
            for (int i = 0; i < args.length; i++) {
                if (i > 0) b.append('|');
                b.append(args[i]);
            }
            return b.toString();
        }
        public boolean isIpv4(String s) { return getIpVersion(s) == 4; }
        public boolean isIpv6(String s) { return getIpVersion(s) == 6; }
        public int getIpVersion(String s) {
            if (s == null) return 0;
            if (s.matches("(\\d{1,3})\\.(\\d{1,3})\\.(\\d{1,3})\\.(\\d{1,3})")) return 4;
            if (s.indexOf(':') >= 0 && s.matches("[0-9a-fA-F:.]+")) return 6;
            return 0;
        }
        public long absAsInt64(Object o) {
            long l = o instanceof Number ? ((Number) o).longValue() : Long.parseLong(String.valueOf(o).trim());
            return Math.abs(l);
        }
        public double absAsDouble(Object o) {
            double d = o instanceof Number ? ((Number) o).doubleValue() : Double.parseDouble(String.valueOf(o).trim());
            return Math.abs(d);
        }
        public String concat(String head, Object... rest) {
            StringBuilder b = new StringBuilder(head);
            for (Object r : rest) b.append(r);
            return b.toString();
        }
        public String kind(int x) { return "int"; }
        public String kind(long x) { return "long"; }
        public String kind(double x) { return "double"; }
        public String kind(String x) { return "String"; }
        public String kind(Object x) { return "Object"; }
        public int sum(int a, int b) { return a + b; }
        public double sum(double a, double b) { return a + b; }
        public int size(Object... args) { return args.length; }
        public Object nvl(Object a, Object b) { return a == null || a instanceof JsonNull ? b : a; }
        @Override public String toString() { return "Ns"; }
    }

    /** A bean with properties, a duck-typed get and methods. */
    public static final class Bean {
        private String name;
        private int value;
        private boolean flag;
        private final List<Object> items = new ArrayList<>();
        public Bean() { this("bean", 0); }
        public Bean(String name, int value) { this.name = name; this.value = value; }
        public String getName() { return name; }
        public void setName(String n) { name = n; }
        public int getValue() { return value; }
        public void setValue(int v) { value = v; }
        public boolean isFlag() { return flag; }
        public void setFlag(boolean f) { flag = f; }
        public List<Object> getItems() { return items; }
        public String greet(String who) { return "hello " + who + " from " + name; }
        public int twice(int x) { return 2 * x; }
        @Override public String toString() { return "Bean(" + name + "," + value + ")"; }
    }

    public static Object create(String name, List<Object> args) {
        switch (name) {
            case "jsonNull": return JsonNull.INSTANCE;
            case "ns": return new Ns();
            case "bean":
                return args == null || args.isEmpty()
                    ? new Bean()
                    : new Bean((String) args.get(0), ((Number) args.get(1)).intValue());
            default: throw new IllegalArgumentException("unknown host " + name);
        }
    }

    /** Short name of a registered host class, or null. */
    public static String hostName(Object o) {
        if (o instanceof JsonNull) return "JsonNull";
        if (o instanceof Ns) return "Ns";
        if (o instanceof Bean) return "Bean";
        return null;
    }

    public static void namespaces(Map<String, Object> spec, Map<String, Object> into) {
        for (Map.Entry<String, Object> e : spec.entrySet()) {
            into.put(e.getKey(), create((String) e.getValue(), null));
        }
    }
}
