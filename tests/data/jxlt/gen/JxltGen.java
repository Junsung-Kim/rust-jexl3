package rustjexl.oracle;

import java.io.FileOutputStream;
import java.io.PrintStream;
import java.io.StringReader;
import java.io.StringWriter;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.TreeSet;

import org.apache.commons.jexl3.JexlBuilder;
import org.apache.commons.jexl3.JexlContext;
import org.apache.commons.jexl3.JexlEngine;
import org.apache.commons.jexl3.JexlInfo;
import org.apache.commons.jexl3.JxltEngine;
import org.apache.commons.jexl3.MapContext;
import org.apache.commons.jexl3.internal.TemplateDebugger;

/**
 * Writes the JXLT *API* fixtures: the parts of JxltEngine the JSONL oracle protocol cannot express,
 * because they need object identities rather than a one-shot evaluation -- `Expression.prepare()`,
 * `Expression.getSource()`, `toString()`, custom directive prefixes and expression characters,
 * `createJxltEngine(noScript, cacheSize, immediate, deferred)` and `TemplateDebugger`.
 *
 * Every field it writes is produced by `run_api_case` in tests/jxlt_oracle.rs, in the same order.
 *
 * Usage: tools/gen_jxlt_api.sh
 */
public final class JxltGen {
    private JxltGen() {}

    /** The engine spec both sides build from; kept in the case so the Rust side matches. */
    private static Map<String, Object> engineSpec() {
        final Map<String, Object> ns = new LinkedHashMap<>();
        ns.put("ns", "ns");
        final Map<String, Object> engine = new LinkedHashMap<>();
        engine.put("namespaces", ns);
        return engine;
    }

    /** The unified expressions the API surface is probed with. */
    private static final String[] EXPRESSIONS = {
        "${a}", "#{a}", "${a}#{b}", "constant", "", " ", "${a + b}", "#{a + b}",
        "${a ?: b}", "${a ? b : c}", "${'한글'}", "${\"하늘-\"}", "${a.b}", "${a[0]}",
        "${ns:isNull(a)}", "\\${a}", "\\#{a}", "${a}\\${b}", "#{#{a}}", "${#{a}}",
        "#{a + ${b}}", "text ${a} more #{b} tail", "${", "#{", "${a", "#{a",
        "${}", "#{}", "${a}${b}${c}", "#{a}#{b}", "${a}#{b}${c}#{d}",
        "${1 + 2}", "${1.5b}", "${[1, 2]}", "${{'k': 'v'}}", "${(x)->{ x + 1 }}",
        "${a = 1}", "${var x = 1; x}", "${a.b.c}", "${empty a}", "${size a}",
        "${a =~ '.*'}", "${null}", "${true}", "${'a' + 1}", "${a}text`",
        "#{a}#{b}#{c}", "${a}#{a}", "$${a}", "##{a}", "${'$'}${a}",
        "${var x}", "${var x; x}", "${for (i : [1, 2]) { i }}", "${x = 1}",
        "${var x = 1; var y = x + 1; y}",
    };

    private static final String[] TEMPLATES = {
        "${a}\n",
        "$$ var x = 1;\n${x}\n",
        "$$ if (a) {\n  yes ${a}\n$$ }\n",
        "plain text\n",
        "$$ for (i : [1, 2, 3]) {\n${i}\n$$ }\n",
        "${a} and #{b}\n",
        "$$ // comment\n${a}\n",
        "line1\nline2\n",
        "",
        "$$ var y = a; \n#{y}\n",
        "${'한글'} ${a}\n",
        "$$ while (false) {\n x\n$$ }\n",
        "$$ var z = 0;\n$$ z = z + 1;\n${z}\n",
        "#{a}\n",
        "no newline at end ${a}",
    };

    /** (immediate, deferred, prefix) triples; the last two are the non-default syntaxes. */
    private static final String[][] SYNTAX = {
        {"$", "#", "$$"},
        {"#", "$", "##"},
        {"%", "@", "%%"},
    };

    private static final Object[][] BINDINGS = {
        {"a", 1}, {"a", "text"}, {"a", null}, {"a", Boolean.TRUE}, {"a", Hosts.JsonNull.INSTANCE},
        {"b", 2}, {"b", "x"}, {"c", 3},
    };

