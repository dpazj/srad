# TODO

## General

- tests
- use tck to test
- examples
- docs

## Srad

- feature flags

## EoN

- add node rebirth cooldown
- provide properties in MessageMetric
- multi part metric message support
  - ddata and cmd
- wait states
- handle client errors better
  - handle subscriptions failing
- multiple brokers support

## App

- subscription config - only need to subscribe to birth data and cmd topics.
- subscribe to own state and publish online message death cert recieved
- add application state subscription config
  - expose application state messages in callback api
- check sequence order and issue rebirths if invalid after timeout
- add ability to reorder out of sequence messages
- wait states
- multi part metric message support
- multiple brokers support

## Client

- remove option from poll event
- decoding of state online/offline messages - do this with the wait for states update

## Rumqtt client

- mqtt options struct
- feature flags
- better ensuring of operation success
  - make sure response to subscription request is made
  - ensure metric publish has been sent/acked

## Types

- template support
- dataset support
