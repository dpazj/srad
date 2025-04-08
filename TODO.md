# TODO

## General

- more tests
- use tck to test
- better examples
- more detailed docs

## Srad

- feature flags

## EoN

- datatypes example
- custom device and node impl

- check result of subscribe and publishes and handle failures accordingly
  - failed subs and nbirth publishes should probably disconnect and return error from run()
  - how to handle failed dbirth/ddeaths?

- add node rebirth cooldown
- provide properties in MessageMetric
- multi part metric message support
  - ddata and cmd
- wait states
- handle client errors better
  - handle subscriptions failing
- multiple brokers support

## App

- check result of subscribe and publishes and handle failures accordingly
  - failed subs and online publishes should probably disconnect and return error from run()
- app produces duplicate online states if offline state has been retained on topic
  - resolved by client implementing success of publishes and subscription results.
- check sequence order and issue rebirths if invalid after timeout
- add ability to reorder out of sequence messages
- wait app states
- multi part metric message support
- multiple brokers support

## Client

## Rumqtt client

- better ensuring of operation success
  - make sure response to subscription request is made
  - ensure metric publish has been sent/acked
  - waiting on https://github.com/bytebeamio/rumqtt/pull/916 being merged

## Types

- template support
- dataset support