    public static void main(final String[] args) throws Exception {
        final List<Map<String, Object>> cases = buildCases();
        try (PrintStream cout = new PrintStream(new FileOutputStream(args[0]), false, "UTF-8");
             PrintStream eout = new PrintStream(new FileOutputStream(args[1]), false, "UTF-8")) {
            for (final Map<String, Object> kase : cases) {
                cout.println(Json.write(kase));
                eout.println(Json.write(run(kase)));
            }
        }
        System.err.println("cases=" + cases.size());
    }

    private static Map<String, Object> kase(final String id, final String kind, final String src,
                                            final String[] syntax, final boolean noscript, final int cacheSize,
                                            final Map<String, Object> ctx, final List<Object> params) {
        final Map<String, Object> m = new LinkedHashMap<>();
        m.put("id", id);
        m.put("kind", kind);
        m.put("src", src);
        m.put("immediate", syntax[0]);
        m.put("deferred", syntax[1]);
        m.put("prefix", syntax[2]);
        m.put("noscript", noscript);
        m.put("cacheSize", Integer.toString(cacheSize));
        m.put("engine", engineSpec());
        m.put("ctx", ctx);
        if (params != null) {
            m.put("params", params);
        }
        return m;
    }

    private static Map<String, Object> bind(final Object[] binding) {
        final Map<String, Object> ctx = new LinkedHashMap<>();
        ctx.put((String) binding[0], encodeBinding(binding[1]));
        return ctx;
    }

    private static List<Map<String, Object>> buildCases() {
        final List<Map<String, Object>> out = new ArrayList<>();
        int n = 0;
        for (final String src : EXPRESSIONS) {
            for (final String[] syntax : SYNTAX) {
                for (final Object[] binding : BINDINGS) {
                    out.add(kase("ja" + n++, "jxlt", src, syntax, true, 256, bind(binding), null));
                }
                // the script-enabled engine, with the cache off
                out.add(kase("ja" + n++, "jxlt", src, syntax, false, 0, new LinkedHashMap<>(), null));
            }
        }
        for (final String src : TEMPLATES) {
            for (final String[] syntax : SYNTAX) {
                for (final Object[] binding : BINDINGS) {
                    out.add(kase("ja" + n++, "template", src, syntax, true, 256, bind(binding), null));
                }
                out.add(kase("ja" + n++, "template", src, syntax, false, 0, new LinkedHashMap<>(),
                        Arrays.asList((Object) "p", "q")));
            }
        }
        return out;
    }

    /** The typed-value encoding of tests/common/encode.rs, for the literal bindings above. */
    private static Map<String, Object> encodeBinding(final Object v) {
        final Map<String, Object> m = new LinkedHashMap<>();
        if (v == null) {
            m.put("t", "null");
        } else if (v instanceof Integer) {
            m.put("t", "Integer");
            m.put("v", v.toString());
        } else if (v instanceof Boolean) {
            m.put("t", "Boolean");
            m.put("v", v.toString());
        } else if (v instanceof Hosts.JsonNull) {
            m.put("t", "Host");
            m.put("v", "jsonNull");
        } else {
            m.put("t", "String");
            m.put("v", v.toString());
        }
        return m;
    }

    @SuppressWarnings("unchecked")
    private static Object decodeBinding(final Object spec) {
        final Map<String, Object> m = (Map<String, Object>) spec;
        final String t = (String) m.get("t");
        final Object v = m.get("v");
        switch (t) {
            case "null": return null;
            case "Integer": return Integer.valueOf(v.toString());
            case "Boolean": return Boolean.valueOf(v.toString());
            case "Host": return Hosts.create(v.toString(), null);
            default: return v;
        }
    }

