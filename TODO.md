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
  - client disconnection
  - add better config options

## App

- TODO

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
- provide properties in MessageMetric

- organise exports
- multi part metric message support
  - ddata and cmd

- better server cancellation method
- use tck to test
- look into fx hashmap test

### v0.2

- multi part publishing

### Test
 
Test rebirth
test types
