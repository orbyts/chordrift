#!/bin/sh

# Operator-only v0.1.4 convenience around the installed Chordrift binary. It
# contains no clustering or persistence logic of its own; the Rust CLI remains
# authoritative while ongoing v0.2 work continues independently.
#
# Default mode is read-only. --apply requires the exact current proposal UUID.
# The script never approves a proposal, creates/applies a sync plan, cleans an
# intake playlist, or writes to Spotify.

set -eu

ACCOUNT=personal
APPLY=false
CONFIRM=
INCLUDE_INTAKE=false
CENTROID_SIMILARITY=0.05
CONSENSUS_DOMINANCE=0.55
CONSENSUS_EVIDENCE=10

usage() {
    printf '%s\n' \
        "Usage: scripts/chordrift-cluster-unresolved.sh [--account LABEL]" \
        "       scripts/chordrift-cluster-unresolved.sh [--account LABEL] --apply --confirm PROPOSAL_UUID" \
        "" \
        "Read-only mode audits unresolved tracks against Chordrift's established" \
        "clustering criteria. Apply mode uses direct centroid similarity >= 0.05," \
        "then analytical-group dominance >= 0.55 with at least 10 placed tracks." \
        "It persists generated destinations as durable assignment revisions." \
        "" \
        "By default, unresolved Inbox/intake and saved/liked tracks" \
        "are reserved for manual decisions and block automatic assignment." \
        "Use --include-intake only after reviewing every remaining intake item." \
        "" \
        "Environment:" \
        "  CHORDRIFT_BIN  Installed Chordrift executable; defaults to chordrift." \
        "" \
        "No mode approves or applies a proposal, cleans intake, or writes Spotify."
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --account)
            [ "$#" -ge 2 ] || { usage >&2; exit 2; }
            ACCOUNT=$2
            shift 2
            ;;
        --apply)
            APPLY=true
            shift
            ;;
        --confirm)
            [ "$#" -ge 2 ] || { usage >&2; exit 2; }
            CONFIRM=$2
            shift 2
            ;;
        --include-intake)
            INCLUDE_INTAKE=true
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

CHORDRIFT_BIN=${CHORDRIFT_BIN:-chordrift}
if ! command -v "$CHORDRIFT_BIN" >/dev/null 2>&1; then
    printf "Chordrift is not installed or executable as '%s'.\n" "$CHORDRIFT_BIN" >&2
    exit 127
fi

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
[ -x "$SCRIPT_DIR/chordrift-require-capabilities.sh" ] || {
    printf 'Missing executable helper: %s/chordrift-require-capabilities.sh\n' "$SCRIPT_DIR" >&2
    exit 1
}
"$SCRIPT_DIR/chordrift-require-capabilities.sh" "$CHORDRIFT_BIN" \
    --require maintenance.intake-workflow.v1

WORK_DIR=$(mktemp -d "${TMPDIR:-/tmp}/chordrift-cluster-unresolved.XXXXXX")
STATUS_FILE=$WORK_DIR/status.txt
UNRESOLVED_FILE=$WORK_DIR/unresolved.tsv
RESERVED_FILE=$WORK_DIR/manual-reserved.tsv
AUDIT_FILE=$WORK_DIR/audit.txt
INSPECTION_FILE=$WORK_DIR/inspection.txt
PLACEMENTS_FILE=$WORK_DIR/placements.tsv
REMAINING_FILE=$WORK_DIR/remaining.tsv

cleanup() {
    rm -f "$STATUS_FILE" "$UNRESOLVED_FILE" "$RESERVED_FILE" "$AUDIT_FILE" \
        "$INSPECTION_FILE" "$PLACEMENTS_FILE" "$REMAINING_FILE"
    rmdir "$WORK_DIR" 2>/dev/null || true
}
trap cleanup EXIT HUP INT TERM

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

stage "Proposal" "capture the exact editable proposal"
run_chordrift proposals status --account "$ACCOUNT" >"$STATUS_FILE"
PROPOSAL_STATE=$(field proposal "$STATUS_FILE")
PROPOSAL_ID=$(field generation_id "$STATUS_FILE")
if [ "$PROPOSAL_STATE" != proposed ]; then
    printf 'The latest proposal is %s, not editable. No assignment was attempted.\n' \
        "$PROPOSAL_STATE" >&2
    exit 1
fi

run_chordrift proposals unresolved --account "$ACCOUNT" --limit 10000 >"$UNRESOLVED_FILE"
UNRESOLVED_COUNT=$(awk 'NR > 1 { count += 1 } END { print count + 0 }' "$UNRESOLVED_FILE")
if [ "$UNRESOLVED_COUNT" -eq 0 ]; then
    printf 'Proposal %s has no unresolved tracks.\n' "$PROPOSAL_ID"
    exit 0
fi

awk -F '\t' '
    NR == 1 { print; next }
    {
        evidence = tolower($3 " " $4)
        if (evidence ~ /intake/ || evidence ~ /saved/ || evidence ~ /inbox/ ||
            evidence ~ /re-evaluate/ || evidence ~ /liked songs/) print
    }
' "$UNRESOLVED_FILE" >"$RESERVED_FILE"
RESERVED_COUNT=$(awk 'NR > 1 { count += 1 } END { print count + 0 }' "$RESERVED_FILE")

if [ "$RESERVED_COUNT" -gt 0 ] && [ "$INCLUDE_INTAKE" = false ]; then
    stage "Manual review required" "$RESERVED_COUNT intake/liked track(s) are reserved"
    cat "$RESERVED_FILE"
    printf '\nAssign your Hindi, Telugu, Tamil, and A. R. Rahman intake tracks manually first.\n'
    printf 'After reviewing any remaining intake tracks, rerun with --include-intake if appropriate.\n'
    exit 3
