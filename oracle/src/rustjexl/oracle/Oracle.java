package rustjexl.oracle;

import java.io.BufferedReader;
import java.io.InputStreamReader;
import java.io.PrintStream;
import java.io.StringWriter;
import java.lang.reflect.Array;
import java.math.BigDecimal;
import java.math.BigInteger;
import java.math.MathContext;
import java.math.RoundingMode;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Collection;
import java.util.HashMap;
import java.util.HashSet;
import java.util.Iterator;
import java.util.LinkedHashMap;
import java.util.LinkedHashSet;
import java.util.LinkedList;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.TreeMap;
import java.util.TreeSet;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.Future;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.TimeoutException;

import org.apache.commons.jexl3.JexlArithmetic;
import org.apache.commons.jexl3.JexlBuilder;
import org.apache.commons.jexl3.JexlEngine;
import org.apache.commons.jexl3.JexlException;
import org.apache.commons.jexl3.JexlExpression;
import org.apache.commons.jexl3.JexlFeatures;
import org.apache.commons.jexl3.JexlInfo;
import org.apache.commons.jexl3.JexlScript;
import org.apache.commons.jexl3.JxltEngine;
import org.apache.commons.jexl3.MapContext;

/**
 * The oracle runner: reads cases (JSONL) on stdin, runs them on the real commons-jexl3-3.2.1 jar,
 * writes results (JSONL) on stdout. See PROTOCOL.md for the format.
 */
public final class Oracle {
    private static final Map<String, JexlEngine> ENGINES = new HashMap<>();
    private static final ExecutorService EXEC = Executors.newSingleThreadExecutor(r -> {
        Thread t = new Thread(r, "case");
        t.setDaemon(true);
        return t;
    });

    public static void main(String[] args) throws Exception {
        // commons-logging warnings would pollute nothing (stderr), but keep them quiet
        System.setProperty("org.apache.commons.logging.Log", "org.apache.commons.logging.impl.NoOpLog");
        BufferedReader in = new BufferedReader(new InputStreamReader(System.in, StandardCharsets.UTF_8));
        PrintStream out = new PrintStream(System.out, false, "UTF-8");
        String line;
        while ((line = in.readLine()) != null) {
            if (line.trim().isEmpty()) continue;
            Map<String, Object> result;
            @SuppressWarnings("unchecked")
            Map<String, Object> kase = (Map<String, Object>) Json.parse(line);
            Future<Map<String, Object>> f = EXEC.submit(() -> run(kase));
            try {
                result = f.get(5, TimeUnit.SECONDS);
            } catch (TimeoutException xtime) {
                f.cancel(true);
                result = new LinkedHashMap<>();
                result.put("id", kase.get("id"));
                result.put("timeout", Boolean.TRUE);
                out.println(Json.write(result));
                out.flush();
                // the stuck thread may never die: replace the executor's worker by exiting cleanly
                System.exit(3);
                return;
            } catch (java.util.concurrent.ExecutionException xexec) {
                result = new LinkedHashMap<>();
                result.put("id", kase.get("id"));
                result.put("harness_error", String.valueOf(xexec.getCause()));
            }
            out.println(Json.write(result));
        }
        out.flush();
        System.exit(0);
    }

    // ---------------------------------------------------------------- engine

