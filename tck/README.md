# Sparkplug TCK

An **AI generated** containerised test harness for running the [Eclipse Sparkplug TCK 3.0.0][tck] against an
`srad` node, plus `tck-runner`, a CLI that runs the tests programmatically.

See [`CONFORMANCE.md`](./CONFORMANCE.md) for the current `edge` and `host`
results and the implementation gaps behind the tests that do not pass.

[tck]: https://github.com/eclipse-sparkplug/sparkplug

## Build

```shell
cd srad/tck
docker build -t srad-tck --file Dockerfile.tck . --progress=plain
```

The first build downloads the TCK release and builds its web console, so it takes a
few minutes and needs network access.

## Run the web console

```shell
docker run --rm -it --network=host srad-tck
```

The container starts HiveMQ on `localhost:1883` and the TCK web console on
<http://localhost:3000>.

In the web console:

1. Configure the MQTT broker as `localhost:1883`.
2. Configure the Sparkplug group ID (`foo`) and node ID (`bar`).
3. Run the TCK tests.

Note that the TCK expects a node that implements the features covered by each test.
Tests for commands, aliases or templates only pass if the node under test exposes the
corresponding metrics.

## Run tests programmatically

The TCK is controlled entirely over MQTT, so the web console is not needed.
`tck-runner` starts tests, optionally manages the broker container and the node under
test, collects the results from `SPARKPLUG_TCK/RESULT` and writes JUnit XML and JSON
reports.

Build the runner and the example nodes first:

```shell
cargo build -p tck-runner
cargo build -p examples --example simple-node --example tck-node --example tck-host --example generic-app
```

Then run a single test against a fresh, containerised TCK broker:

```shell
./target/debug/tck-runner edge:SendDataTest \
    --docker-image srad-tck \
    --node-cmd ./target/debug/examples/simple-node
```

`examples/node/tck.rs` (`--example tck-node`) is a node that implements the
features checked by `SendComplexDataTest`: it publishes a DataSet, a template
definition and instance, and data messages that use aliases. Run it with:

```shell
./target/debug/tck-runner edge:SendComplexDataTest \
    --docker-image srad-tck \
    --node-cmd ./target/debug/examples/tck-node
```

The `tck-node` example configures `SCADA_HOST` as its Sparkplug Primary Host
Application. The TCK brings this host online before starting the node, so `tck-node`
waits for the host's STATE message before publishing its birth and
`edge:SessionEstablishmentTest` exercises the primary-host wait path. If you change
`--host-app-id`, update `tck.rs` to match.

Run several tests, or every test of a profile:

```shell
./target/debug/tck-runner edge:SessionEstablishmentTest edge:SendDataTest \
    --docker-image srad-tck \
    --node-cmd ./target/debug/examples/tck-node

./target/debug/tck-runner --suite broker --docker-image srad-tck
```

Tests are named `<profile>:<test>`, where the profile is `edge`, `host` or `broker`.
`--list` prints every test the TCK ships with.

Keep the per-test timeout at the default of 120 seconds when you run a whole
profile. The interactive command tests need about 50 seconds, so shorter timeouts
report `timed_out` even though every assertion passed.

To use an already-running broker (for example the web console container), omit
`--docker-image`:

```shell
./target/debug/tck-runner edge:SendDataTest \
    --node-cmd ./target/debug/examples/simple-node \
    --broker 127.0.0.1:1883
```

### Results

Each run writes to a new directory under `tck/results/` (override with
`--results-dir`):

- `results.json`: machine readable results.
- `junit.xml`: JUnit report for CI systems.
- `<profile>-<test>/tck.log`: the raw TCK result and log messages.
- `<profile>-<test>/node.log`: output of the node under test.

The exit code is `0` when every test passed, `1` when at least one test failed or
timed out, and `2` for a runner error.

### Test parameters

| Option | Default | Meaning |
| --- | --- | --- |
| `--broker` | `127.0.0.1:1883` | Broker address used by the runner. |
| `--host-app-id` | `SCADA_HOST` | Host application ID used by the tests. |
| `--group-id` | `foo` | Edge node group ID. |
| `--node-id` | `bar` | Edge node ID. |
| `--device-id` | `dev1` | Device ID, repeatable. |
| `--client-id` | `srad-app` | MQTT client ID of the host application under test. |
| `--timeout` | `120` | Per-test timeout in seconds. |
| `--node-cmd` | | Command that starts the node under test, run through `sh -c`. |
| `--node-start-delay` | `1000` | Milliseconds to wait before/after starting the node. |
| `--node-stop-delay` | `5000` | Milliseconds before stopping the node in termination tests. |
| `--console-reply` | | Automatic answer to interactive TCK prompts, e.g. `PASS`. |
| `--docker-image` | | Start the TCK broker from this image. |
| `--docker-isolation` | `per-test` | `per-test` starts a fresh container per test, `shared` reuses one. |

