#!/usr/bin/env sh

export PORT=${PORT:-4443}
export RUST_LOG=${RUST_LOG:-info}

mkdir cert
# Nothing to see here...
echo "$MOQ_CRT" | base64 -d > dev/moq-demo.crt
echo "$MOQ_KEY" | base64 -d > dev/moq-demo.key

moq-relay-ietf --bind "[::]:${PORT}" --tls-cert dev/moq-demo.crt --tls-key dev/moq-demo.key