    @SuppressWarnings("unchecked")
    static JexlEngine engine(Map<String, Object> conf) {
        String key = Json.write(conf == null ? new LinkedHashMap<>() : conf);
        JexlEngine e = ENGINES.get(key);
        if (e != null) return e;
        JexlBuilder b = new JexlBuilder();
        if (conf != null) {
            Map<String, Object> ar = (Map<String, Object>) conf.get("arithmetic");
            if (ar != null) {
                boolean strict = ar.containsKey("strict") ? (Boolean) ar.get("strict") : true;
                MathContext mc = mathContext(ar.get("mathContext"));
                int scale = ar.containsKey("mathScale") ? Integer.parseInt(ar.get("mathScale").toString()) : Integer.MIN_VALUE;
                b.arithmetic(new JexlArithmetic(strict, mc, scale));
            }
            for (Map.Entry<String, Object> opt : conf.entrySet()) {
                Object v = opt.getValue();
                switch (opt.getKey()) {
                    case "strict": b.strict((Boolean) v); break;
                    case "silent": b.silent((Boolean) v); break;
                    case "safe": b.safe((Boolean) v); break;
                    case "lexical": b.lexical((Boolean) v); break;
                    case "lexicalShade": b.lexicalShade((Boolean) v); break;
                    case "antish": b.antish((Boolean) v); break;
                    case "cancellable": b.cancellable((Boolean) v); break;
                    case "debug": b.debug((Boolean) v); break;
                    case "collectMode": b.collectMode(Integer.parseInt(v.toString())); break;
                    case "cache": b.cache(Integer.parseInt(v.toString())); break;
                    case "cacheThreshold": b.cacheThreshold(Integer.parseInt(v.toString())); break;
                    case "stackOverflow": b.stackOverflow(Integer.parseInt(v.toString())); break;
                    case "namespaces": {
                        Map<String, Object> ns = new HashMap<>();
                        Hosts.namespaces((Map<String, Object>) v, ns);
                        b.namespaces(ns);
                        break;
                    }
                    case "features": b.features(features((Map<String, Object>) v)); break;
                    case "arithmetic": break;
                    default: throw new IllegalArgumentException("unknown engine option " + opt.getKey());
                }
            }
        }
        e = b.create();
        ENGINES.put(key, e);
        return e;
    }

    static MathContext mathContext(Object spec) {
        if (spec == null) return MathContext.DECIMAL128;
        switch (spec.toString()) {
            case "DECIMAL32": return MathContext.DECIMAL32;
            case "DECIMAL64": return MathContext.DECIMAL64;
            case "DECIMAL128": return MathContext.DECIMAL128;
            case "UNLIMITED": return MathContext.UNLIMITED;
            default: {
                // "precision:ROUNDING"
                String[] p = spec.toString().split(":");
                return new MathContext(Integer.parseInt(p[0]), RoundingMode.valueOf(p[1]));
            }
        }
    }

    @SuppressWarnings("unchecked")
    static JexlFeatures features(Map<String, Object> spec) {
        JexlFeatures f = new JexlFeatures();
        for (Map.Entry<String, Object> e : spec.entrySet()) {
            Object v = e.getValue();
            switch (e.getKey()) {
                case "register": f.register((Boolean) v); break;
                case "localVar": f.localVar((Boolean) v); break;
                case "sideEffect": f.sideEffect((Boolean) v); break;
                case "sideEffectGlobal": f.sideEffectGlobal((Boolean) v); break;
                case "arrayReferenceExpr": f.arrayReferenceExpr((Boolean) v); break;
                case "newInstance": f.newInstance((Boolean) v); break;
                case "loops": f.loops((Boolean) v); break;
                case "lambda": f.lambda((Boolean) v); break;
                case "methodCall": f.methodCall((Boolean) v); break;
                case "structuredLiteral": f.structuredLiteral((Boolean) v); break;
                case "pragma": f.pragma((Boolean) v); break;
                case "annotation": f.annotation((Boolean) v); break;
                case "script": f.script((Boolean) v); break;
                case "lexical": f.lexical((Boolean) v); break;
                case "lexicalShade": f.lexicalShade((Boolean) v); break;
                case "reservedNames": f.reservedNames((List<String>) (List<?>) v); break;
                case "namespaceTest": {
                    Set<String> names = new HashSet<>((List<String>) (List<?>) v);
                    f.namespaceTest(names::contains);
                    break;
                }
                default: throw new IllegalArgumentException("unknown feature " + e.getKey());
            }
        }
        return f;
    }

    // ---------------------------------------------------------------- case

