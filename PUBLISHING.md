# Publishing

Releases go out from GitHub Actions through crates.io
[Trusted Publishing](https://crates.io/docs/trusted-publishing): no token is stored anywhere.

## Releasing a version

1. Move the `[Unreleased]` notes in [CHANGELOG.md](CHANGELOG.md) under `## [X.Y.Z] - date`, and
   add its compare link at the bottom.
2. Set `version = "X.Y.Z"` in `Cargo.toml`.
3. Run the last gate locally: `sh tools/verify.sh --full`.
4. Commit, then tag and push:

   ```sh
   git tag -a vX.Y.Z -m "rust-jexl3 X.Y.Z"
   git push origin main vX.Y.Z
   ```

[`release.yml`](.github/workflows/release.yml) then checks that the tag names the version in
`Cargo.toml`, runs the tests, publishes to crates.io and opens the GitHub release with the
CHANGELOG section as its notes.

A published version can be yanked but never replaced, and a crate name is never freed. Read
what `cargo package --list` includes before tagging.

## How the trust is set up

- crates.io, crate settings → Trusted Publishing: repository `Junsung-Kim/rust-jexl3`, workflow
  `release.yml`, environment `release`.
- GitHub, repository settings → Environments → `release`: deployments only from tags `v*`.

0.1.0 was published by hand with an API token, since Trusted Publishing needs the crate to exist
first.

## Names

Crate `rust-jexl3`, repository `https://github.com/Junsung-Kim/rust-jexl3`. `jexl`, `jexl-eval`
and `jexl-parser` on crates.io are a *different* expression language (TomFrost's JavaScript
JEXL), which is why the name says which JEXL this is.
