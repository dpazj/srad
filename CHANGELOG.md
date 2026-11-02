# Changelog

## Unreleased

### Types

- Add `DataSet`, `DataSetColumn`, `TypedDataSet` and `DataSetRow` derive support,
  including `#[dataset(rename = "...")]`, `#[dataset(datatype = "...")]`,
  `#[dataset(skip)]` and `Option<T>` columns
- **breaking change** `MetricValueKind::DataSet` now contains a `DataSet`
- **breaking change** `HasDataType` now requires `DEFAULT_DATATYPE` and
  `SUPPORTED_DATATYPES` consts; the `default_datatype()` and `supported_datatypes()`
  functions were removed
- Add `DataTypeError`, `QualityError`, `PropertyValueError` and `PropertySetError`
- Add `TemplateError::MissingValue` and `TemplateError::ValueConversion`
- **breaking change** `PropertySet::insert`, `DataType::try_from`, `Quality::try_from` and the property set/template conversions now return typed errors instead of `()`

### Client

- Add `ClientError` and return it from all `Client` methods
- **breaking change** `Client` implementations must return `ClientError`

### App

- `AppClient` publish methods now return `ClientError`
- Add `BdseqError` and preserve BDSEQ decoding details in `PayloadError`
- **breaking change** `PayloadError::InvalidBdseq` now carries a `BdseqError`

### EoN

- Add `PublishError::Client` and stop masking client publish failures as `StateError::Offline`
- Add `MessageMetricError` and `MessageMetricsError`
- **breaking change** `MessageMetric` and `MessageMetrics` conversions now return typed errors

## 0.5.0

### EoN

- Add `AliasConfig` to enable custom aliases

## 0.4.1

- Update all dependencies

### EoN

- Expose `simple_manager` as a module, containing `SimpleMetricManager` implementation

## 0.4.0

### EoN

- Add ability to configure `SimpleManagerPublishMetric` metric fields

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