    @SuppressWarnings("unchecked")
    static Map<String, Object> run(Map<String, Object> kase) {
        Map<String, Object> r = new LinkedHashMap<>();
        r.put("id", kase.get("id"));
        String kind = kase.containsKey("kind") ? (String) kase.get("kind") : "script";
        String src = (String) kase.get("src");
        List<String> params = (List<String>) (List<?>) kase.get("params");
        String[] pnames = params == null ? null : params.toArray(new String[0]);
        List<Object> ops = kase.containsKey("ops") ? (List<Object>) kase.get("ops") : java.util.Arrays.asList("vars", "exec");
        JexlEngine jexl;
        try {
            jexl = engine((Map<String, Object>) kase.get("engine"));
        } catch (RuntimeException xany) {
            r.put("engine_error", error(xany));
            return r;
        }
        JexlInfo info = new JexlInfo("case", 1, 1);
        MapContext ctx = new MapContext(context((Map<String, Object>) kase.get("ctx")));
        Object[] args = args((List<Object>) kase.get("args"));
        try {
            switch (kind) {
                case "script":
                case "expression": {
                    JexlScript script;
                    try {
                        if ("script".equals(kind)) {
                            script = jexl.createScript(info, src, pnames);
                        } else {
                            script = (JexlScript) jexl.createExpression(info, src);
                        }
                    } catch (RuntimeException xparse) {
                        r.put("parse", error(xparse));
                        return r;
                    }
                    for (Object op : ops) {
                        switch (op.toString()) {
                            case "vars": r.put("vars", vars(script.getVariables())); break;
                            case "params": r.put("params", strings(script.getParameters())); break;
                            case "locals": r.put("locals", strings(script.getLocalVariables())); break;
                            case "pragmas": r.put("pragmas", encode(script.getPragmas())); break;
                            case "parsed": r.put("parsed", script.getParsedText()); break;
                            case "ast": r.put("ast", ast(script)); break;
                            case "exec": {
                                try {
                                    Object v = "script".equals(kind)
                                        ? script.execute(ctx, args)
                                        : ((JexlExpression) script).evaluate(ctx);
                                    r.put("result", encode(v));
                                } catch (RuntimeException xexec) {
                                    r.put("error", error(xexec));
                                } catch (StackOverflowError xso) {
                                    r.put("error", error(xso));
                                }
                                break;
                            }
                            case "ctx": r.put("ctx", encodeContext(ctx)); break;
                            default: throw new IllegalArgumentException("unknown op " + op);
                        }
                    }
                    break;
                }
                case "tokens": {
                    r.put("tokens", tokens(src, Boolean.TRUE.equals(kase.get("registers"))));
                    break;
                }
                case "template": {
                    JxltEngine jxlt = jexl.createJxltEngine();
                    JxltEngine.Template t;
                    try {
                        t = jxlt.createTemplate(info, src, pnames);
                    } catch (RuntimeException xparse) {
                        r.put("parse", error(xparse));
                        return r;
                    }
                    for (Object op : ops) {
                        switch (op.toString()) {
                            case "vars": r.put("vars", vars(t.getVariables())); break;
                            case "params": r.put("params", strings(t.getParameters())); break;
                            case "pragmas": r.put("pragmas", encode(t.getPragmas())); break;
                            case "parsed": r.put("parsed", t.asString()); break;
                            case "exec": {
                                try {
                                    StringWriter w = new StringWriter();
                                    t.evaluate(ctx, w, args);
                                    r.put("output", w.toString());
                                } catch (RuntimeException xexec) {
                                    r.put("error", error(xexec));
                                }
                                break;
                            }
                            case "ctx": r.put("ctx", encodeContext(ctx)); break;
                            default: throw new IllegalArgumentException("unknown op " + op);
                        }
                    }
                    break;
                }
                case "jxlt": {
                    JxltEngine jxlt = jexl.createJxltEngine();
                    JxltEngine.Expression x;
                    try {
                        x = jxlt.createExpression(info, src);
                    } catch (RuntimeException xparse) {
                        r.put("parse", error(xparse));
                        return r;
                    }
                    for (Object op : ops) {
                        switch (op.toString()) {
                            case "vars": r.put("vars", vars(x.getVariables())); break;
                            case "parsed": r.put("parsed", x.asString()); break;
                            case "exec": {
                                try {
                                    r.put("result", encode(x.evaluate(ctx)));
                                } catch (RuntimeException xexec) {
                                    r.put("error", error(xexec));
                                }
                                break;
                            }
                            case "ctx": r.put("ctx", encodeContext(ctx)); break;
                            default: throw new IllegalArgumentException("unknown op " + op);
                        }
                    }
                    break;
                }
                default: throw new IllegalArgumentException("unknown kind " + kind);
            }
        } catch (RuntimeException xharness) {
            r.put("harness_error", String.valueOf(xharness));
        }
        return r;
    }

