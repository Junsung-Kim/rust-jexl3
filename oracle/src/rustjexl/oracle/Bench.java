package rustjexl.oracle;

import java.util.LinkedHashMap;
import java.util.Map;

import org.apache.commons.jexl3.JexlBuilder;
import org.apache.commons.jexl3.JexlContext;
import org.apache.commons.jexl3.JexlEngine;
import org.apache.commons.jexl3.JexlScript;
import org.apache.commons.jexl3.MapContext;

/** Times parse and execute on the JVM, for comparison with the Rust port's examples/bench.rs. */
public final class Bench {
    private Bench() {}

    static final String[] SCRIPTS = {
        "a.b > 1 && name == '한글'",
        "x * 3 + y / 2 - 1",
        "a.b == null || (x > 10 ? 'big' : 'small') == 'small'",
        "var t = 0; for (i : 1..20) { t = t + i * 2; } t",
    };

    public static void main(final String[] args) {
        final int parseN = 20_000;
        final int execN = 200_000;
        final Map<String, Object> ns = new LinkedHashMap<>();
        ns.put("ns", new Hosts.Ns());
        final JexlEngine jexl = new JexlBuilder().namespaces(ns).strict(true).cache(512).create();
        final Map<String, Object> vars = new LinkedHashMap<>();
        vars.put("a.b", 2);
        vars.put("name", "한글");
        vars.put("x", 7L);
        vars.put("y", 3);
        final JexlContext ctx = new MapContext(vars);

        for (final String src : SCRIPTS) {
            // warm up both paths so the JIT has compiled them
            for (int i = 0; i < 20_000; i++) {
                jexl.createScript(src).execute(ctx);
            }
            long t0 = System.nanoTime();
            for (int i = 0; i < parseN; i++) {
                jexl.createScript(src);
            }
            final long parse = System.nanoTime() - t0;
            final JexlScript script = jexl.createScript(src);
            t0 = System.nanoTime();
            Object last = null;
            for (int i = 0; i < execN; i++) {
                last = script.execute(ctx);
            }
            final long exec = System.nanoTime() - t0;
            System.out.printf("%-58s parse %8.0f ns  exec %8.0f ns  (%s)%n",
                '"' + src.replace('\n', ' ') + '"', (double) parse / parseN, (double) exec / execN, last);
        }
    }
}
