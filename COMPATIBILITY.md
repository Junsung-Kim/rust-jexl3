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

(Anything else the harness reports as a mismatch is a bug, not an entry here.)