    /** Dumps the token stream of the real ParserTokenManager (lexer differential tests). */
    static List<Object> tokens(String src, boolean registers) {
        List<Object> out = new ArrayList<>();
        try {
            org.apache.commons.jexl3.parser.ParserTokenManager tm = new org.apache.commons.jexl3.parser.ParserTokenManager(
                new org.apache.commons.jexl3.parser.SimpleCharStream(new org.apache.commons.jexl3.parser.StringProvider(src)));
            if (registers) {
                java.lang.reflect.Field f = tm.getClass().getDeclaredField("defaultLexState");
                f.setAccessible(true);
                f.setInt(tm, 2);
                tm.SwitchTo(2);
            }
            while (true) {
                org.apache.commons.jexl3.parser.Token t = tm.getNextToken();
                List<Object> tk = new ArrayList<>();
                tk.add(t.kind); tk.add(t.image); tk.add(t.beginLine); tk.add(t.beginColumn); tk.add(t.endLine); tk.add(t.endColumn);
                out.add(tk);
                if (t.kind == 0) break;
            }
        } catch (org.apache.commons.jexl3.parser.TokenMgrException x) {
            Map<String, Object> e = new LinkedHashMap<>();
            e.put("msg", x.getMessage()); e.put("line", x.getLine()); e.put("column", x.getColumn()); e.put("after", x.getAfter());
            out.add(e);
        } catch (ReflectiveOperationException x) {
            throw new IllegalStateException(x);
        }
        return out;
    }

    // ---------------------------------------------------------------- AST dump (parser differential tests)

    static Object ast(JexlScript script) {
        try {
            java.lang.reflect.Field f = org.apache.commons.jexl3.internal.Script.class.getDeclaredField("script");
            f.setAccessible(true);
            return node((org.apache.commons.jexl3.parser.JexlNode) f.get(script));
        } catch (ReflectiveOperationException x) {
            throw new IllegalStateException(x);
        }
    }

