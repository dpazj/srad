# Sparkplug TCK

A containerised setup for running the [Eclipse Sparkplug TCK 3.0.0][tck] against an
`srad` node, plus `tck-runner`, a CLI that runs the tests programmatically.

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

Build the runner and an example node first:

```shell
cargo build -p tck-runner --example simple-node
```

Then run a single test against a fresh, containerised TCK broker:

```shell
./target/debug/tck-runner edge:SendDataTest \
    --docker-image srad-tck \
    --node-cmd ./target/debug/examples/simple-node
```

Run several tests, or every test of a profile:

```shell
./target/debug/tck-runner edge:SessionEstablishmentTest edge:SendDataTest \
    --docker-image srad-tck \
    --node-cmd ./target/debug/examples/simple-node

./target/debug/tck-runner --suite broker --docker-image srad-tck
```

Tests are named `<profile>:<test>`, where the profile is `edge`, `host` or `broker`.
`--list` prints every test the TCK ships with.

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
- Host application tests expect the application to be running already; their
  interactive prompts need `--console-reply` or a person reading the console.

### Isolation and bdSeq

The TCK's monitor requires `bdSeq` to increment on every new MQTT `CONNECT`, including
after a process restart. A node that starts `bdSeq` at zero in every process therefore
fails that assertion as soon as it is restarted on the same broker. `--docker-isolation
per-test` (the default) avoids this by giving every test a fresh broker and TCK
extension. Use `shared` only when the node can persist `bdSeq`, or when the node is not
restarted between tests.
