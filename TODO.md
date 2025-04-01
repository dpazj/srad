# TODO v0.1

## General
- tests
- samples
- logging

### future 
- remove option from event
- multiple clients/brokers
- Add ability to return success of operations

## Rumqtt

### fututre
  - better ensuring of operation success
    - make sure response to subscription request is made 
    - ensure metric publish has been sent

## Client
   
   - investiagted on clean disconnect sending will manually.

## App
- node and device birth tokens

### future 
 - check sequence order and issue rebirths if invalid after timeout 
  - add ability to reorder out of sequence messages
 - how to handle out of order bdsequences
 - wait states
 - multi part metric message support

## Types 0.1
- valid naming String
  - verify the string's name
- why is metadata under traits

### future 
  - template support
  - dataset support

## EoN 0.1

### future

- provide properties in MessageMetric
- multi part metric message support
  - ddata and cmd
- wait states
- handle client errors better
  - handle subscriptions failing

### Test
- more tests 
- use tck to test
