# Publishing

Everything here needs credentials, so it is a runbook rather than a script. The repository is
ready otherwise: `sh tools/verify.sh` is the gate, `cargo package` succeeds at 344 KiB, the
licence and NOTICE are in place, and no private data is in the history.

## 1. Decide the two names

`Cargo.toml` still carries a placeholder:

```toml
repository = "https://github.com/OWNER/rust-jexl3"
```

Replace `OWNER` with the GitHub account or organisation. The crate name (`rust-jexl3`) is already
chosen; `jexl`, `jexl-eval` and `jexl-parser` on crates.io are a *different* expression language
(TomFrost's JavaScript JEXL), which is why the name says which JEXL this is.

## 2. Create the repository and push

```sh
gh auth login                       # interactive; in Claude Code type it as `! gh auth login`
gh repo create <owner>/rust-jexl3 --public --source=. --remote=origin --push
```

## 3. Publish the crate

```sh
cargo login                         # token from https://crates.io/settings/tokens
sh tools/verify.sh --full           # last gate: suites, upstream corpus, a fresh 8k sample
cargo publish --dry-run
cargo publish
```

`cargo publish` is irreversible: a version can be yanked but never replaced, and a crate name is
never freed. Read what `cargo package --list` includes first.

## 4. Before the first release, decide what to say about

- **Version.** `0.1.0` is right while the differential suites still report the mismatches in
  [MISMATCHES.md](MISMATCHES.md). Say so in the release notes rather than in a footnote.
- **Performance.** Evaluation is 2-7x slower than the JVM (see PROGRESS.md). Better to lead with
  it than to be found out.
- **Scope.** [COMPATIBILITY.md](COMPATIBILITY.md) lists what a script cannot reach: no reflection
  over arbitrary Java classes, no `java.util.Date`, no JSR-223.
