# Changelog

## Unreleased

### Client

- **breaking change** Update `rumqttc` to 0.25.1. The `MqttOptions` and
  `ConnectProperties` re-exported by `srad-client-rumqtt` now come from
  rumqttc 0.25, whose TLS connections use rustls 0.23, with aws-lc-rs as the
  default crypto provider, instead of rustls 0.22. Certificates are then
  verified by rustls-webpki 0.103 rather than 0.102, which avoids
  RUSTSEC-2026-0346, RUSTSEC-2026-0349, RUSTSEC-2026-0049, RUSTSEC-2026-0098,
  RUSTSEC-2026-0099, RUSTSEC-2026-0104 and RUSTSEC-2026-0382

## 0.6.0

Minimum supported Rust version is now 1.85.

### Types

- Add `DataSet`, `DataSetColumn`, `TypedDataSet` and `DataSetRow` derive
  support, including `#[dataset(rename = "...")]`,
  `#[dataset(datatype = "...")]`, `#[dataset(skip)]` and `Option<T>` columns
- Add `DataSetError` for fallible DataSet operations
- **breaking change** `MetricValueKind::DataSet` now contains a `DataSet`
- **breaking change** `HasDataType` now requires `DEFAULT_DATATYPE` and
  `SUPPORTED_DATATYPES` consts; the `default_datatype()` and
  `supported_datatypes()` functions were removed
- Add `DataTypeError`, `QualityError`, `PropertyValueError` and
  `PropertySetError`
- Add `TemplateError::MissingValue` and `TemplateError::ValueConversion`
- **breaking change** `PropertySet::insert`, `DataType::try_from`,
  `Quality::try_from` and the property set/template conversions now return typed
  errors instead of `()`
- Sign-extend negative `Int8` and `Int16` values to 32 bits when encoding for
  compatibility with Tahu and other Sparkplug implementations; decoding accepts
  both sign-extended and width-sized forms
- Add value conversions from references (`&T`, `&str` and `&[T]`) for metric,
  property, data set and parameter values
- Fix `TemplateDefinition::try_from` to accept `is_definition = true`
- Accept integer metric values encoded in either the protobuf `int_value` or
  `long_value` field. Some producers, including the Eclipse Tahu based Sparkplug
  TCK, encode 32-bit datatypes in `long_value`, which previously made
  `MetricValueKind::try_from_metric_value` reject the whole birth payload

### Client

- Add support for deferred payload decoding: `MessagePayload` stores either
  encoded `Bytes` or a decoded `Payload`, `Message::decode` decodes on demand,
  and `topic_and_payload_to_event_deferred` defers decoding
- **breaking change** `Message.payload` is now a `MessagePayload`, and
  `Event::InvalidPublish` carries `Bytes`
- Add `ClientError` and return it from all `Client` methods
- **breaking change** `Client` implementations must return `ClientError`
- Fix the Node Death Last Will being retained: it is now registered with
  `retain = false`
- Back off between reconnect attempts after a manual disconnect instead of
  retrying in a tight loop
- Add `Client::disconnect_with_will`, allowing a client implementation to
  arrange for the broker to publish the Last Will on shutdown. The rumqtt
  implementation closes the connection without an MQTT `DISCONNECT` packet, so
  the Will is published as the node death certificate

### App

- `AppClient` publish methods now return `ClientError`
- Publish the application online STATE only after its subscriptions have been
  established
- Retry the application session instead of stopping when the subscription or
  online STATE publish fails. The delay between attempts is configurable with
  `ApplicationBuilder::with_retry_delay` and defaults to 3 seconds
- Add `AppEventLoop::set_retry_delay` to configure the delay between session
  retry attempts
- Add `NodeEvent::Raw` and `DeviceEvent::Raw` so the generic application can
  decode deferred payloads in per-node tasks
- Add `DecodedNodeEvent` and `DecodedDeviceEvent`, returned by
  `NodeEvent::from_message` and `DeviceEvent::from_message`, for decoding
  deferred events
- Add `BdseqError` and preserve BDSEQ decoding details in `PayloadError`
- **breaking change** `PayloadError::InvalidBdseq` now carries a `BdseqError`
- Establish the application session (subscribe and online STATE publish) on a
  spawned task so the event loop keeps polling. This fixes a deadlock when the
  client uses a zero-capacity request channel
- Always subscribe to the application's own `spBv1.0/STATE/<host>` topic, even
  when the namespace subscription already covers it
- Add `PublishMetric::with_datatype` so applications can include a datatype on
  published metrics. `publish_node_rebirth` continues to omit the datatype, as
  recommended by the specification

### EoN

- Change EoN lifecycle publishes to QoS 0: NDeath, DBirth and DDeath.
- Subscribe before publishing the NBIRTH and retry the node session instead of
  skipping the birth when the subscription or NBIRTH publish fails. The delay
  between attempts is configurable with `EoNBuilder::with_retry_delay` and
  defaults to 3 seconds
- Add `PublishError::Client` and stop masking client publish failures as
  `StateError::Offline`
- Add `MessageMetricError` and `MessageMetricsError`
- **breaking change** `MessageMetric` and `MessageMetrics` conversions now
  return typed errors
