#!/bin/sh

set -e

# Headless mode is used by tck-runner: run HiveMQ only, no web console.
if [ "${TCK_HEADLESS:-0}" = "1" ]; then
    exec /opt/hivemq/bin/run.sh
fi

/opt/hivemq/bin/run.sh &

echo "Waiting for the HiveMQ broker to accept connections..."
until nc -z localhost 1883 2>/dev/null; do
    sleep 1
done

cd /sparkplug-3.0.0/tck/webconsole
exec yarn start