    static Object node(org.apache.commons.jexl3.parser.JexlNode n) {
        List<Object> out = new ArrayList<>();
        out.add(n.getClass().getSimpleName());
        out.add(n.getLine());
        out.add(n.getColumn());
        Map<String, Object> a = new LinkedHashMap<>();
        if (n.isConstant()) a.put("const", Boolean.TRUE);
        if (n instanceof org.apache.commons.jexl3.parser.ASTIdentifier) {
            org.apache.commons.jexl3.parser.ASTIdentifier id = (org.apache.commons.jexl3.parser.ASTIdentifier) n;
            a.put("name", id.getName());
            a.put("symbol", id.getSymbol());
            if (id.getNamespace() != null) a.put("ns", id.getNamespace());
            if (id.isRedefined()) a.put("redefined", Boolean.TRUE);
            if (id.isShaded()) a.put("shaded", Boolean.TRUE);
            if (id.isCaptured()) a.put("captured", Boolean.TRUE);
        } else if (n instanceof org.apache.commons.jexl3.parser.ASTIdentifierAccess) {
            org.apache.commons.jexl3.parser.ASTIdentifierAccess id = (org.apache.commons.jexl3.parser.ASTIdentifierAccess) n;
            a.put("name", id.getName());
            a.put("id", encode(id.getIdentifier()));
            if (id.isSafe()) a.put("safe", Boolean.TRUE);
            if (id.isExpression()) a.put("expr", Boolean.TRUE);
        } else if (n instanceof org.apache.commons.jexl3.parser.ASTNumberLiteral) {
            org.apache.commons.jexl3.parser.ASTNumberLiteral nl = (org.apache.commons.jexl3.parser.ASTNumberLiteral) n;
            a.put("value", encode(nl.getLiteral()));
            a.put("class", nl.getLiteralClass().getSimpleName());
            a.put("image", nl.toString());
        } else if (n instanceof org.apache.commons.jexl3.parser.ASTStringLiteral) {
            a.put("value", ((org.apache.commons.jexl3.parser.ASTStringLiteral) n).getLiteral());
        } else if (n instanceof org.apache.commons.jexl3.parser.ASTJxltLiteral) {
            a.put("value", ((org.apache.commons.jexl3.parser.ASTJxltLiteral) n).getLiteral());
        } else if (n instanceof org.apache.commons.jexl3.parser.ASTRegexLiteral) {
            a.put("value", n.toString());
        } else if (n instanceof org.apache.commons.jexl3.parser.ASTAnnotation) {
            a.put("name", ((org.apache.commons.jexl3.parser.ASTAnnotation) n).getName());
        }
        if (n instanceof org.apache.commons.jexl3.parser.ASTJexlScript) {
            org.apache.commons.jexl3.parser.ASTJexlScript sc = (org.apache.commons.jexl3.parser.ASTJexlScript) n;
            a.put("args", sc.getArgCount());
            if (sc.getScope() != null) {
                a.put("symbols", strings(sc.getSymbols()));
                a.put("params", strings(sc.getParameters()));
                a.put("locals", strings(sc.getLocalVariables()));
                List<Object> caps = new ArrayList<>();
                String[] syms = sc.getSymbols();
                for (int i = 0; i < syms.length; ++i) if (sc.isCapturedSymbol(i)) caps.add(i);
                a.put("captured", caps);
            }
            if (sc.getPragmas() != null && !sc.getPragmas().isEmpty()) a.put("pragmas", encode(sc.getPragmas()));
        }
        if (n instanceof org.apache.commons.jexl3.parser.JexlLexicalNode) {
            int c = ((org.apache.commons.jexl3.parser.JexlLexicalNode) n).getSymbolCount();
            if (c > 0) {
                List<Object> ls = new ArrayList<>();
                org.apache.commons.jexl3.internal.LexicalScope scope = ((org.apache.commons.jexl3.parser.JexlLexicalNode) n).getLexicalScope();
                for (int i = 0; i < 128; ++i) if (scope.hasSymbol(i)) ls.add(i);
                a.put("lexical", ls);
            }
        }
        out.add(a);
        List<Object> kids = new ArrayList<>();
        for (int i = 0; i < n.jjtGetNumChildren(); ++i) {
            org.apache.commons.jexl3.parser.JexlNode c = n.jjtGetChild(i);
            if (c.jjtGetParent() != n) a.put("badparent", i);
            kids.add(node(c));
        }
        out.add(kids);
        return out;
    }

    static List<Object> strings(String[] s) {
        List<Object> l = new ArrayList<>();
        if (s != null) for (String e : s) l.add(e);
        return l;
    }

    static List<Object> vars(Set<List<String>> vs) {
        List<Object> l = new ArrayList<>();
        for (List<String> v : vs) l.add(new ArrayList<Object>(v));
        return l;
    }

    // ---------------------------------------------------------------- errors

    static Map<String, Object> error(Throwable t) {
        Map<String, Object> m = new LinkedHashMap<>();
        m.put("class", className(t.getClass()));
        m.put("msg", t.getMessage());
        Throwable cause = t.getCause();
        if (cause != null && cause != t) {
            Map<String, Object> c = new LinkedHashMap<>();
            c.put("class", className(cause.getClass()));
            c.put("msg", cause.getMessage());
            m.put("cause", c);
        }
        return m;
    }

    static String className(Class<?> c) {
        String n = c.getName();
        return n.startsWith("org.apache.commons.jexl3.") ? n.substring("org.apache.commons.jexl3.".length()) : n;
    }

    // ---------------------------------------------------------------- values in

    static Map<String, Object> context(Map<String, Object> spec) {
        Map<String, Object> m = new HashMap<>();
        if (spec != null) {
            for (Map.Entry<String, Object> e : spec.entrySet()) {
                m.put(e.getKey(), decode(e.getValue()));
            }
        }
        return m;
    }

