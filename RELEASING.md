# Releasing srad

## Update tasks

- Create a release branch
  - e.g `git checkout -b release/0.1.1`
- Update CHANGELOG.md
- Run `cargo update`
- Bump versions of all crates appropriately
  - e.g `cargo release version minor`
- Create PR with changes & merge onto `master`

## Release

- On `master` create a release tag
  - e.g `srad-v0.1.1`
- Create Tags and Publish to crates.io with:
  - `./publish_release.sh`
- Push tag 
