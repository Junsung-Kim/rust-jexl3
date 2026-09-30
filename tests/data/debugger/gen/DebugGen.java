/*
 * Fixture generator for tests/debugger_oracle.rs.
 *
 * The oracle's `parsed` op only reaches JexlScript.getParsedText() (indentation 2). This program
 * reaches the rest of org.apache.commons.jexl3.internal.Debugger's surface -- getParsedText(n) for
 * several indentations, data(node) on every subtree, depth(1).data(node) (what
 * InterpreterBase.stringifyPropertyValue uses) and debug(node)'s start()/end() offsets (what
 * JexlException.detailedInfo uses) -- by living in the library's own package.
 *
 * Reads the cases.jsonl written by tools/fuzz_gen.py on stdin, writes api_expected.jsonl on stdout
 * (one record per source that parses with a default engine; api_cases.jsonl gets those sources).
 *
 * Build & run (from the repo root):
 *   M2=$HOME/.m2/repository
 *   JAR=$M2/org/apache/commons/commons-jexl3/3.2.1/commons-jexl3-3.2.1.jar
 *   LOG=$M2/commons-logging/commons-logging/1.2/commons-logging-1.2.jar
 *   javac -cp "$JAR:$LOG" -d /tmp/dbggen tests/data/debugger/gen/DebugGen.java \
 *         oracle/src/rustjexl/oracle/Json.java
 *   java -cp "/tmp/dbggen:$JAR:$LOG" org.apache.commons.jexl3.internal.DebugGen \
 *        tests/data/debugger/api_cases.jsonl < tests/data/debugger/cases.jsonl \
 *        > tests/data/debugger/api_expected.jsonl
 *
 * Add -DmaxCases=N (before the class name) for a larger local campaign set.
 */
package org.apache.commons.jexl3.internal;

import java.io.BufferedReader;
import java.io.InputStreamReader;
import java.io.PrintStream;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Map;
import java.util.regex.Pattern;

import org.apache.commons.jexl3.JexlBuilder;
import org.apache.commons.jexl3.JexlEngine;
import org.apache.commons.jexl3.JexlScript;
import org.apache.commons.jexl3.parser.JexlNode;

import rustjexl.oracle.Json;

public final class DebugGen {
    /** Maximum number of nodes walked per script (keeps the fixture a sane size). */
    private static final int MAX_NODES = 24;
    /** Maximum number of sources kept (-DmaxCases=N for a larger local campaign). */
    private static final int MAX_CASES = Integer.getInteger("maxCases", 1500).intValue();

    /** Hand-written sources for shapes tools/fuzz_gen.py does not reach. */
    private static final String[] EXTRA = {
        "x.`${y}`", "x?.`${y}`", "x.`a\\`b`", "x?.`${y}`.z",
        "x.'a b'", "x.size", "x.empty", "x.'it\\'s'", "x.'a.b'", "x.@a#b$c_d", "x.a1",
        "ns:fn(1, 2)", "ns:fn()", "a\\ b", "a\\'b", "a\\\"b", "a\\\\b",
        "[ 1, 2, ... ]", "[ ... ]", "[]", "[ 1 ]", "{:}", "{ }", "{ 1 : 2, 3 : 4 }", "{ 1, 2 }", "{ 1 }",
        "var f = function(a, b) { a + b }", "(a, b)->{ a + b }", "()->1", "(a)->a", "var g = function() { }",
        "@ann(1) x = 2", "@ann x = 2; @b @c { y }", "@ann { }",
        "for(var x : y);", "for(var x : y) { z }", "while(x);", "while(x) { y }",
        "do ; while(x)", "do { y } while(x)", "do y; while(x)",
        "a ?: b", "a ?? b", "a ? b : c", "if (a) b", "if (a) b; else c", "if (a) b; else if (c) d; else e",
        "(a + b) * c", "a * (b + c)", "(a - b) / c", "(a + b) % c", "a | b & c", "a ^ b & c", "a && (b || c)",
        "~(a + b)", "-(a + b)", "+(a + b)", "~a", "-a", "+a", "!a", "!(a + b)",
        "new('java.lang.Integer', 1)", "new('java.lang.Object')",
        "size x", "empty x", "size(x)", "(a)[0]", "(a)[0][1]", "a[0][1]",
        "~/a\\/b/", "~/a/", "'it\\'s'", "'a\nb'", "`a${b}c`", "`a\\`b`",
        "1 .. 2", "a .. b", "return 1", "return", "break", "continue",
        "1; 2; 3", "{ }", "{ { } }", "{ a; { b } }",
        "a.b.c.d", "a?.b?.c", "a.0.1", "x()", "x.y()", "x.y.z()", "a:b()",
        "0x1F", "1L", "1.5d", "1b", "1h", "1e3", "0.0", "-0.0",
        "var a = 1; var b = 2; a + b",
        "x = y = z", "x += 1", "x -= 1", "x *= 1", "x /= 1", "x %= 1", "x &= 1", "x |= 1", "x ^= 1",
        "a =~ b", "a !~ b", "a =^ b", "a =$ b", "a !^ b", "a !$ b",
        "a == b", "a != b", "a < b", "a > b", "a <= b", "a >= b", "a && b", "a || b",
        "if (a) { b } else { c }", "for(var i : 1..10) { if (i) { continue } else { break } }",
        "function(){}", "()->{ }", "(x)->{ x }",
        "null", "true", "false",
        "#pragma a.b 1\nx",
    };