    static Object[] args(List<Object> spec) {
        if (spec == null) return new Object[0];
        Object[] a = new Object[spec.size()];
        for (int i = 0; i < a.length; i++) a[i] = decode(spec.get(i));
        return a;
    }

    @SuppressWarnings("unchecked")
    static Object decode(Object o) {
        Map<String, Object> spec = (Map<String, Object>) o;
        String t = (String) spec.get("t");
        Object v = spec.get("v");
        String c = (String) spec.get("c");
        switch (t) {
            case "null": return null;
            case "Boolean": return Boolean.valueOf(v.toString());
            case "Byte": return Byte.valueOf(v.toString());
            case "Short": return Short.valueOf(v.toString());
            case "Integer": return Integer.valueOf(v.toString());
            case "Long": return Long.valueOf(v.toString());
            case "Float":
                return spec.containsKey("bits")
                    ? Float.intBitsToFloat(Integer.parseUnsignedInt(spec.get("bits").toString(), 16))
                    : Float.valueOf(v.toString());
            case "Double":
                return spec.containsKey("bits")
                    ? Double.longBitsToDouble(Long.parseUnsignedLong(spec.get("bits").toString(), 16))
                    : Double.valueOf(v.toString());
            case "BigInteger": return new BigInteger(v.toString());
            case "BigDecimal": return new BigDecimal(v.toString());
            case "Character": return ((String) v).charAt(0);
            case "String": return v;
            case "List": {
                List<Object> l = "java.util.LinkedList".equals(c) ? new LinkedList<>() : new ArrayList<>();
                for (Object e : (List<Object>) v) l.add(decode(e));
                return l;
            }
            case "Set": {
                Set<Object> s = "java.util.LinkedHashSet".equals(c) ? new LinkedHashSet<>()
                    : "java.util.TreeSet".equals(c) ? new TreeSet<>() : new HashSet<>();
                for (Object e : (List<Object>) v) s.add(decode(e));
                return s;
            }
            case "Map": {
                Map<Object, Object> m = "java.util.LinkedHashMap".equals(c) ? new LinkedHashMap<>()
                    : "java.util.TreeMap".equals(c) ? new TreeMap<>() : new HashMap<>();
                for (Object e : (List<Object>) v) {
                    List<Object> kv = (List<Object>) e;
                    m.put(decode(kv.get(0)), decode(kv.get(1)));
                }
                return m;
            }
            case "Array": {
                List<Object> l = (List<Object>) v;
                Class<?> ct = componentClass(c == null ? "Object" : c);
                Object a = Array.newInstance(ct, l.size());
                for (int i = 0; i < l.size(); i++) Array.set(a, i, decode(l.get(i)));
                return a;
            }
            case "Host": {
                List<Object> hargs = (List<Object>) spec.get("args");
                return Hosts.create((String) v, hargs == null ? null : java.util.Arrays.asList(args(hargs)));
            }
            default: throw new IllegalArgumentException("unknown type " + t);
        }
    }

    static Class<?> componentClass(String c) {
        switch (c) {
            case "int": return int.class;
            case "long": return long.class;
            case "short": return short.class;
            case "byte": return byte.class;
            case "char": return char.class;
            case "float": return float.class;
            case "double": return double.class;
            case "boolean": return boolean.class;
            case "String": return String.class;
            case "Integer": return Integer.class;
            case "Long": return Long.class;
            case "Double": return Double.class;
            case "Number": return Number.class;
            case "Object": return Object.class;
            default: throw new IllegalArgumentException("unknown component " + c);
        }
    }

    // ---------------------------------------------------------------- values out

    static Map<String, Object> encodeContext(MapContext ctx) {
        // MapContext hides its map; probe through a reflective read of the private field
        try {
            java.lang.reflect.Field f = MapContext.class.getDeclaredField("map");
            f.setAccessible(true);
            @SuppressWarnings("unchecked")
            Map<String, Object> m = (Map<String, Object>) f.get(ctx);
            Map<String, Object> out = new LinkedHashMap<>();
            for (String k : new TreeSet<>(m.keySet())) out.put(k, encode(m.get(k)));
            return out;
        } catch (ReflectiveOperationException x) {
            throw new IllegalStateException(x);
        }
    }