fi

stage "Audit" "measure the established clustering criteria without mutation"
run_chordrift proposals placement-audit --account "$ACCOUNT" >"$AUDIT_FILE"
cat "$AUDIT_FILE"
STRONG=$(field strong_existing_fit "$AUDIT_FILE")
USABLE=$(field usable_existing_fit "$AUDIT_FILE")
WEAK=$(field weak_fit_review "$AUDIT_FILE")
UNEMBEDDED=$(field unembedded_unresolved_tracks "$AUDIT_FILE")

printf '\nProposal: %s\n' "$PROPOSAL_ID"
printf 'Unresolved: %s · strong: %s · usable: %s · weak/group fallback: %s · unembedded: %s\n' \
    "$UNRESOLVED_COUNT" "$STRONG" "$USABLE" "$WEAK" "$UNEMBEDDED"

if [ "$APPLY" = false ]; then
    printf '\nRead-only audit complete. To execute this exact proposal, run:\n'
    printf '  %s --account %s --apply --confirm %s' "$0" "$ACCOUNT" "$PROPOSAL_ID"
    if [ "$INCLUDE_INTAKE" = true ]; then
        printf ' --include-intake'
    fi
    printf '\n'
    exit 0
fi

if [ -z "$CONFIRM" ] || [ "$CONFIRM" != "$PROPOSAL_ID" ]; then
    printf '\n--confirm must exactly match current proposal %s. No assignment was attempted.\n' \
        "$PROPOSAL_ID" >&2
    exit 2
fi

stage "Centroid" "assign direct destination similarity >= $CENTROID_SIMILARITY"
run_chordrift proposals centroid-assign \
    --account "$ACCOUNT" \
    --min-similarity "$CENTROID_SIMILARITY"

stage "Consensus" "assign remaining groups at >= $CONSENSUS_DOMINANCE dominance and >= $CONSENSUS_EVIDENCE evidence"
run_chordrift proposals consensus-assign \
    --account "$ACCOUNT" \
    --min-dominance "$CONSENSUS_DOMINANCE" \
    --min-evidence "$CONSENSUS_EVIDENCE"

: >"$PLACEMENTS_FILE"
: >"$REMAINING_FILE"
while IFS="$(printf '\t')" read -r title artists source_classes source_playlists spotify_id; do
    [ "$spotify_id" != spotify_id ] || continue
    run_chordrift tracks inspect --account "$ACCOUNT" --spotify-id "$spotify_id" >"$INSPECTION_FILE"
    EXCLUDED=$(field excluded "$INSPECTION_FILE")
    if [ "$EXCLUDED" != false ]; then
        printf '%s became excluded during clustering; refusing durable assignment.\n' "$spotify_id" >&2
        exit 1
    fi
    DESTINATION_KEY=$(sed -n 's/^  - .* key \([^,]*\), source generated)$/\1/p' "$INSPECTION_FILE" | sed -n '1p')
    METHOD=$(sed -n 's/^    provenance: .*"method":"\([^"]*\)".*/\1/p' "$INSPECTION_FILE" | sed -n '1p')
    if [ -z "$DESTINATION_KEY" ]; then
        printf '%s\t%s\t%s\t%s\t%s\n' \
            "$title" "$artists" "$source_classes" "$source_playlists" "$spotify_id" \
            >>"$REMAINING_FILE"
        continue
    fi
    [ -n "$METHOD" ] || METHOD=generated-clustering
    printf '%s\t%s\t%s\n' "$DESTINATION_KEY" "$METHOD" "$spotify_id" >>"$PLACEMENTS_FILE"
done <"$UNRESOLVED_FILE"

stage "Persist" "supersede needs-review decisions with exact generated destinations"
cut -f1,2 "$PLACEMENTS_FILE" | sort -u | while IFS="$(printf '\t')" read -r destination_key method; do
    [ -n "$destination_key" ] || continue
    case "$method" in
        current-playlist-centroid)
            REASON="Regular clustering: current destination centroid similarity >= $CENTROID_SIMILARITY"
            ;;
        analytical-cluster-dominant-destination)
            REASON="Regular clustering: analytical group dominance >= $CONSENSUS_DOMINANCE with at least $CONSENSUS_EVIDENCE placed tracks"
            ;;
        *)
            REASON="Regular clustering: generated placement from proposal $PROPOSAL_ID"
            ;;
    esac
    set -- proposals assign --account "$ACCOUNT"
    while IFS="$(printf '\t')" read -r row_key row_method spotify_id; do
        if [ "$row_key" = "$destination_key" ] && [ "$row_method" = "$method" ]; then
            set -- "$@" --spotify-id "$spotify_id"
        fi
    done <"$PLACEMENTS_FILE"
    set -- "$@" --playlist "$destination_key" --reason "$REASON"
    run_chordrift "$@"
done

stage "Verify" "show durable proposal coverage and anything still unresolved"
run_chordrift proposals status --account "$ACCOUNT"
run_chordrift proposals unresolved --account "$ACCOUNT" --limit 100

if [ -s "$REMAINING_FILE" ]; then
    printf '\nSome tracks had no accepted automatic placement and remain for manual review:\n'
    cat "$REMAINING_FILE"
    exit 3
fi

printf '\nClustering decisions are durable in Neon. No proposal was approved and no Spotify write was attempted.\n'
