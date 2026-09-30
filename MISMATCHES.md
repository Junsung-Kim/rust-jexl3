# Open mismatches

Every case below is a difference between this port and `commons-jexl3-3.2.1.jar` running on
Corretto 25, found by the differential suites. Each one is explained; none is unexplained.

Re-measure with the commands in [PROGRESS.md](PROGRESS.md).

## Execution suite — 5 of 5,962 (`tests/data/exec`)

| id | expression | Java | this port | why |
|---|---|---|---|---|
| `f31_844` | `c.name.new && t` | `undefined property 'new'`, caused by `java.beans.IntrospectionException: property get error: class java.lang.Character@new` | `undefined property 'name'` | Java's bean introspector reports the failure one segment further along, and attaches a `java.beans` cause. The port has no `java.beans`, so the first unresolvable segment is the one blamed. |
| `f31_1949` | `ns:concat(\`text\\\`\`, map) lt obj` | `variable 'obj' is undefined` | `unsolvable function/method 'concat(String, String)'` | Java's reflective varargs packing accepts `(String, Map)` for `concat(String...)`, so evaluation reaches `obj`. The shim's namespace method table does not model varargs widening for a Map argument. |
| `f31_2320` | `ns:joinWithPipe(z)` with `z` null | NPE `Cannot read the array length because "<parameter1>" is null` | ...because `"args"` is null | The JDK's helpful NullPointerException names a parameter from the *bytecode's* local-variable table when it has one and `<parameterN>` when it does not. The name depends on how the class was compiled, not on JEXL. |
| `f31_3162` | `obj[z][\`\`]` with `z` NaN | `undefined property ''` | `undefined property 'null'` | Java's second index evaluates to the empty string; the port produces null after the first index misses. Not yet traced. |
| `f31_3284` | `new('java.util.ArrayList', foo)` with a huge size | `java.lang.OutOfMemoryError: Requested array size exceeds VM limit` | evaluation continues, then `& error` | The port does not allocate a backing array eagerly, so the JVM's array-size limit never applies. Reproducing an OutOfMemoryError faithfully is not something the port should do. |

## Script-API suite — 3 of 2,977 (`tests/data/exec/api_*`)

| id | what differs | why |
|---|---|---|
| `f61_525` | `new('java.lang.StringBuilder') > ~/^[0-9]+$/` — Java raises a raw `ClassCastException` (`java.util.regex.Pattern cannot be cast to java.lang.StringBuilder`), the port an `ArithmeticException: Object comparison` | `JexlArithmetic.compare` casts to `Comparable` and calls `compareTo`; `StringBuilder` is `Comparable<StringBuilder>`, so the cast fails inside the JDK. The shim does not model `StringBuilder.compareTo`. |
| `f61_1481` | `unsolvable function/method '?'` vs `'null'` | The method name Java reports when the call site has none. |
| `f61_1686` | `input.next()` — Java raises a boolean-coercion error, the port a `NoSuchElementException` | Evaluation order inside a map literal used as the left of `\|`. |

## Not comparable at all

Three further cases differ only in the iteration order of a `java.util.HashMap` or `HashSet` that
holds a value with no `hashCode` override — a bean, a `Pattern`, a lambda. Java orders those by
identity hash, which it picks per object per run: **two runs of the Java program disagree with each
other**, so there is no order for the port to match.

Where such a collection is the result, `tests/common/mod.rs::normalize` stops comparing its order.
Where it is rendered *inside an exception message* (`f31_2701`, `f31_2931`, `f31_3744`) the text
cannot be taken apart safely, and the cases are left visible rather than hidden.
