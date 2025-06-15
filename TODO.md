# TODO

## General

**Features**

- Full datatype support
  - Templates
  - Datasets
- Multi part metric support
- Wait for states
- Multi broker support


**Others**

- more tests
- use tck to test
- better examples
- more detailed docs

- add test for code in readme.md

## CI

- dont need protoc for all actions
- use Swatinem/rust-cache@v2?
- Coverage

## Srad

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

- app state change callbacks

- check result of subscribe and publishes and handle failures accordingly
  - failed subs and online publishes should probably disconnect and return error from run()
- app produces duplicate online states if offline state has been retained on topic
  - resolved by client implementing success of publishes and subscription results.
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

- impl to value types for reference to types ?
- impl to value types for atomic types ?

- compile time checks HasDatatype default type is provided in supported type
  - also checks in places where user overrides default type


- template support
- dataset support
