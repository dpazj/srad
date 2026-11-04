# Sparkplug TCK Conformance

**Document AI Generated**

This document records the current results of the [Eclipse Sparkplug TCK][tck]
`edge` and `host` profiles against the `srad` Edge Node and Host Application
implementations. It lists what passes, which assertions are not executed, and the
implementation gaps behind them. The TCK's `broker` profile is not covered
because it tests the MQTT server rather than the library.

Last run: against Eclipse Sparkplug TCK **3.0.0** with the
`hivemq/hivemq-ce:2025.3` based `srad-tck` image.

[tck]: https://github.com/eclipse-sparkplug/sparkplug

## Scope

| Profile | Implementation under test | Example used |
| --- | --- | --- |
| `edge` | `srad-eon` | `examples/node/tck.rs` (`tck-node`) |
| `host` | `srad-app` | `examples/app/tck_host.rs` (`tck-host`) |

## Results

| Profile | Test | Result | Assertions |
| --- | --- | --- | --- |
| edge | `SessionEstablishmentTest` | PASS but INCOMPLETE | 77 passed, 1 not executed |
| edge | `SessionTerminationTest` | PASS but INCOMPLETE | 25 passed, 10 not executed |
| edge | `SendDataTest` | PASS | 44 passed |
| edge | `SendComplexDataTest` | PASS | 48 passed |
| edge | `ReceiveCommandTest` | PASS | 14 passed |
| edge | `PrimaryHostTest` | PASS | 35 passed |
| edge | `MultipleBrokerTest` | **FAIL** | 23 passed, 1 failed, 1 not executed |
| host | `SessionEstablishmentTest` | PASS but INCOMPLETE | 43 passed, 1 not executed |
| host | `SessionTerminationTest` | PASS but INCOMPLETE | 11 passed, 1 not executed |
| host | `SendCommandTest` | PASS but INCOMPLETE | 36 passed, 1 not executed |
| host | `EdgeSessionTerminationTest` | PASS | 15 passed |
| host | `MessageOrderingTest` | PASS | 22 passed |
| host | `MultipleBrokerTest` | PASS but INCOMPLETE | 7 passed, 1 not executed |

`PASS but INCOMPLETE` means every executed assertion passed but some assertions
were not exercised. It is the expected result when a requirement does not apply
to the implementation (for example, the MQTT 3.1.1 clean-session requirement
when the client only speaks MQTT 5).

## What is covered

### Edge Node (`srad-eon`)

The following are exercised and pass:

- Session establishment: `CONNECT` ordering, Last Will (topic, QoS, retain,
  payload, `bdSeq`), NBIRTH/DBIRTH topics, QoS, retain, sequence numbers,
  timestamps, ordering, and the `bdSeq` relationship between the Will and the
  birth.
- Primary host handling: waiting for an online `STATE`, ignoring a wrong host,
  re-birthing when the host comes online, and the offline/reconnect path.
- Data: NDATA/DDATA topics, QoS 0, no retain, sequence increments, UTC
  timestamps, metric ordering, and alias use.
- Complex data: DataSets, template definitions and instances, parameters,
  property sets, all scalar types, and aliases.
- Commands: NCMD/DCMD subscription, `Node Control/Rebirth` handling, and the
  rebirth sequence (`bdSeq` continuity, NBIRTH followed by DBIRTH).
- Topic structure and name validation, metric name/datatype rules, and
  birth/data alias rules.

### Host Application (`srad-app`)

- Session establishment: online `STATE` publish and subscription, retained
  birth/death payloads, QoS, and timestamps.
- Session termination: offline `STATE` before a clean MQTT DISCONNECT, death
  payload, QoS, and retain.
- Edge and device death handling: the application marks nodes, devices, and
  metrics stale when it receives NDEATH/DDEATH.
- Message ordering: out-of-order DDATA is buffered, and the application sends
  `Node Control/Rebirth` when the reorder timeout elapses.
- Commands: the `tck-host` example turns the TCK's console prompts into NCMD and
  DCMD publishes. `AppClient::publish_metrics` supports command topics, QoS 0,
  no retain, a payload timestamp, no sequence number and metrics with names and
  optional datatypes. The edge rebirth uses `publish_node_rebirth`, which omits
  the datatype as recommended by the specification. The other commands use
  `PublishMetric::with_datatype` where the TCK checks the datatype.