- Include registered template definitions in NBIRTH messages. The definitions
  were registered with the `TemplateRegistry` but never added to the birth
  payload, so template instances referenced a definition that nodes never
  published
- Add `EoNBuilder::with_primary_host_id` for Sparkplug primary host support.
  Nodes subscribe to `spBv1.0/STATE/<primary_host_id>`, defer NBIRTH and DBIRTH
  until the host publishes an online STATE, publish NDEATH and reconnect when
  the host goes offline, and ignore STATE messages with older timestamps
- Accept command payloads that omit the payload timestamp by using the time the
  command was received, instead of discarding the command
- Populate `MessageMetric::properties` from received command metrics instead of
  always setting them to `None`, and reject malformed property sets with
  `MessageMetricError::InvalidProperties`
- Stop data publishes immediately when a rebirth command is received, so no DATA
  messages are sent after the command, and publish the node and device births
  from the same session
- Stop accepting data publishes before publishing the NDEATH on shutdown, and
  use `Client::disconnect_with_will` when the client supports it. This stops
  DATA arriving after the death certificate and satisfies the MQTT 5 intentional
  disconnect requirement as far as rumqttc allows

### Examples

- Use a request channel capacity of 64 in the examples instead of a
  zero-capacity (rendezvous) channel
- Wait for the application event loop to finish in the `generic-app` example
  before exiting, so the offline STATE message and MQTT DISCONNECT are flushed
  and the TCK's `host:SessionTerminationTest` sees the host death certificate
- Publish the `tck-node` template instance in the device birth, rather than a
  parameters-only template, so the TCK's template member assertions execute
- Add a `tck-node` example that implements the DataSet and template features
  checked by the Sparkplug TCK's `SendComplexDataTest`
- Configure the `tck-node` example to wait for the TCK's `SCADA_HOST` primary
  host, which enables the TCK's `edge:SessionEstablishmentTest` primary host
  check
- Add a `tck-host` example that uses the generic application and answers the
  TCK's command prompts with NCMD and DCMD publishes, enabling
  `host:SendCommandTest`. It reads the runner's test parameters from the
  environment so it follows the configured host, group, node, device and broker.
  The edge rebirth follows the specification and omits the metric datatype, so
  the TCK's `ncmd-rebirth-value` assertion remains not executed

### TCK

- Manage the host application under test with `--node-cmd` for the host profile:
  session establishment and multiple-broker tests start it after the test
  request, session termination starts and then stops it, and the command and
  death tests start it before the test so its retained online STATE is present
- Wait `--node-start-delay` after starting a node for `StartBefore` tests, so
  the node has connected and published its retained birth or STATE before the
  test observes it
- Export the test parameters to `--node-cmd` as `SRAD_HOST_APP_ID`,
  `SRAD_GROUP_ID`, `SRAD_NODE_ID`, `SRAD_DEVICE_ID`, `SRAD_CLIENT_ID`,
  `SRAD_BROKER_HOST` and `SRAD_BROKER_PORT`, so examples can follow the runner's
  configuration instead of hardcoding it

### Known limitations

- **No multi-broker failover.** `srad-eon` owns a single client and event loop,
  so a node reconnects to the same MQTT server when the primary host goes
  offline instead of moving to the next server in a connection list. The TCK's
  `edge:MultipleBrokerTest` fails its failover assertions.
- **Single-broker host applications.** `srad-app` owns one client and event
  loop, and there is no built-in way to publish online and offline STATE to more
  than one MQTT server. The TCK's `host:MultipleBrokerTest` only executes its
  single-server assertions.
- **MQTT 5 only.** `srad-client-rumqtt` uses `rumqttc`'s MQTT 5 API, so MQTT
  3.1.1 sessions and their clean-session and will assertions are not exercised.
- **No MQTT 5 Disconnect with Will Message.** `Client::disconnect_with_will`
  closes the connection instead of sending reason code `0x04`, because `rumqttc`
  0.24 does not expose the DISCONNECT reason code. The broker still publishes
  the Last Will.
- **No multi-part metrics.** Multi-part NDATA/DDATA and NCMD/DCMD payloads are
  not supported.
- **No publish or subscription acknowledgements.** `rumqttc` 0.24 only enqueues
  requests and does not surface SUBACKs, so callers cannot confirm that a
  subscription or publish has been accepted.

## 0.5.0

### EoN

- Add `AliasConfig` to enable custom aliases

## 0.4.1

- Update all dependencies

### EoN

- Expose `simple_manager` as a module, containing `SimpleMetricManager`
  implementation

## 0.4.0

### EoN

- Add ability to configure `SimpleManagerPublishMetric` metric fields

## [0.3.0]

- Added Template support
  - Traits, Derive Macro and utility types

### EoN

- Add Support for use of Template metric values
- **breaking change** Add `SimpleMetricBuilder` type to configure simple metric
  manager nodes.
- Add ability to configure `SimpleMetricManager` metrics to use an alias

### App

- **breaking change** Move generic application configuration to new
  `generic::ApplicationBuilder` struct
- Add ability to configure node queue sizes
- Add ability to disable application message reordering.
- App will now discard messages produced before node went stale

## [0.2.2]

### App

- Fix reliability and performance issues in generic application when large
  number of metrics/second from the same node are received

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
