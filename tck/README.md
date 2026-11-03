# Sparkplug TCK

A containerised setup for running the [Eclipse Sparkplug TCK 3.0.0][tck] against an
`srad` node. The image contains HiveMQ CE, the TCK HiveMQ extension and the TCK web
console.

[tck]: https://github.com/eclipse-sparkplug/sparkplug

## Build

```shell
cd srad/tck
docker build -t srad-tck --file Dockerfile.tck . --progress=plain
```

The first build downloads the TCK release and builds its web console, so it takes a
few minutes and needs network access.

## Run

```shell
docker run --rm -it --network=host srad-tck
```

The container starts HiveMQ on `localhost:1883` and the TCK web console on
<http://localhost:3000>.

## Test an example node

In another terminal, run an example node against the containerised broker:

```shell
cd srad
cargo run --example simple-node
```

The example uses group ID `foo` and node ID `bar`.

In the web console:

1. Configure the MQTT broker as `localhost:1883`.
2. Configure the Sparkplug group ID (`foo`) and node ID (`bar`).
3. Run the TCK tests.

Note that the TCK expects a node that implements the features covered by each test.
Tests for commands, aliases or templates only pass if the node under test exposes the
corresponding metrics.
