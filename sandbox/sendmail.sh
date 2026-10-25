#!/bin/sh
# A local-only mail transport for the training sandbox.
set -eu
mkdir -p /workspace/outbox
cat > /workspace/outbox/message.eml
