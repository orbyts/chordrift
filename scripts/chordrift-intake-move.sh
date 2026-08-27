#!/bin/sh

# Record reviewed Inbox tracks in an existing editable Chordrift proposal.
# This helper writes assignment intent to Neon, but never approves a proposal,
# creates an apply plan, or writes to Spotify.

set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)
ACCOUNT=personal
DESTINATION_NAME=
DESTINATION_KEY=
REASON=
PREPARE=false

usage() {
    printf '%s\n' \
        "Usage: scripts/chordrift-intake-move.sh --to NAME --spotify-id ID [--spotify-id ID ...] --reason TEXT" \
        "       scripts/chordrift-intake-move.sh --playlist STABLE_KEY --spotify-id ID [--spotify-id ID ...] --reason TEXT" \
        "       scripts/chordrift-intake-move.sh [--account LABEL] [--prepare] ..." \
        "" \
        "Records reviewed Inbox tracks in the latest editable proposal." \
        "The destination may be an exact proposal name or stable playlist key." \
        "Tracks with an active exclusion, tracks outside Inbox, and tracks that" \
        "are already resolved are refused before any assignment is recorded." \
        "" \
        "Environment:" \
        "  CHORDRIFT_BIN  Optional executable name or exact path." \
        "                 Defaults to the installed 'chordrift' command." \
        "" \
        "This helper changes Neon proposal intent only. It never approves," \
        "plans, applies, removes an Inbox item, or writes to Spotify."
}

WORK_DIR=$(mktemp -d "${TMPDIR:-/tmp}/chordrift-intake-move.XXXXXX")
IDS_FILE=$WORK_DIR/spotify-ids.txt
STATUS_FILE=$WORK_DIR/status.txt
PLAYLISTS_FILE=$WORK_DIR/playlists.tsv
UNRESOLVED_FILE=$WORK_DIR/unresolved.tsv
INSPECTION_FILE=$WORK_DIR/inspection.txt
: >"$IDS_FILE"

cleanup() {
    rm -f "$IDS_FILE" "$STATUS_FILE" "$PLAYLISTS_FILE" "$UNRESOLVED_FILE" "$INSPECTION_FILE"
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
        --prepare)
            PREPARE=true
            shift
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
    ''|*[!A-Za-z0-9_-]*)
        printf 'Account labels may contain only letters, numbers, - and _.\n' >&2
        exit 2
        ;;
esac

if { [ -n "$DESTINATION_NAME" ] && [ -n "$DESTINATION_KEY" ]; } ||
   { [ -z "$DESTINATION_NAME" ] && [ -z "$DESTINATION_KEY" ]; }; then
    printf 'Supply exactly one of --to NAME or --playlist STABLE_KEY.\n' >&2
    exit 2
fi
if [ ! -s "$IDS_FILE" ]; then
    printf 'Supply at least one --spotify-id.\n' >&2
    exit 2
fi
if [ -z "$REASON" ]; then
    printf 'Supply a non-empty --reason for the durable audit record.\n' >&2
    exit 2
fi

while IFS= read -r spotify_id; do
    case "$spotify_id" in
        ''|*[!A-Za-z0-9]*)
            printf 'Invalid Spotify track ID: %s\n' "$spotify_id" >&2
            exit 2
            ;;
    esac
done <"$IDS_FILE"

DUPLICATE_ID=$(sort "$IDS_FILE" | uniq -d | sed -n '1p')
if [ -n "$DUPLICATE_ID" ]; then
    printf 'Spotify track ID was supplied more than once: %s\n' "$DUPLICATE_ID" >&2
    exit 2
fi

cd "$REPO_ROOT"

CHORDRIFT_BIN=${CHORDRIFT_BIN:-chordrift}
if ! command -v "$CHORDRIFT_BIN" >/dev/null 2>&1; then
    printf "Chordrift is not installed or executable as '%s'.\n" "$CHORDRIFT_BIN" >&2
    printf 'Install Chordrift or set CHORDRIFT_BIN to its exact executable path.\n' >&2
    exit 127
fi

run_chordrift() {
    "$CHORDRIFT_BIN" "$@"
}

stage() {
    if [ -t 1 ]; then
        printf '\n\033[1;36m%s\033[0m  \033[2m— %s\033[0m\n' "$1" "$2"
    else
        printf '\n%s: %s\n' "$1" "$2"
    fi
}

field() {
    key=$1
    file=$2
    sed -n "s/^${key}: //p" "$file" | tail -n 1
}

stage "Proposal" "verify that manual decisions are currently editable"
run_chordrift proposals status --account "$ACCOUNT" >"$STATUS_FILE"
PROPOSAL_STATE=$(field proposal "$STATUS_FILE")
PROPOSAL_ID=$(field generation_id "$STATUS_FILE")
case "$PROPOSAL_STATE" in
    proposed|approved) ;;
    *)
        printf 'The latest proposal is %s, not usable for manual placement. No assignment was recorded.\n' \
            "$PROPOSAL_STATE" >&2
        exit 1
        ;;
esac

if [ "$PROPOSAL_STATE" = approved ] && [ "$PREPARE" = false ]; then
    printf 'The latest proposal is approved and cannot be edited in place.\n' >&2
    printf 'After reviewing the safety rule, repeat this command with --prepare.\n' >&2
    printf '%s\n' '--prepare is allowed only when the supplied IDs cover every unresolved track.' >&2
    exit 1
fi

