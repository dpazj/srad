# TODO

## General

**Features**

- Multi-part metric support
- Wait for states

**Other**

- More tests, especially end-to-end lifecycle coverage

## CI

- Don't require `protoc` for all actions
- Add coverage reporting

## EoN

- Add a node rebirth cooldown for CMD-triggered rebirths
- Provide properties in `MessageMetric`
- Multi-part metric message support for NDATA/DDATA and NCMD/DCMD
- Wait for states

## App

- App state change callbacks
- Multi-part metric message support
- Wait for app states

## Client

- Better guarantees around operation success
  - Ensure subscription requests receive a response
  - Ensure metric publishes have been sent/acked
  - Blocked on <https://github.com/bytebeamio/rumqtt/pull/916>

