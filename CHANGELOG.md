# Changelog

## [0.3.0]

- Added Template support
  - Traits, Derive Macro and utility types

### EoN

- Add Support for use of Template metric values
- **breaking change** Add `SimpleMetricBuilder` type to configure simple metric manager nodes.
- Add ability to configure `SimpleMetricManager` metrics to use an alias

### App

- **breaking change** Move generic application configuration to new `generic::ApplicationBuilder` struct
- Add ability to configure node queue sizes
- Add ability to disable application message reordering.
- App will now discard messages produced before node went stale

## [0.2.2]

### App

- Fix reliability and performance issues in generic application when large number of metrics/second from the same node are received

## [0.2.1]

- Update project deps

## [0.2.0]

### Types

- Add `MetricValueKind` enum type

### App

- **breaking change** Remove `App`
- Add specific application eventloop implementation
- Add generic Application implementation
- Add message resequencer struct `Resequencer`

## [0.1.1]

### Types

- fix `BDSEQ` constant value from `bdseq` to `bdSeq`

## [0.1.0]

- Initial release
