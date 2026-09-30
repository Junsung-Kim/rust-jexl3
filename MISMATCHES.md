# Open mismatches

Every case below is a difference between this port and `commons-jexl3-3.2.1.jar` running on
Corretto 25, found by the differential suites. Each one is explained; none is unexplained.

Re-measure with the commands in [PROGRESS.md](PROGRESS.md).

The committed suites hold these as a baseline: `tests/data/exec/known_mismatches.txt` lists them,
and `cargo test` fails on any difference not listed there -- and on a listed one that has started
to match, so a fix must remove its line. `tests/data/exec/not_comparable.txt` lists the cases
whose Java answer is not reproducible at all (below).

## Execution suite — 2 of 5,959 (`tests/data/exec`)

| id | expression | Java | this port | why |
|---|---|---|---|---|
| `f31_844` | `c.name.new && t` | `undefined property 'new'`, caused by `java.beans.IntrospectionException: property get error: class java.lang.Character@new` | `undefined property 'name'` | Java's bean introspector reports the failure one segment further along, and attaches a `java.beans` cause. The port has no `java.beans`, so the first unresolvable segment is the one blamed. |
| `f31_2320` | `ns:joinWithPipe(z)` with `z` null | NPE `Cannot read the array length because "<parameter1>" is null` | ...because `"args"` is null | The JDK's helpful NullPointerException names a parameter from the *bytecode's* local-variable table when it has one and `<parameterN>` when it does not. The name depends on how the class was compiled, not on JEXL. |

## Script-API suite — 1 of 2,976 (`tests/data/exec/api_*`)

| id | what differs | why |
|---|---|---|
| `f61_1686` | `input.next()` — Java raises a boolean-coercion error, the port a `NoSuchElementException` | Evaluation order inside a map literal used as the left of `\|`. |

## A note on the oracle itself

One upstream expression, `latch.release(); while(true);`, shows the oracle is not stateless: a
fresh JVM times out on it (Java loops forever, and so does the port -- faithfully), while the same
JVM part-way through a long run answers `variable 'latch' is undefined`. JEXL keeps engine state
across parses on purpose -- the port reproduces the parser's state leak -- so an expectation
recorded mid-run is not always reproducible on its own. Corpora are therefore generated through
`tools/run_oracle.py`, which restarts the JVM around a wedged script and marks the case, and
`tests/exec_oracle.rs` now refuses to compare a case against an expectation carrying a different
id rather than drifting silently.

## Not comparable at all

Three further cases differ only in the iteration order of a `java.util.HashMap` or `HashSet` that
holds a value with no `hashCode` override — a bean, a `Pattern`, a lambda. Java orders those by
identity hash, which it picks per object per run: **two runs of the Java program disagree with each
other**, so there is no order for the port to match.

Where such a collection is the result, `tests/common/mod.rs::normalize` stops comparing its order.
Where it is rendered *inside an exception message* (`f31_2701`, `f31_2931`, `f31_3744`) the text
cannot be taken apart safely, and the cases are left visible rather than hidden.
