#!/bin/bash

# Script which automates publishing a crates.io release of the srad crates.

set -ex

if [ "$#" -ne 0 ]
then
  echo "Usage: $0"
  exit 1
fi

DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"

CRATES=( \
  "srad-macros" \
  "srad-types" \
  "srad-client" \
  "srad-client-rumqtt" \
  "srad-app" \
  "srad-eon" \
  "srad" \
)

for CRATE in "${CRATES[@]}"; do
  pushd "$DIR/$CRATE"

  echo "Publishing $CRATE"

  cargo publish --locked

  # The crates.io index needs to see the new crate before a dependent crate can
  # be published, so wait long enough for propagation instead of relying on the
  # retry path. If a dependent crate still fails, wait and publish the remaining
  # crates by hand: this script aborts on the first error, and an already
  # published version cannot be uploaded again.
  echo "Sleeping 30 seconds for the release to be visible"
  sleep 30

  popd
done
