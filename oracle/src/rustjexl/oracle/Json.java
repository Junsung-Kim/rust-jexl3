package rustjexl.oracle;

import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * Minimal JSON reader/writer for the differential protocol.
 * Objects decode to LinkedHashMap, arrays to ArrayList, strings to String (lone surrogates kept),
 * numbers to their source text wrapped in {@link Num}, true/false to Boolean, null to null.
 * The writer escapes every char outside printable ASCII as \\uXXXX, so output is canonical ASCII.
 */
public final class Json {
    private Json() {}

    /** A JSON number kept as source text. */
    public static final class Num {
        public final String text;
        Num(String t) { text = t; }
        @Override public String toString() { return text; }
    }

    public static Object parse(String s) {
        Json.Reader r = new Json.Reader(s);
        r.ws();
        Object o = r.value();
        r.ws();
        if (r.i != s.length()) throw new IllegalArgumentException("trailing json at " + r.i);
        return o;
    }

    private static final class Reader {
        final String s;
        int i;
        Reader(String s) { this.s = s; }
        void ws() { while (i < s.length() && " \t\r\n".indexOf(s.charAt(i)) >= 0) i++; }
        Object value() {
            char c = s.charAt(i);
            switch (c) {
                case '{': {
                    i++; Map<String, Object> m = new LinkedHashMap<>(); ws();
                    if (s.charAt(i) == '}') { i++; return m; }
                    while (true) {
                        ws(); String k = str(); ws(); expect(':'); ws();
                        m.put(k, value()); ws();
                        if (s.charAt(i) == ',') { i++; continue; }
                        expect('}'); return m;
                    }
                }
                case '[': {
                    i++; List<Object> l = new ArrayList<>(); ws();
                    if (s.charAt(i) == ']') { i++; return l; }
                    while (true) {
                        ws(); l.add(value()); ws();
                        if (s.charAt(i) == ',') { i++; continue; }
                        expect(']'); return l;
                    }
                }
                case '"': return str();
                case 't': i += 4; return Boolean.TRUE;
                case 'f': i += 5; return Boolean.FALSE;
                case 'n': i += 4; return null;
                default: {
                    int b = i;
                    while (i < s.length() && "+-0123456789.eE".indexOf(s.charAt(i)) >= 0) i++;
                    return new Num(s.substring(b, i));
                }
            }
        }
        void expect(char c) {
            if (s.charAt(i) != c) throw new IllegalArgumentException("expected " + c + " at " + i);
            i++;
        }
        String str() {
            expect('"');
            StringBuilder b = new StringBuilder();
            while (true) {
                char c = s.charAt(i++);
                if (c == '"') return b.toString();
                if (c == '\\') {
                    char e = s.charAt(i++);
                    switch (e) {
                        case 'n': b.append('\n'); break;
                        case 't': b.append('\t'); break;
                        case 'r': b.append('\r'); break;
                        case 'b': b.append('\b'); break;
                        case 'f': b.append('\f'); break;
                        case 'u': b.append((char) Integer.parseInt(s.substring(i, i + 4), 16)); i += 4; break;
                        default: b.append(e);
                    }
                } else {
                    b.append(c);
                }
            }
        }
    }

    public static void quote(StringBuilder b, String s) {
        b.append('"');
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            if (c == '"' || c == '\\') {
                b.append('\\').append(c);
            } else if (c >= 0x20 && c < 0x7f) {
                b.append(c);
            } else {
                b.append(String.format("\\u%04x", (int) c));
            }
        }
        b.append('"');
    }

    @SuppressWarnings("unchecked")
    public static void write(StringBuilder b, Object o) {
        if (o == null) {
            b.append("null");
        } else if (o instanceof String) {
            quote(b, (String) o);
        } else if (o instanceof Boolean || o instanceof Num || o instanceof Integer || o instanceof Long) {
            b.append(o.toString());
        } else if (o instanceof Map) {
            b.append('{');
            boolean first = true;
            for (Map.Entry<String, Object> e : ((Map<String, Object>) o).entrySet()) {
                if (!first) b.append(',');
                first = false;
                quote(b, e.getKey());
                b.append(':');
                write(b, e.getValue());
            }
            b.append('}');
        } else if (o instanceof List) {
            b.append('[');
            boolean first = true;
            for (Object e : (List<Object>) o) {
                if (!first) b.append(',');
                first = false;
                write(b, e);
            }
            b.append(']');
        } else {
            throw new IllegalArgumentException("not json: " + o.getClass());
        }
    }

    public static String write(Object o) {
        StringBuilder b = new StringBuilder();
        write(b, o);
        return b.toString();
    }
}
