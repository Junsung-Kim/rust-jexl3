# Compatibility with Apache Commons JEXL 3.2.1

`rust-jexl` is measured against the official `commons-jexl3-3.2.1.jar` running on Corretto 25:
every behavior below is either proven identical by the differential harness (see `PROGRESS.md`)
or listed here as an intended divergence with a test that pins it.

## Out of scope (with the reason)

| Area | Reason |
|---|---|
| Arbitrary Java class reflection | There is no JVM. Property, method and constructor resolution goes through the `JexlUberspect` SPI; the JDK types a script can touch are modeled in `src/introspection/jdk_shim.rs`, and embedders register their own Rust types as host objects. |
| `new('some.java.Class')` outside the shim | Same reason: only the JDK classes the shim models can be constructed. The failure follows Java's "unsolvable function/method" path. |
| Identity-hash output (`Object.toString()` of an array, `IntegerRange.hashCode()`, …) | `System.identityHashCode` is nondeterministic in Java itself; the harness compares only the shape of such strings. |
| JSR-223 (`org.apache.commons.jexl3.scripting`) | A `javax.script` integration has no meaning outside the JVM. |
| `java.util.Date`, `Calendar`, `SimpleDateFormat`, `DecimalFormat`, `Locale` | Not modeled. A script that constructs one fails the way an unknown class does. Pinned by the upstream corpus (`new('java.util.Date')`). |
| `java.lang.Object`'s monitor methods (`wait`, `notify`, `notifyAll`) | There is no Java monitor to own. In Java these resolve on the context and throw `IllegalMonitorStateException`; here they are unsolvable methods. Pinned by the upstream corpus (`wait(10)`). |
| `java.beans` introspection | Java's bean introspector reports a failed accessor as `java.beans.IntrospectionException`, and finds a property one segment further along a chain than the port does. See MISMATCHES.md (`f31_844`). |
| `StringBuilder.compareTo` | `StringBuilder` is `Comparable<StringBuilder>`, so Java's `compareTo` raises a raw `ClassCastException` when compared with anything else; the shim reports an `Object comparison` arithmetic error instead. See MISMATCHES.md (`f61_525`). |
| `org.w3c.dom` / `javax.xml` | Not modeled (upstream `ArithmeticTest.testXmlArithmetic`). |
| Subclassing `JexlArithmetic` to *override* an operator | `JexlArithmetic` is a struct, not a class. Operator *overloads* work: register them through `JexlUberspect::get_operator`, which is the same path Java's `ArithmeticUberspect` uses. Overriding `divide`/`mod` outright (upstream `Arithmetic132`) has no equivalent. |
| `JexlBuilder.logger` | The engine writes no log, so the silent-mode warning counts upstream asserts cannot be observed. |

## Known divergences

(Each entry names the test that pins it.)

- **Tree-bin ordering of mutually incomparable keys with equal hashes.** When `java.util.HashMap`
  treeifies a bin whose keys have the same hash, are not mutually `Comparable` and share a class
  name, Java falls back to `System.identityHashCode`, which varies per run. The port picks a fixed
  order. Pinned by `tests/java_hash_map.rs::ceiling_identity_tie_break_is_pinned`; it matches a JVM
  started with `-XX:+UnlockExperimentalVMOptions -XX:hashCode=2`.
- **`ClassCastException` message for a host object.** The message the JVM builds for the implicit
  checkcast inside `Comparable.compareTo` names the defining module and loader. The port assumes
  both classes are in `java.base`, which is true for every modeled JDK value but not for a host
  object. See `src/jexl_arithmetic.rs` (`fn cce`).

- **Deeply nested unterminated literals parse in exponential time.** `"8%" + "{" * n` costs the
  *jar* 66 ms at n=8, 429 ms at n=10, 6.8 s at n=12 and 27 s at n=13; the port is within 2x of
  that. This is JEXL 3.2.1's own backtracking, reproduced rather than fixed — bound the size of
  untrusted input. Found by `cargo fuzz run parse`, which skips those shapes for that reason.

(Anything else the harness reports as a mismatch is a bug, not an entry here; the ones still open
are listed, with their reasons, in [MISMATCHES.md](MISMATCHES.md).)
