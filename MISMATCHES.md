# Open mismatches

Every case below is a difference between this port and `commons-jexl3-3.2.1.jar` running on
Corretto 25, found by the differential suites. Each one is explained; none is unexplained.

Re-measure with the commands in [PROGRESS.md](PROGRESS.md).

The committed suites hold understood differences as a baseline in
`tests/data/exec/known_mismatches.txt`; `cargo test` fails on any difference not listed there, and
on a listed one that has started to match, so a fix must remove its line. The list is empty now. `tests/data/exec/not_comparable.txt` lists the cases
whose Java answer is not reproducible at all (below).

## Execution suite — 0 of 5,959 (`tests/data/exec`)

None open. `known_mismatches.txt` is empty; the replay fails on any difference from the jar.

## Script-API suite — 0 of 2,976 (`tests/data/exec/api_*`)

None open.

## Known limits of the modelled JDK

Outside the committed suites, fresh random samples still find the edges of what the JDK shim
models. The ones known today:

- **Unicode tables.** `Character.getName(int)`, `getType(int)` and `getDirectionality(int)` need
  the JDK's Unicode data, which the port does not carry. The indexed property JEXL builds from them
  (`c.name`, `c.type`) is created exactly as on the JVM; asking it for a value with an int key fails
  here where Java answers.
- Anything COMPATIBILITY.md lists as out of scope (`java.util.Date`, reflection, ...).

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
