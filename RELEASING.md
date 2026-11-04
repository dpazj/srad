# Releasing srad

## Pre-release checks

Run these on the commit that will be released (usually `master`) and attach the
TCK results to the release PR.

- CI is green on `master` (fmt, clippy, docs, codegen, test and MSRV jobs)
- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `RUSTDOCFLAGS=-Dwarnings cargo doc --lib --no-deps --all-features`
- `cargo +1.85.0 test --workspace`
- The Sparkplug TCK `edge` and `host` profiles pass as recorded in
  [`tck/CONFORMANCE.md`](./tck/CONFORMANCE.md). See [`tck/README.md`](./tck/README.md)
  for how to run them; use `--timeout 120` so slower tests are not cut off
- Update `tck/CONFORMANCE.md` from the new results, including changes to the
  assertions that are not executed

## Update tasks

- Create a release branch
  - e.g. `git checkout -b release/0.1.1`
- Update CHANGELOG.md
  - Fold `Unreleased` into the version section being released
- Run `cargo update`
- Bump versions of all crates appropriately
  - e.g. `cargo release version minor`
- Re-run the pre-release checks after `cargo update`, especially the MSRV test
- Create a PR with the changes and merge onto `master`

## Release

- On `master` create a release tag
  - Tag format is `srad-v<version>`, e.g. `srad-v0.1.1`
- Push the tag
  - e.g. `git push origin srad-v0.1.1`
- Publish to crates.io with:
  - `./publish_release.sh`
  - The script publishes in dependency order: `srad-macros`, `srad-types`,
    `srad-client`, `srad-client-rumqtt`, `srad-app`, `srad-eon`, `srad`
  - If a crate fails because the crates.io index has not caught up with a
    dependency yet, wait and publish the remaining crates by hand, starting with
    the one that failed. Never change the version of, or republish, an already
    accepted crate
- Verify the release
  - docs.rs builds every published crate
  - `cargo install srad` succeeds in a clean environment
  - the tag matches the published version