    private static void walk(final JexlNode node, final List<JexlNode> out) {
        if (out.size() >= MAX_NODES) {
            return;
        }
        out.add(node);
        for (int i = 0; i < node.jjtGetNumChildren(); ++i) {
            walk(node.jjtGetChild(i), out);
        }
    }

    /** Pins Debugger.QUOTED_IDENTIFIER by measurement: the code units it matches. */
    private static Object quotedChars() {
        final Pattern p = Pattern.compile("[\\s]|[\\p{Punct}&&[^@#\\$_]]");
        final List<Object> hits = new ArrayList<>();
        for (int c = 0; c < 0x10000; ++c) {
            if (p.matcher(String.valueOf((char) c)).find()) {
                hits.add(Integer.valueOf(c));
            }
        }
        return hits;
    }

    public static void main(final String[] args) throws Exception {
        final PrintStream out = new PrintStream(System.out, true, "UTF-8");
        final PrintStream cases = new PrintStream(new java.io.FileOutputStream(args[0]), true, "UTF-8");
        final Map<String, Object> pin = new LinkedHashMap<>();
        pin.put("id", "__quoted");
        pin.put("chars", quotedChars());
        out.println(Json.write(pin));
        cases.println(Json.write(pin));

        final JexlEngine jexl = new JexlBuilder().create();
        final BufferedReader in = new BufferedReader(new InputStreamReader(System.in, StandardCharsets.UTF_8));
        final LinkedHashSet<String> seen = new LinkedHashSet<>();
        String line;
        int n = 0;
        final List<String> queue = new ArrayList<>();
        for (final String e : EXTRA) {
            queue.add(e);
        }
        while (n < MAX_CASES) {
            final String src;
            if (!queue.isEmpty()) {
                src = queue.remove(0);
            } else {
                line = in.readLine();
                if (line == null) {
                    break;
                }
                @SuppressWarnings("unchecked")
                final Map<String, Object> kase = (Map<String, Object>) Json.parse(line);
                src = (String) kase.get("src");
            }
            if (src == null || !seen.add(src)) {
                continue;
            }
            final JexlScript script;
            try {
                script = jexl.createScript(src);
            } catch (final Exception xany) {
                continue;
            }
            final JexlNode root = ((Script) script).getScript();
            final Map<String, Object> rec = new LinkedHashMap<>();
            final String id = "a" + n;
            rec.put("id", id);
            for (final int indent : new int[]{0, 1, 2, 4}) {
                rec.put("p" + indent, script.getParsedText(indent));
            }
            final List<JexlNode> nodes = new ArrayList<>();
            walk(root, nodes);
            final List<Object> dumps = new ArrayList<>();
            boolean teq = true;
            for (final JexlNode node : nodes) {
                final Map<String, Object> d = new LinkedHashMap<>();
                d.put("c", node.getClass().getSimpleName());
                final Debugger dbg = new Debugger();
                final boolean found = dbg.debug(node);
                d.put("found", Boolean.valueOf(found));
                // Debugger.debug(node) always renders from the root, so the text is the script's
                // getParsedText(2); only start/end depend on the node. `teq` records that as a
                // measured fact rather than storing the same text once per node.
                teq &= dbg.toString().equals(rec.get("p2"));
                d.put("s", Integer.valueOf(dbg.start()));
                d.put("e", Integer.valueOf(dbg.end()));
                d.put("data", new Debugger().data(node));
                d.put("d1", new Debugger().depth(1).data(node));
                d.put("d2", new Debugger().depth(2).data(node));
                dumps.add(d);
            }
            rec.put("teq", Boolean.valueOf(teq));
            rec.put("nodes", dumps);
            out.println(Json.write(rec));
            final Map<String, Object> kout = new LinkedHashMap<>();
            kout.put("id", id);
            kout.put("src", src);
            cases.println(Json.write(kout));
            n += 1;
        }
        out.flush();
        cases.flush();
    }
}
