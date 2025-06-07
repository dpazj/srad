# Release steps

## Update tasks

- Create a release branch
  - e.g `git checkout -b release/0.1.1`
- Update CHANGELOG.md
- Bump versions of all crates appropriately
  - e.g `cargo release version minor`
- Create PR with changes & merge onto `master`

## Release

- Create Tags and Publish to crates.io with:
  - `cargo release --workspace`
