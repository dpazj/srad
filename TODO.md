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

- Check the result of publishes and handle failures accordingly
  - Decide how to handle failed DBIRTH/NDATA/DDATA/NCMD/DCMD/DDEATH publishes
- Multi-part metric message support for NDATA/DDATA and NCMD/DCMD
- Wait for states

## App

- Multi-part metric message support

## Client

- Better guarantees around operation success
  - Surface broker responses to subscriptions: rumqtt 0.24 `subscribe_many` only enqueues the request and incoming SUBACKs are ignored
  - Ensure metric publishes have been sent/acked
  - Blocked on <https://github.com/bytebeamio/rumqtt/pull/916>
