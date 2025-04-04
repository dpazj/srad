# TODO 

## General

- tests
- use tck to test
- examples

### future

- remove option from event
- multiple clients/brokers
- Add ability to return success of operations

## Srad

- feature flags
- how do feature flags work with rumqtt

## Rumqtt client

### future

  - better ensuring of operation success
    - make sure response to subscription request is made
    - ensure metric publish has been sent/acked

## Client

### future 
 - decoding of state online/offline messages - do this with the wait for states update

## App

### future

- subscribe to own state and publish online message if it see's its own death cert
- add application state subscription config 
  - expose application state messages in callback api
 - check sequence order and issue rebirths if invalid after timeout
  - add ability to reorder out of sequence messages
 - wait states
 - multi part metric message support

## Types

### future 

  - template support
  - dataset support

## EoN

### future

- provide properties in MessageMetric
- multi part metric message support
  - ddata and cmd
- wait states
- handle client errors better
  - handle subscriptions failing

