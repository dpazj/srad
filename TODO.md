# TODO

Outstanding work after the 0.6.0 release. Items completed in 0.6.0 are listed at
the bottom for context.

## General

**Features**

- Multi-part metric support
- Wait for states
- Multi-broker support

**Other**

- More tests, especially end-to-end lifecycle coverage
- Use the Sparkplug TCK to test
- Better examples and more detailed docs
- Add tests for the code in `README.md`
- Custom node and device implementations

## CI

- Don't require `protoc` for all actions
- Add coverage reporting

## EoN

- Check the result of subscriptions and publishes and handle failures accordingly
  - Failed subscriptions and NBIRTH publishes should disconnect and return an error from `run()`
  - Decide how to handle failed DBIRTH/DDATA/DDEATH publishes
- Add a node rebirth cooldown for CMD-triggered rebirths
- Provide properties in `MessageMetric`
- Multi-part metric message support for NDATA/DDATA and NCMD/DCMD
- Wait for states
- Multiple broker support

## App

- App state change callbacks
- Check the result of subscriptions and publishes and handle failures accordingly
- Multi-part metric message support
- Wait for app states
- Multiple broker support

## Client

- Better guarantees around operation success
  - Ensure subscription requests receive a response
  - Ensure metric publishes have been sent/acked
  - Blocked on <https://github.com/bytebeamio/rumqtt/pull/916>

## Types

- Implement value conversions for references to types
- Compile-time checks that `HasDataType::DEFAULT_DATATYPE` is included in
  `SUPPORTED_DATATYPES`, including where users override the default type

## Completed in 0.6.0

- Full datatype support, including DataSets and typed DataSet rows
- Typed errors across types, client, app and EoN
- Deferred payload decoding