## Gaps and not-executed assertions

### `edge:MultipleBrokerTest` — no broker failover

**Failing assertion:** `operational-behavior-primary-application-state-with-multiple-servers-walk`

**Not executed:** `operational-behavior-primary-application-state-with-multiple-servers-single-server`

**Why:** `srad-eon` owns a single `EventLoop` and `Client`. When the primary host
goes offline, `NodeHandle::on_primary_host_offline` publishes NDEATH and
disconnects, but the node reconnects to the same broker. The specification
requires the Edge Node to terminate the session and move to the next MQTT Server
in its connection list. There is no broker list, no switching logic, and no
`EoNBuilder` API for multiple brokers.

The runner does not provision the second broker on port 1884 that this test
expects; start one separately and pass it with `--second-broker` (see
[`README.md`](./README.md#multiple-brokers)). The failing assertion is the node's
missing failover, not the harness.

**Implementation:** add a multi-broker client/event-loop adapter (or
`EoNBuilder::with_brokers`), switch brokers after the primary host goes offline,
and let the existing primary-host wait logic establish the new session. The
per-session `bdSeq` increment and the STATE timestamp handling already work.

### `host:MultipleBrokerTest` — single-broker application

**Not executed:**
`operational-behavior-host-application-multi-server-timestamp`.

**Why:** the test requires the Host Application to be online on two MQTT Servers
at the same time, publish its online `STATE` to both, and reconnect after the TCK
forcibly disconnects it from one. `srad-app` owns one client and one event loop,
and `tck-host` connects to a single broker. The single-server assertions
(`...-state-subs` and `...-state`) pass, so the test reports `PASS but INCOMPLETE`;
the multi-server timestamp assertion is not executed.

**Implementation:** run one application per broker (same host id, different MQTT
client ids) or add a multi-client application wrapper that publishes online and
offline `STATE` to every broker and reconnects each independently. The second
broker on port 1884 has to be started separately (see
[`README.md`](./README.md#multiple-brokers)).

### `host:SendCommandTest` — command datatype

**Not executed:** `operational-behavior-data-commands-ncmd-rebirth-value`.

**Why:** Sparkplug says the datatype *SHOULD NOT* be included in command
metrics. `tck-host` uses `AppClient::publish_node_rebirth` for the edge rebirth,
which omits it. The TCK only sets this assertion when the `Node Control/Rebirth`
metric has `datatype == Boolean`, so the assertion is not executed and the test
reports `PASS but INCOMPLETE`.

**Implementation (optional):** publish the edge rebirth through
`AppClient::publish_metrics` with
`PublishMetric::with_datatype(DataType::Boolean)`. The example intentionally
does not do this because it would send a command payload that the specification
says should not include a datatype.

### MQTT 3.1.1 assertions — client is MQTT 5 only

**Not executed:**
- `principles-persistence-clean-session-311` (`edge:SessionEstablishmentTest`)
- `message-flow-phid-sparkplug-clean-session-311`
  (`host:SessionEstablishmentTest`)

**Why:** `srad-client-rumqtt` imports `rumqttc::v5` only, so every session is
MQTT 5 and the 3.1.1 clean-session path cannot be exercised. The MQTT 5
equivalents (`clean-session-50`) pass.

**Implementation:** add a v4 client implementation using the root `rumqttc` API
and a way to select the MQTT version. This would also execute the MQTT 3.1.1
will/disconnect assertions.

### MQTT 5 Disconnect-with-Will — blocked by `rumqttc`

**Not executed:**
- `payloads-ndeath-will-message-publisher-disconnect-mqtt50`
- `payloads-ndeath-will-message-publisher-disconnect-mqtt311`

**Why:** for MQTT 5 the specification requires a DISCONNECT packet with reason
code `0x04` (Disconnect with Will Message). `srad-client-rumqtt` cannot send it:
`rumqttc` 0.24 exposes `Request::Disconnect` without a reason code, so
`disconnect_with_will` returns `true` and closes the TCP connection instead. The
broker still publishes the Will, but the TCK never sees a DISCONNECT packet and
cannot execute the assertion.

**Implementation:** patch or upgrade `rumqttc` to expose the reason code (the
existing TODO references rumqtt/rumqtt#916), then send `0x04` from
`disconnect_with_will`. Keep the explicit NDEATH publish, because brokers do not
expose server-published Wills to extensions; the duplicate Will NDEATH carries
the same `bdSeq` and is ignored by hosts.

### DDEATH on node shutdown — not published

**Not executed:** `operational-behavior-device-ddeath`,
`payloads-ddeath-seq`, `payloads-ddeath-seq-inc`,
`payloads-ddeath-seq-number`, `payloads-ddeath-timestamp`,
`topics-ddeath-mqtt`, `topics-ddeath-seq-num`, `topics-ddeath-topic`.

**Why:** the node publishes DDEATH when a device is disabled or removed
(`DeviceHandle::disable()` and `NodeStateMessage::Removed` call
`Device::death(true)`), but `NodeStateMessage::Death` calls `death(false)`.
`NodeHandle::cancel()` publishes only NDEATH, so a graceful node shutdown
produces no DDEATH. The TCK's `edge:SessionTerminationTest` stops the whole node
and therefore cannot observe a DDEATH.

The specification says an NDEATH implies that all associated devices are
offline, so DDEATH on node shutdown is not strictly required. This is primarily
TCK coverage.

**Implementation (optional):** publish DDEATH for each birthed device before
NDEATH in `NodeHandle::cancel()`. This needs a sequence allocation that does not
require `birthed` (the node is still online) and access to each device's birthed
state. Alternatively, add a `disable_all_devices()` helper with an
acknowledgement and call it from the test node before `cancel()`.

### `host:SessionTerminationTest` — no-DISCONNECT branch

**Not executed:**
`host-topic-phid-death-payload-timestamp-disconnect-with-no-disconnect-packet`

**Why:** the application publishes the offline `STATE` and then sends a clean MQTT
DISCONNECT, so the TCK takes the `disconnect-clean` branch. The other branch is
only executed when the application terminates the connection without a
DISCONNECT packet.

**Implementation:** run a variant that closes the socket without a DISCONNECT
(for example through `Client::disconnect_with_will` or a forced process kill) to
exercise the branch.

## TCK quirks worth knowing

- **Tahu integer encoding.** The TCK's Tahu encoder stores a `UInt32` value in
  the protobuf `long_value` field when the Java source value is a `Long`. This
  previously made `srad-types` reject the whole DBIRTH. `MetricValueKind` now
  accepts integer values in either `int_value` or `long_value`; the fix is
  covered by a unit test.
- **Command datatype.** `host:SendCommandTest` requires a datatype in command
  metrics even though the specification says it *SHOULD NOT* be included.
  `PublishMetric::with_datatype` supports this explicitly, but the generic
  `publish_node_rebirth` helper leaves the datatype out to stay spec-correct.
  `tck-host` therefore does not execute the TCK's `ncmd-rebirth-value`
  assertion; its other command publishes opt in where the TCK checks the
  datatype.
- **DDEATH is only reachable through node shutdown.** `edge:SessionTerminationTest`
  has no way to take a single device offline while the node stays up, so the
  DDEATH assertions depend on shutdown behaviour.

## Reproducing

Build the runner and examples:

```shell
cargo build -p tck-runner
cargo build -p examples --example tck-node --example tck-host
```

Edge profile:

```shell
./target/debug/tck-runner --suite edge \
    --docker-image srad-tck \
    --node-cmd ./target/debug/examples/tck-node \
    --timeout 120
```

Host profile:

```shell
./target/debug/tck-runner --suite host \
    --docker-image srad-tck \
    --node-cmd ./target/debug/examples/tck-host \
    --host-app-id foo \
    --client-id client \
    --console-reply PASS \
    --timeout 120
```

The runner writes `results.json`, `junit.xml`, and per-test logs under
`tck/results/<timestamp>/`. The host suite needs `--host-app-id` and
`--client-id` to match the application under test (`foo` and `client` for
`tck-host`). Both `MultipleBrokerTest` tests need a second broker on port 1884;
start it separately and pass `--second-broker` as described in
[`README.md`](./README.md#multiple-brokers). The host test also needs a host
application connected to both brokers. The simpler `generic-app` example can
still be used for host tests that do not send commands, but
`host:SendCommandTest` needs `tck-host`'s prompt handling.
