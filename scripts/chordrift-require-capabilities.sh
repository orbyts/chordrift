#!/bin/sh

# Fail closed unless one installed Chordrift binary advertises every exact
# capability requested by an operator helper. Version strings are informational;
# the machine-readable capability command is the compatibility authority.

set -eu

[ "$#" -ge 3 ] || {
    printf '%s\n' \
        'Usage: chordrift-require-capabilities.sh BINARY --require CAPABILITY [--require CAPABILITY ...]' >&2
    exit 2
}

CHORDRIFT_BIN=$1
shift

if ! "$CHORDRIFT_BIN" capabilities "$@" >/dev/null; then
    RESOLVED_BIN=$(command -v "$CHORDRIFT_BIN" 2>/dev/null || printf '%s' "$CHORDRIFT_BIN")
    printf '%s\n' \
        "Selected binary: $RESOLVED_BIN" \
        'The selected Chordrift binary does not satisfy this workflow capability contract.' \
        'Install current main with `cargo install --path . --force`, or clear a stale CHORDRIFT_BIN override.' >&2
    exit 64
fi
