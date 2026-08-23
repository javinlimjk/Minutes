#!/bin/sh
BINARY="$1"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
INFO_PLIST="$SCRIPT_DIR/../Info.plist"

# Embed Info.plist into the __TEXT,__info_plist section so TCC can read
# usage descriptions (NSSpeechRecognitionUsageDescription, etc.) from the
# dev binary which has no .app bundle.
if [ -f "$INFO_PLIST" ]; then
  /usr/bin/ld -r -arch arm64 -sectcreate __TEXT __info_plist "$INFO_PLIST" -o "${BINARY}_with_plist" "$BINARY" 2>/dev/null \
    || cp "$BINARY" "${BINARY}_with_plist"
  mv "${BINARY}_with_plist" "$BINARY" 2>/dev/null || true
fi

codesign -s - -f "$BINARY"
exec "$@"