    @SuppressWarnings("unchecked")
    private static Map<String, Object> run(final Map<String, Object> kase) {
        final Map<String, Object> r = new LinkedHashMap<>();
        final String src = (String) kase.get("src");
        final String kind = (String) kase.get("kind");
        final String prefix = (String) kase.get("prefix");
        final char immediate = ((String) kase.get("immediate")).charAt(0);
        final char deferred = ((String) kase.get("deferred")).charAt(0);
        final boolean noscript = (Boolean) kase.get("noscript");
        final int cacheSize = Integer.parseInt(kase.get("cacheSize").toString());
        final List<Object> params = (List<Object>) kase.get("params");
        final String[] pnames = params == null ? null : params.toArray(new String[0]);

        final Map<String, Object> namespaces = new LinkedHashMap<>();
        namespaces.put("ns", Hosts.create("ns", null));
        final JexlEngine jexl = new JexlBuilder().namespaces(namespaces).create();
        final Map<String, Object> vars = new LinkedHashMap<>();
        for (final Map.Entry<String, Object> e : ((Map<String, Object>) kase.get("ctx")).entrySet()) {
            vars.put(e.getKey(), decodeBinding(e.getValue()));
        }
        final JexlContext context = new MapContext(vars);
        final JexlInfo info = new JexlInfo("case", 1, 1);
        final JxltEngine jxlt = jexl.createJxltEngine(noscript, cacheSize, immediate, deferred);
        try {
            if ("template".equals(kind)) {
                final JxltEngine.Template template;
                try {
                    template = jxlt.createTemplate(info, prefix, new StringReader(src), pnames);
                } catch (final RuntimeException xparse) {
                    r.put("parse", error(xparse));
                    return r;
                }
                r.put("parsed", template.asString());
                r.put("toString", template.toString());
                final TemplateDebugger dbg = new TemplateDebugger();
                r.put("debug", dbg.debug(template));
                r.put("debugged", dbg.toString());
                try {
                    final JxltEngine.Template prepared = template.prepare(context);
                    if (prepared == null) {
                        r.put("prepared", null);
                    } else {
                        r.put("prepared", prepared.asString());
                        final StringWriter w = new StringWriter();
                        try {
                            prepared.evaluate(context, w);
                            r.put("output", w.toString());
                        } catch (final RuntimeException xeval) {
                            r.put("error", error(xeval));
                        }
                    }
                } catch (final RuntimeException xprep) {
                    r.put("prepare_error", error(xprep));
                }
            } else {
                final JxltEngine.Expression expr;
                try {
                    expr = jxlt.createExpression(info, src);
                } catch (final RuntimeException xparse) {
                    r.put("parse", error(xparse));
                    return r;
                }
                if (expr == null) {
                    r.put("harness_error", "silent null");
                    return r;
                }
                r.put("parsed", expr.asString());
                r.put("toString", expr.toString());
                r.put("immediate", expr.isImmediate());
                r.put("deferred", expr.isDeferred());
                final TemplateDebugger dbg = new TemplateDebugger();
                r.put("debug", dbg.debug(expr));
                r.put("debugged", dbg.toString());
                try {
                    final JxltEngine.Expression prepared = expr.prepare(context);
                    if (prepared == null) {
                        r.put("prepared", null);
                    } else {
                        r.put("prepared", prepared.asString());
                        r.put("prepared_toString", prepared.toString());
                        r.put("source", prepared.getSource().asString());
                        try {
                            r.put("result", Oracle.encode(prepared.evaluate(context)));
                        } catch (final RuntimeException xeval) {
                            r.put("error", error(xeval));
                        }
                    }
                } catch (final RuntimeException xprep) {
                    r.put("prepare_error", error(xprep));
                }
            }
        } catch (final StackOverflowError | RuntimeException xany) {
            r.put("harness_error", String.valueOf(xany));
            return r;
        }
        final Map<String, Object> ctx = new LinkedHashMap<>();
        for (final String key : new TreeSet<>(vars.keySet())) {
            ctx.put(key, Oracle.encode(vars.get(key)));
        }
        r.put("ctx", ctx);
        return r;
    }

    /** The exception shape Oracle.error() produces: class, message and one level of cause. */
    private static Map<String, Object> error(final Throwable t) {
        final Map<String, Object> m = new LinkedHashMap<>();
        m.put("class", className(t));
        m.put("msg", t.getMessage());
        final Throwable cause = t.getCause();
        if (cause != null && cause != t) {
            final Map<String, Object> c = new LinkedHashMap<>();
            c.put("class", className(cause));
            c.put("msg", cause.getMessage());
            m.put("cause", c);
        }
        return m;
    }

    private static String className(final Throwable t) {
        final String name = t.getClass().getName();
        return name.startsWith("org.apache.commons.jexl3.")
            ? name.substring("org.apache.commons.jexl3.".length())
            : name;
    }
}