    static Object encode(Object o) {
        return encode(o, 0);
    }

    private static Map<String, Object> typed(String t, Object v) {
        Map<String, Object> m = new LinkedHashMap<>();
        m.put("t", t);
        if (v != null) m.put("v", v);
        return m;
    }

    @SuppressWarnings("unchecked")
    static Object encode(Object o, int depth) {
        if (depth > 16) return typed("Deep", null);
        if (o == null) return typed("null", null);
        Class<?> c = o.getClass();
        if (o instanceof Boolean) return typed("Boolean", o.toString());
        if (o instanceof Integer || o instanceof Long || o instanceof Short || o instanceof Byte
            || o instanceof BigInteger || o instanceof BigDecimal) {
            return typed(c.getSimpleName(), o.toString());
        }
        if (o instanceof Double) {
            Map<String, Object> m = typed("Double", o.toString());
            m.put("bits", Long.toHexString(Double.doubleToRawLongBits((Double) o)));
            return m;
        }
        if (o instanceof Float) {
            Map<String, Object> m = typed("Float", o.toString());
            m.put("bits", Integer.toHexString(Float.floatToRawIntBits((Float) o)));
            return m;
        }
        if (o instanceof Character) return typed("Character", o.toString());
        if (o instanceof String) return typed("String", o);
        String host = Hosts.hostName(o);
        if (host != null) {
            Map<String, Object> m = typed("Host", o.toString());
            m.put("c", host);
            return m;
        }
        if (c.isArray()) {
            List<Object> l = new ArrayList<>();
            int n = Array.getLength(o);
            for (int i = 0; i < n; i++) l.add(encode(Array.get(o, i), depth + 1));
            Map<String, Object> m = typed("Array", l);
            m.put("c", c.getComponentType().getSimpleName());
            return m;
        }
        if (o instanceof Map) {
            List<Object> l = new ArrayList<>();
            for (Map.Entry<Object, Object> e : ((Map<Object, Object>) o).entrySet()) {
                List<Object> kv = new ArrayList<>();
                kv.add(encode(e.getKey(), depth + 1));
                kv.add(encode(e.getValue(), depth + 1));
                l.add(kv);
            }
            Map<String, Object> m = typed("Map", l);
            m.put("c", c.getName());
            return m;
        }
        String cname = c.getName();
        if (cname.startsWith("org.apache.commons.jexl3.internal.IntegerRange")
            || cname.startsWith("org.apache.commons.jexl3.internal.LongRange")) {
            try {
                List<Object> l = new ArrayList<>();
                l.add(encode(c.getMethod("getMin").invoke(o), depth + 1));
                l.add(encode(c.getMethod("getMax").invoke(o), depth + 1));
                Map<String, Object> m = typed("Range", l);
                m.put("c", className(c));
                return m;
            } catch (ReflectiveOperationException x) {
                throw new IllegalStateException(x);
            }
        }
        if (o instanceof List || o instanceof Set) {
            List<Object> l = new ArrayList<>();
            for (Object e : (Collection<Object>) o) l.add(encode(e, depth + 1));
            Map<String, Object> m = typed(o instanceof List ? "List" : "Set", l);
            m.put("c", cname);
            return m;
        }
        if (o instanceof JexlScript) {
            Map<String, Object> m = typed("Script", ((JexlScript) o).getParsedText());
            m.put("c", className(c));
            return m;
        }
        if (o instanceof Class) return typed("Class", ((Class<?>) o).getName());
        Map<String, Object> m = typed("Object", null);
        m.put("c", className(c));
        String s = o.toString();
        if (s.equals(cname + "@" + Integer.toHexString(o.hashCode())) || o instanceof Iterator) {
            m.put("nd", Boolean.TRUE);
        } else {
            m.put("v", s);
        }
        return m;
    }
}