run_chordrift proposals list --account "$ACCOUNT" >"$PLAYLISTS_FILE"
if [ -n "$DESTINATION_NAME" ]; then
    MATCHING_KEYS=$(awk -F '\t' -v target="$DESTINATION_NAME" '
        NR > 1 && tolower($4) == tolower(target) { print $3 }
    ' "$PLAYLISTS_FILE")
    MATCH_COUNT=$(printf '%s\n' "$MATCHING_KEYS" | sed '/^$/d' | wc -l | tr -d ' ')
    if [ "$MATCH_COUNT" -ne 1 ]; then
        printf "Destination name '%s' matched %s proposed playlists. No assignment was recorded.\n" \
            "$DESTINATION_NAME" "$MATCH_COUNT" >&2
        printf 'Use the exact display name or pass --playlist with a stable key.\n' >&2
        exit 1
    fi
    DESTINATION_KEY=$(printf '%s\n' "$MATCHING_KEYS" | sed -n '1p')
else
    if ! awk -F '\t' -v target="$DESTINATION_KEY" 'NR > 1 && $3 == target { found = 1 } END { exit !found }' "$PLAYLISTS_FILE"; then
        printf "Stable destination '%s' is not in the latest proposal. No assignment was recorded.\n" \
            "$DESTINATION_KEY" >&2
        exit 1
    fi
    DESTINATION_NAME=$(awk -F '\t' -v target="$DESTINATION_KEY" 'NR > 1 && $3 == target { print $4; exit }' "$PLAYLISTS_FILE")
fi

stage "Safety check" "require unresolved Inbox tracks with no active exclusion"
run_chordrift proposals unresolved --account "$ACCOUNT" --limit 10000 >"$UNRESOLVED_FILE"
while IFS= read -r spotify_id; do
    run_chordrift tracks inspect --account "$ACCOUNT" --spotify-id "$spotify_id" >"$INSPECTION_FILE"
    EXCLUDED=$(field excluded "$INSPECTION_FILE")
    if [ "$EXCLUDED" != false ]; then
        printf "Track %s has an active exclusion (%s). No assignment was recorded.\n" \
            "$spotify_id" "$EXCLUDED" >&2
        exit 1
    fi
    if ! grep -F '  - Inbox (' "$INSPECTION_FILE" >/dev/null 2>&1; then
        printf 'Track %s is not currently in Inbox. No assignment was recorded.\n' "$spotify_id" >&2
        exit 1
    fi
    if ! awk -F '\t' -v target="$spotify_id" 'NR > 1 && $5 == target { found = 1 } END { exit !found }' "$UNRESOLVED_FILE"; then
        printf 'Track %s is not unresolved; it may already have a placement. No assignment was recorded.\n' \
            "$spotify_id" >&2
        exit 1
    fi
    awk -F '\t' -v target="$spotify_id" '
        NR > 1 && $5 == target { printf "  %s — %s\n", $1, $2; exit }
    ' "$UNRESOLVED_FILE"
done <"$IDS_FILE"

if [ "$PROPOSAL_STATE" = approved ]; then
    UNRESOLVED_COUNT=$(awk 'NR > 1 { count += 1 } END { print count + 0 }' "$UNRESOLVED_FILE")
    SUPPLIED_COUNT=$(wc -l <"$IDS_FILE" | tr -d ' ')
    if [ "$UNRESOLVED_COUNT" -ne "$SUPPLIED_COUNT" ]; then
        printf '%s unresolved tracks exist, but %s were supplied. No proposal was prepared.\n' \
            "$UNRESOLVED_COUNT" "$SUPPLIED_COUNT" >&2
        printf '%s\n' '--prepare requires the complete unresolved set so unrelated tracks cannot be changed.' >&2
        exit 1
    fi
    stage "Prepare" "clone the approved structure into one editable proposal"
    run_chordrift proposals extend --account "$ACCOUNT" --min-similarity 1
    run_chordrift proposals status --account "$ACCOUNT" >"$STATUS_FILE"
    PROPOSAL_STATE=$(field proposal "$STATUS_FILE")
    PROPOSAL_ID=$(field generation_id "$STATUS_FILE")
    if [ "$PROPOSAL_STATE" != proposed ]; then
        printf 'Chordrift did not produce an editable proposal. No manual assignment was attempted.\n' >&2
        exit 1
    fi
fi

stage "Record" "assign reviewed Inbox tracks to $DESTINATION_NAME in Neon"
set -- proposals assign --account "$ACCOUNT"
while IFS= read -r spotify_id; do
    set -- "$@" --spotify-id "$spotify_id"
done <"$IDS_FILE"
set -- "$@" --playlist "$DESTINATION_KEY" --reason "$REASON"
run_chordrift "$@"

stage "Review" "show the resulting proposal state"
run_chordrift proposals status --account "$ACCOUNT" >"$STATUS_FILE"
cat "$STATUS_FILE"
PROPOSAL_ID=$(field generation_id "$STATUS_FILE")
COVERAGE_COMPLETE=$(field coverage_complete "$STATUS_FILE")

printf '\nRecorded in Neon. No Spotify write was attempted.\n'
if [ "$COVERAGE_COMPLETE" = true ]; then
    printf 'After reviewing the entire proposal, approve it explicitly with:\n'
    printf '  chordrift proposals approve --account %s --confirm %s\n' "$ACCOUNT" "$PROPOSAL_ID"
    printf 'Then run the normal plan/readiness/apply workflow.\n'
else
    printf 'The proposal still has unresolved inventory. Add the remaining decisions before approval.\n'
    run_chordrift proposals unresolved --account "$ACCOUNT" --limit 100
fi
