#!/bin/sh

set -e

/opt/hivemq/bin/run.sh &

echo "Waiting for the HiveMQ broker to accept connections..."
until nc -z localhost 1883 2>/dev/null; do
    sleep 1
done

cd /sparkplug-3.0.0/tck/webconsole
exec yarn start
