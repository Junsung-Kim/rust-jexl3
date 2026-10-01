# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project follows
[Semantic Versioning](https://semver.org/). Until 1.0, a minor version may change the Rust API;
behavior stays pinned to Apache Commons JEXL 3.2.1 throughout.

## [Unreleased]

## [0.1.1] - 2026-10-01

### Changed
- The README shipped to crates.io links the crate on crates.io and docs.rs, and no longer says
  the crate is unpublished.
- Releases are published from GitHub Actions through crates.io Trusted Publishing.

## [0.1.0] - 2026-09-30

First release: a port of Apache Commons JEXL 3.2.1.

### Added
- Lexer, parser and interpreter for the full JEXL 3.2.1 language: scripts, lambdas, `var`/`let`
  scoping, loops, namespaces, annotations, pragmas, operator overloading through `JexlArithmetic`.
- `JxltEngine` templates (`${...}`, `#{...}`, `$$` lines).
- The Java-side API: `JexlBuilder`, `JexlEngine`, `JexlScript`, `JexlContext`/`MapContext`,
  `JexlOptions`, `JexlFeatures`, `JexlException`, `JexlInfo`, `JexlSandbox` and the
  parse cache, with Java's names in snake_case and the main types re-exported at the crate root.
- A model of the JDK types scripts reach (strings, boxed numbers, `BigInteger`/`BigDecimal`,
  collections, maps, regex), so scripts behave as they do on a JVM.
- `HostIntrospector`, for exposing your own Rust types to scripts.
- `guard::nesting_depth`, a pre-parse check for untrusted input.

### Verified
- Differential suites against `commons-jexl3-3.2.1.jar`: execution 5,959 cases and script API
  2,976 cases with no unexplained difference, plus lexer, parser, arithmetic, templates and 633
  cases taken from the upstream test sources. See [MISMATCHES.md](https://github.com/Junsung-Kim/rust-jexl3/blob/main/MISMATCHES.md).

[Unreleased]: https://github.com/Junsung-Kim/rust-jexl3/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/Junsung-Kim/rust-jexl3/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/Junsung-Kim/rust-jexl3/releases/tag/v0.1.0
