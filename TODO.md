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
- Add ability to return success of operations
- multiple clients/brokers

### Rumqtt
  - handle multiple clients connected with same Id
  - client disconnection
  - add better config options

## App

- user should manage list/maps of metrics
- user reports if metric mismatch
- data before birth
  - check timestamp ordering
    - timestamp gt birth timstamp,  start timer for rebirth
    - discard premature metrics
- app handles ordering of birth and death
- user handles reporting decode errors and unknown metric errors.

### 0.2
 - check sequence order and issue rebirths if invalid after timeout 

## Types
- valid naming String
  - verify the string's name
- why is metadata under traits
- Zero copy for applicable array types
- full support for types
  - file
  - dataset
  - property set/ property set list

### v0.2 
  - template support

## EoN

- extract common publisher logic from device and node
- user controlled device birth and death
- how to handle when connection is offline and user publish metrics or births device.
- provide properties in MessageMetric
- organise exports

- better server cancellation method
- use tck to test

### v0.2

- multi part metric message support
  - ddata and cmd
- wait states

### Test
 
Test rebirth
test types
