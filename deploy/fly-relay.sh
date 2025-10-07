#!/usr/bin/env sh

export PORT=${PORT:-4443}
export RUST_LOG=${RUST_LOG:-info}

mkdir cert
# Nothing to see here...
echo "$MOQ_CRT" | base64 -d > dev/moq-demo.crt
echo "$MOQ_KEY" | base64 -d > dev/moq-demo.key

# Set up qlog directory if QLOG_DIR is set
QLOG_ARGS=""
if [ -n "${QLOG_DIR}" ]; then
    mkdir -p "${QLOG_DIR}"
    QLOG_ARGS="--qlog-dir ${QLOG_DIR}"
    echo "qlog enabled: writing to ${QLOG_DIR}"
fi

moq-relay-ietf --bind "[::]:${PORT}" --tls-cert dev/moq-demo.crt --tls-key dev/moq-demo.key ${QLOG_ARGS}
