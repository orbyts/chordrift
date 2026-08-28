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
    printf '%s\n' \
        'The selected Chordrift binary does not satisfy this workflow capability contract.' \
        'Install a compatible development binary or use the commands it explicitly supports.' >&2
    exit 64
fi
