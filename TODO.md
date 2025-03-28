# TODO v0.1

## General

- tests
- samples
- remove panics/better error handling
  - errors in general

- look at atomic operation ordering
- logging

## Client

- Handle disconnections and producing Offline events

### future 
- remove option from event
- multiple clients/brokers
- Add ability to return success of operations

## Rumqtt
  - handle multiple clients connected with same Id
  - client disconnection

### fututre
  - add better config options

## App

- add cmd publish support
- add rebirth support
- add rebirth issuing configuration

### future 
 - check sequence order and issue rebirths if invalid after timeout 
 - wait states?

## Types 0.1
- valid naming String
  - verify the string's name
- why is metadata under traits

### future 
  - template support
  - dataset support

## EoN 0.1

- clean up todos
- logging

### future

- provide properties in MessageMetric
- multi part metric message support
  - ddata and cmd
- wait states


### Test
- more tests 
- use tck to test
