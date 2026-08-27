#!/bin/sh

# Resolve a destination by display name or stable key and record one or more
# explicit account-owner placements in the latest editable proposal.

set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)
ACCOUNT=personal
DESTINATION_NAME=
DESTINATION_KEY=
REASON=

usage() {
    printf '%s\n' \
        "Usage: scripts/chordrift-manual-place.sh --to NAME --spotify-id ID [--spotify-id ID ...] --reason TEXT" \
        "       scripts/chordrift-manual-place.sh --playlist STABLE_KEY --spotify-id ID [--spotify-id ID ...] --reason TEXT" \
        "" \
        "Records explicit placements in the latest editable proposal." \
        "It accepts unresolved tracks and corrections of existing placements," \
        "but refuses active exclusions. It never writes Spotify." \
        "" \
        "Environment:" \
        "  CHORDRIFT_BIN  Installed Chordrift executable; defaults to chordrift."
}

WORK_DIR=$(mktemp -d "${TMPDIR:-/tmp}/chordrift-manual-place.XXXXXX")
IDS_FILE=$WORK_DIR/spotify-ids.txt
STATUS_FILE=$WORK_DIR/status.txt
PLAYLISTS_FILE=$WORK_DIR/playlists.tsv
INSPECTION_FILE=$WORK_DIR/inspection.txt
: >"$IDS_FILE"

cleanup() {
    rm -f "$IDS_FILE" "$STATUS_FILE" "$PLAYLISTS_FILE" "$INSPECTION_FILE"
    rmdir "$WORK_DIR" 2>/dev/null || true
}
trap cleanup EXIT HUP INT TERM

while [ "$#" -gt 0 ]; do
    case "$1" in
        --account)
            [ "$#" -ge 2 ] || { usage >&2; exit 2; }
            ACCOUNT=$2
            shift 2
            ;;
        --to)
            [ "$#" -ge 2 ] || { usage >&2; exit 2; }
            DESTINATION_NAME=$2
            shift 2
            ;;
        --playlist)
            [ "$#" -ge 2 ] || { usage >&2; exit 2; }
            DESTINATION_KEY=$2
            shift 2
            ;;
        --spotify-id)
            [ "$#" -ge 2 ] || { usage >&2; exit 2; }
            printf '%s\n' "$2" >>"$IDS_FILE"
            shift 2
            ;;
        --reason)
            [ "$#" -ge 2 ] || { usage >&2; exit 2; }
            REASON=$2
            shift 2
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            printf 'Unknown option: %s\n\n' "$1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

case "$ACCOUNT" in
    ''|*[!A-Za-z0-9_-]*) printf 'Invalid account label.\n' >&2; exit 2 ;;
esac
if { [ -n "$DESTINATION_NAME" ] && [ -n "$DESTINATION_KEY" ]; } ||
   { [ -z "$DESTINATION_NAME" ] && [ -z "$DESTINATION_KEY" ]; }; then
    printf 'Supply exactly one of --to NAME or --playlist STABLE_KEY.\n' >&2
    exit 2
fi
[ -s "$IDS_FILE" ] || { printf 'Supply at least one --spotify-id.\n' >&2; exit 2; }
[ -n "$REASON" ] || { printf 'Supply a non-empty --reason.\n' >&2; exit 2; }
DUPLICATE_ID=$(sort "$IDS_FILE" | uniq -d | sed -n '1p')
[ -z "$DUPLICATE_ID" ] || {
    printf 'Spotify track ID was supplied more than once: %s\n' "$DUPLICATE_ID" >&2
    exit 2
}
while IFS= read -r spotify_id; do
    case "$spotify_id" in
        ''|*[!A-Za-z0-9]*)
            printf 'Invalid Spotify track ID: %s\n' "$spotify_id" >&2
            exit 2
            ;;
    esac
done <"$IDS_FILE"

cd "$REPO_ROOT"
CHORDRIFT_BIN=${CHORDRIFT_BIN:-chordrift}
command -v "$CHORDRIFT_BIN" >/dev/null 2>&1 || {
    printf "Chordrift is not installed or executable as '%s'.\n" "$CHORDRIFT_BIN" >&2
    exit 127
}
run_chordrift() { "$CHORDRIFT_BIN" "$@"; }
field() { sed -n "s/^$1: //p" "$2" | tail -n 1; }

run_chordrift proposals status --account "$ACCOUNT" >"$STATUS_FILE"
PROPOSAL_STATE=$(field proposal "$STATUS_FILE")
if [ "$PROPOSAL_STATE" != proposed ]; then
    printf 'The latest proposal is %s, not editable. No placement was recorded.\n' \
        "$PROPOSAL_STATE" >&2
    printf 'Prepare/review a new editable proposal before recording manual placements.\n' >&2
    exit 1
fi

run_chordrift proposals list --account "$ACCOUNT" >"$PLAYLISTS_FILE"
if [ -n "$DESTINATION_NAME" ]; then
    MATCHING_KEYS=$(awk -F '\t' -v target="$DESTINATION_NAME" '
        NR > 1 && tolower($4) == tolower(target) { print $3 }
    ' "$PLAYLISTS_FILE")
    MATCH_COUNT=$(printf '%s\n' "$MATCHING_KEYS" | sed '/^$/d' | wc -l | tr -d ' ')
    [ "$MATCH_COUNT" -eq 1 ] || {
        printf "Destination '%s' matched %s playlists. No placement was recorded.\n" \
            "$DESTINATION_NAME" "$MATCH_COUNT" >&2
        exit 1
    }
    DESTINATION_KEY=$(printf '%s\n' "$MATCHING_KEYS" | sed -n '1p')
else
    awk -F '\t' -v target="$DESTINATION_KEY" \
        'NR > 1 && $3 == target { found = 1 } END { exit !found }' \
        "$PLAYLISTS_FILE" || {
            printf "Stable destination '%s' is not in the latest proposal.\n" \
                "$DESTINATION_KEY" >&2
            exit 1
        }
    DESTINATION_NAME=$(awk -F '\t' -v target="$DESTINATION_KEY" \
        'NR > 1 && $3 == target { print $4; exit }' "$PLAYLISTS_FILE")
fi

printf 'Destination: %s (%s)\n' "$DESTINATION_NAME" "$DESTINATION_KEY"
while IFS= read -r spotify_id; do
    run_chordrift tracks inspect --account "$ACCOUNT" --spotify-id "$spotify_id" >"$INSPECTION_FILE"
    EXCLUDED=$(field excluded "$INSPECTION_FILE")
    if [ "$EXCLUDED" != false ]; then
        printf 'Track %s has an active exclusion (%s). Nothing was recorded.\n' \
            "$spotify_id" "$EXCLUDED" >&2
        exit 1
    fi
    sed -n '1p;/^canonical_placements:/p;/^  - .* key /p' "$INSPECTION_FILE"
done <"$IDS_FILE"

set -- proposals assign --account "$ACCOUNT"
while IFS= read -r spotify_id; do
    set -- "$@" --spotify-id "$spotify_id"
done <"$IDS_FILE"
set -- "$@" --playlist "$DESTINATION_KEY" --reason "$REASON"
run_chordrift "$@"

run_chordrift proposals status --account "$ACCOUNT"
run_chordrift proposals unresolved --account "$ACCOUNT" --limit 100
printf '\nPlacement intent is durable in Neon. No proposal approval or Spotify write occurred.\n'