### How tests are driven

The runner publishes `NEW_TEST <profile> <test> <arguments...>` to
`SPARKPLUG_TCK/TEST_CONTROL` and subscribes to `SPARKPLUG_TCK/RESULT`,
`SPARKPLUG_TCK/LOG` and `SPARKPLUG_TCK/CONSOLE_PROMPT`. The test catalogue records
which arguments each test takes and how the node has to be managed:

- Tests that observe the initial `CONNECT` and birth sequence get the node started
  after the test request.
- Session termination tests start the node and then stop it to trigger the death
  messages.
- Host application tests run the host application itself as the node under test.
  Session establishment and multiple-broker tests start it after the test
  request, session termination starts and then stops it, and the command and
  death tests start it before the test because the TCK checks the application's
  retained online `STATE` at construction. Their interactive prompts need
  `--console-reply` or a person reading the console.

### Host application tests

The host application under test must use the id passed to `--host-app-id` and
the MQTT client id passed to `--client-id`. `examples/app/tck_host.rs`
(`--example tck-host`) is a host application built on the generic application.
It defaults to `foo` and `client`, and answers the TCK's command prompts by
publishing the requested NCMD and DCMD messages through the generic
application's `AppClient`:

```shell
cargo build -p examples --example tck-host

./target/debug/tck-runner --suite host \
    --docker-image srad-tck \
    --node-cmd ./target/debug/examples/tck-host \
    --host-app-id foo \
    --client-id client \
    --console-reply PASS
```

`host:SendCommandTest` prompts for four commands: an edge rebirth, an edge
metric update, a device rebirth and a device metric update. `tck-host`
subscribes to `SPARKPLUG_TCK/CONSOLE_PROMPT` and publishes the corresponding
commands. It uses metric names from the TCK 3.0.0 edge node utility's NBIRTH
and DBIRTH (`TCK_metric/Int32`, `EdgeNode Control/Rebirth` and `Outputs/1`); if
the TCK changes those metrics, update the constants in the example.

The edge rebirth uses `AppClient::publish_node_rebirth`, which omits the metric
datatype as recommended by the specification. The TCK requires a Boolean
datatype for `Node Control/Rebirth`, so its
`operational-behavior-data-commands-ncmd-rebirth-value` assertion is not
executed and `host:SendCommandTest` reports `PASS but INCOMPLETE`. Adding the
datatype would make the assertion pass but would send a command payload that
the specification says *SHOULD NOT* include a datatype.

`tck-runner` exports the test parameters to every `--node-cmd` process as
`SRAD_HOST_APP_ID`, `SRAD_GROUP_ID`, `SRAD_NODE_ID`, `SRAD_DEVICE_ID`,
`SRAD_CLIENT_ID`, `SRAD_BROKER_HOST` and `SRAD_BROKER_PORT`. `tck-host` reads
these values, so it follows `--host-app-id`, `--group-id`, `--node-id`,
`--device-id`, `--client-id` and `--broker` instead of requiring the defaults.
The example uses the first `--device-id` when the option is repeated.

The simpler `generic-app` example can still be used for host tests that do not
send commands, but `host:SendCommandTest` times out with it because it has no
command workflow.

### Multiple brokers

Both `MultipleBrokerTest` tests expect a second MQTT server on port 1884. The
runner does not start it: start the broker you use for local testing on that
port and pass it with `--second-broker tcp://localhost:1884` (the default).

For example, with the workspace's inver broker, copy its development config,
change `[mqtt] bind` to `0.0.0.0:1884` and `[http] bind` to an unused port, and
start it before running the test. The TCK container uses host networking, so it
can reach the second broker on `localhost`.

`edge:MultipleBrokerTest` still fails because `srad-eon` has no broker failover;
see [`CONFORMANCE.md`](./CONFORMANCE.md). `host:MultipleBrokerTest` needs the host
application connected to both brokers, which `tck-host` does not do.

### Isolation and bdSeq

The TCK's monitor requires `bdSeq` to increment on every new MQTT `CONNECT`, including
after a process restart. A node that starts `bdSeq` at zero in every process therefore
fails that assertion as soon as it is restarted on the same broker. `--docker-isolation
per-test` (the default) avoids this by giving every test a fresh broker and TCK
extension. Use `shared` only when the node can persist `bdSeq`, or when the node is not
restarted between tests.
