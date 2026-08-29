#!/bin/sh

# One ordinary Chordrift maintenance workflow. The user works in Spotify;
# Chordrift observes the delta, asks only for genuinely ambiguous placement,
# shows the provider-visible net effect, and accepts one authorization.

set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)
ACCOUNT=personal
SKIP_PULL=false
REVIEW_ONLY=false
ARTWORK_MANIFEST=$REPO_ROOT/artwork/canonical/drift-atlas-v4/manifest.json
ARTWORK_REVIEW_MANIFEST=

usage() {
    printf '%s\n' \
        "Usage: scripts/chordrift-maintain.sh [--account LABEL] [--skip-pull] [--review-only]" \
        "" \
        "Observes ordinary Spotify edits, asks only where an unresolved track" \
        "belongs, summarizes the exact net effect, and confirms once." \
        "New playlists, artwork redesign, retirement, and Spin publication are" \
        "separate workflows and are never performed here."
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --account) [ "$#" -ge 2 ] || { usage >&2; exit 2; }; ACCOUNT=$2; shift 2 ;;
        --skip-pull) SKIP_PULL=true; shift ;;
        --review-only) REVIEW_ONLY=true; shift ;;
        --artwork-manifest) [ "$#" -ge 2 ] || { usage >&2; exit 2; }; ARTWORK_MANIFEST=$2; shift 2 ;;
        -h|--help) usage; exit 0 ;;
        *) printf 'Unknown option: %s\n\n' "$1" >&2; usage >&2; exit 2 ;;
    esac
done

case "$ACCOUNT" in ''|*[!A-Za-z0-9_-]*) printf 'Invalid account label.\n' >&2; exit 2 ;; esac
[ "$REVIEW_ONLY" = true ] || { [ -t 0 ] && [ -t 1 ]; } || {
    printf 'Maintenance requires an interactive terminal.\n' >&2
    exit 2
}

CHORDRIFT_BIN=${CHORDRIFT_BIN:-chordrift}
command -v "$CHORDRIFT_BIN" >/dev/null 2>&1 || {
    printf "Chordrift is not executable as '%s'.\n" "$CHORDRIFT_BIN" >&2
    exit 127
}
for helper in chordrift-require-capabilities.sh chordrift-plan-phase.sh; do
    [ -x "$SCRIPT_DIR/$helper" ] || { printf 'Missing helper: %s/%s\n' "$SCRIPT_DIR" "$helper" >&2; exit 1; }
done
"$SCRIPT_DIR/chordrift-require-capabilities.sh" "$CHORDRIFT_BIN" \
    --require maintenance.unified-workflow.v1 \
    --require maintenance.intake-audit.v1 \
    --require maintenance.enumerated-playlist-additions.v1 \
    --require plan-origin.v1

WORK_DIR=$(mktemp -d "${TMPDIR:-/tmp}/chordrift-maintain.XXXXXX")
PLAN_FILE=$WORK_DIR/plan.txt
DETAIL_FILE=$WORK_DIR/details.tsv
AUDIT_FILE=$WORK_DIR/intake.tsv
QUEUE_FILE=$WORK_DIR/reevaluate.tsv
AMBIGUOUS_FILE=$WORK_DIR/ambiguous.tsv
AUTO_MOVES_FILE=$WORK_DIR/automatic-moves.tsv
MOVE_AMBIGUOUS_FILE=$WORK_DIR/ambiguous-moves.tsv
DESTINATIONS_FILE=$WORK_DIR/destinations.tsv
STATUS_FILE=$WORK_DIR/status.txt
ARTWORK_FILE=$WORK_DIR/artwork.txt

cleanup() {
    [ -z "$ARTWORK_REVIEW_MANIFEST" ] || rm -f -- "$ARTWORK_REVIEW_MANIFEST"
    case "$WORK_DIR" in "${TMPDIR:-/tmp}"/chordrift-maintain.*) rm -rf -- "$WORK_DIR" ;; esac
}
trap cleanup EXIT HUP INT TERM

cd "$REPO_ROOT"
run() { "$CHORDRIFT_BIN" "$@"; }
field() { sed -n "s/^$1: //p" "$2" | tail -n 1; }

create_plan() {
    run sync plan --account "$ACCOUNT" >"$PLAN_FILE"
    PLAN_ID=$(field plan_id "$PLAN_FILE")
    run sync plan-show --account "$ACCOUNT" --plan "$PLAN_ID" --details >"$DETAIL_FILE"
    [ "$(field plan_origin "$DETAIL_FILE")" = maintenance ] || {
        printf 'Stopped: ordinary maintenance will not execute a non-maintenance plan.\n' >&2
        exit 3
    }
    [ "$(field snapshot_current "$DETAIL_FILE")" = true ] || {
        printf 'Stopped: Spotify changed during review; run maintenance again.\n' >&2
        exit 3
    }
}

operation_ids() {
    phase=$1
    awk -F '\t' -v phase="$phase" '$1 ~ /^[0-9]+$/ && $2 == phase && $6 != "-" { print $6 }' "$DETAIL_FILE" | sort -u
}

find_ambiguous() {
    : >"$AMBIGUOUS_FILE"
    run intake audit --account "$ACCOUNT" >"$AUDIT_FILE"
    awk -F '\t' '
        $1 == "previously_excluded" || $1 == "known_from_history" || $1 == "genuinely_new" {
            print $1 "\t" $2 "\t" $3 "\t" $11
        }
    ' "$AUDIT_FILE" >>"$AMBIGUOUS_FILE"

    if run reevaluate status --account "$ACCOUNT" >/dev/null 2>&1; then
        run playlists tracks --account "$ACCOUNT" --name Re-evaluate >"$QUEUE_FILE"
        CLEANUP_IDS=$WORK_DIR/cleanup-ids.txt
        operation_ids cleanup >"$CLEANUP_IDS"
        # Keep the first awk input non-empty so NR/FNR file detection remains
        # correct when the plan contains no cleanup IDs.
        printf '%s\n' __no_cleanup_track__ >>"$CLEANUP_IDS"
        awk -F '\t' 'NR == FNR { done[$1] = 1; next }
            $1 ~ /^[0-9]+$/ && !done[$5] { print "reevaluate\t" $2 "\t" $3 "\t" $5 }
        ' "$CLEANUP_IDS" "$QUEUE_FILE" >>"$AMBIGUOUS_FILE"
    fi
    cat "$MOVE_AMBIGUOUS_FILE" >>"$AMBIGUOUS_FILE"
    sort -u "$AMBIGUOUS_FILE" -o "$AMBIGUOUS_FILE"
}

find_managed_moves() {
    : >"$AUTO_MOVES_FILE"
    : >"$MOVE_AMBIGUOUS_FILE"
    awk -F '\t' '$1 ~ /^[0-9]+$/ && $3 == "exclude_track" { print $4 "\t" $6 }' \
        "$DETAIL_FILE" | while IFS="$(printf '\t')" read -r old_destination spotify_id; do
        INSPECTION_FILE=$WORK_DIR/inspect-$spotify_id.txt
        CANDIDATES_FILE=$WORK_DIR/candidates-$spotify_id.txt
        run tracks inspect --account "$ACCOUNT" --spotify-id "$spotify_id" >"$INSPECTION_FILE"
        awk -v old="$old_destination" '
            /^  - .* \(position [0-9]+, role .* signal canonical\)$/ {
                value = $0
                sub(/^  - /, "", value)
                sub(/ \(position .*/, "", value)
                if (tolower(value) != tolower(old)) print value
            }
        ' "$INSPECTION_FILE" | sort -u >"$CANDIDATES_FILE"
        candidate_count=$(wc -l <"$CANDIDATES_FILE" | tr -d ' ')
        track=$(sed -n 's/^track: //p' "$INSPECTION_FILE" | tail -n 1)
        title=${track%% — *}
        artists=${track#* — }
        case "$candidate_count" in
            1)
                destination=$(sed -n '1p' "$CANDIDATES_FILE")
                printf '%s\t%s\t%s\t%s\n' "$title" "$artists" "$spotify_id" "$destination" \
                    >>"$AUTO_MOVES_FILE"
                ;;
            0) ;;
            *) printf 'managed_move\t%s\t%s\t%s\n' "$title" "$artists" "$spotify_id" \
                >>"$MOVE_AMBIGUOUS_FILE" ;;
        esac
    done
}

ensure_editable_proposal() {
    run proposals status --account "$ACCOUNT" >"$STATUS_FILE"
    case "$(field proposal "$STATUS_FILE")" in
        proposed) ;;
        approved) run proposals extend --account "$ACCOUNT" --min-similarity 1 >/dev/null ;;
        *) printf 'No current approved playlist library is available.\n' >&2; exit 3 ;;
    esac
    run proposals list --account "$ACCOUNT" >"$DESTINATIONS_FILE"
}

resolve_destination() {
    requested=$1
    awk -F '\t' -v wanted="$requested" '
        NR > 1 && tolower($4) == tolower(wanted) { matches += 1; key = $3 }
        END { if (matches == 1) print key; else exit 1 }
    ' "$DESTINATIONS_FILE"
}

resolve_ambiguous() {
    [ -s "$AMBIGUOUS_FILE" ] || [ -s "$AUTO_MOVES_FILE" ] || return 0
    [ "$REVIEW_ONLY" = false ] || return 0
    ensure_editable_proposal
    while IFS="$(printf '\t')" read -r title artists spotify_id destination; do
        stable_key=$(resolve_destination "$destination") || {
            printf 'Inferred destination "%s" is no longer unique.\n' "$destination" >&2
            exit 3
        }
        run proposals assign --account "$ACCOUNT" --spotify-id "$spotify_id" \
            --playlist "$stable_key" --reason "Inferred from direct provider move" >/dev/null
        printf 'Inferred move: %s — %s → %s\n' "$title" "$artists" "$destination"
    done <"$AUTO_MOVES_FILE"
    [ ! -s "$AMBIGUOUS_FILE" ] || printf '\nChordrift needs one decision for each track below.\n'
    while IFS="$(printf '\t')" read -r source title artists spotify_id; do
        printf '\n%s — %s\nSource: %s\n' "$title" "$artists" "$source"
        while true; do
            printf "Destination name, 'exclude', or blank to stop: " >/dev/tty
            IFS= read -r answer </dev/tty
            case "$answer" in
                '') printf 'Stopped before Spotify writes.\n'; exit 0 ;;
                exclude|EXCLUDE|Exclude)
                    if [ "$source" != previously_excluded ]; then
                        run tracks exclude --account "$ACCOUNT" --spotify-id "$spotify_id" \
                            --reason "Explicitly excluded during ordinary maintenance" \
                            --confirm "$spotify_id" >/dev/null
                    fi
                    break
                    ;;
                *)
                    if stable_key=$(resolve_destination "$answer"); then
                        if [ "$source" = previously_excluded ]; then
                            run tracks restore --account "$ACCOUNT" --spotify-id "$spotify_id" \
                                --reason "Explicitly restored during ordinary maintenance" \
                                --confirm "$spotify_id" >/dev/null
                        fi
                        run proposals assign --account "$ACCOUNT" --spotify-id "$spotify_id" \
                            --playlist "$stable_key" \
                            --reason "Resolved during ordinary maintenance" >/dev/null
                        break
                    fi
                    printf 'No unique existing destination named "%s". Try again.\n' "$answer" >&2
                    ;;
            esac
        done
    done <"$AMBIGUOUS_FILE"

    run proposals status --account "$ACCOUNT" >"$STATUS_FILE"
    [ "$(field coverage_complete "$STATUS_FILE")" = true ] || {
        printf 'Some library tracks still need a destination; Spotify was not changed.\n' >&2
        exit 3
    }
    PROPOSAL_ID=$(field generation_id "$STATUS_FILE")
    run proposals approve --account "$ACCOUNT" --confirm "$PROPOSAL_ID" >/dev/null

    # An unchanged visual system is inherited internally. This is not a new
    # artwork review and cannot create, rename, or redesign a playlist.
    run artwork status --account "$ACCOUNT" >"$ARTWORK_FILE" 2>/dev/null || true
    if [ "$(field proposal_generation_id "$ARTWORK_FILE")" != "$PROPOSAL_ID" ]; then
        [ -f "$ARTWORK_MANIFEST" ] || {
            printf 'The unchanged approved artwork set is unavailable; Spotify was not changed.\n' >&2
            exit 3
        }
        ARTWORK_SOURCE_DIR=$(CDPATH= cd -- "$(dirname -- "$ARTWORK_MANIFEST")" && pwd)
        ARTWORK_REVIEW_MANIFEST=$(mktemp "$ARTWORK_SOURCE_DIR/.chordrift-maintain.XXXXXX")
        sed -E "s/(\"proposal_generation_id\"[[:space:]]*:[[:space:]]*\")[^\"]*(\")/\\1$PROPOSAL_ID\\2/" \
            "$ARTWORK_MANIFEST" >"$ARTWORK_REVIEW_MANIFEST"
        run artwork import --account "$ACCOUNT" --manifest "$ARTWORK_REVIEW_MANIFEST" >"$ARTWORK_FILE"
        BATCH_ID=$(field batch_id "$ARTWORK_FILE")
        run artwork approve --account "$ACCOUNT" --confirm "$BATCH_ID" >/dev/null
    fi
}

if [ "$SKIP_PULL" = false ]; then
    printf 'Observing Spotify changes…\n'
    run sync pull --account "$ACCOUNT" >/dev/null
fi

create_plan
find_managed_moves
find_ambiguous
if [ -s "$AMBIGUOUS_FILE" ] || [ -s "$AUTO_MOVES_FILE" ]; then
    if [ "$REVIEW_ONLY" = true ]; then
        if [ -s "$AUTO_MOVES_FILE" ]; then
            printf 'Inferred moves:\n'
            awk -F '\t' '{ print $1 " — " $2 " → " $4 }' "$AUTO_MOVES_FILE"
        fi
        if [ -s "$AMBIGUOUS_FILE" ]; then
            printf 'Needs a destination:\n'
            cut -f2-3 "$AMBIGUOUS_FILE"
        fi
        printf 'Review only; Spotify unchanged.\n'
        exit 0
    fi
    resolve_ambiguous
    create_plan
fi

UNSAFE=$(awk -F '\t' '$1 ~ /^[0-9]+$/ && $3 !~ /^(add_track|remove_track|exclude_track|restore_track|remove_saved_track)$/ { print }' "$DETAIL_FILE")
[ -z "$UNSAFE" ] || {
    printf 'Stopped: this change includes work outside ordinary maintenance:\n%s\n' "$UNSAFE" >&2
    exit 3
}

OPERATIONS=$(field operations "$PLAN_FILE")
[ "${OPERATIONS:-0}" -gt 0 ] || { printf 'Everything is already in sync.\n'; exit 0; }

printf '\nChordrift will make these Spotify changes:\n'
awk -F '\t' '$1 ~ /^[0-9]+$/ {
    action = $3
    if (action == "add_track") action = "add"
    else if (action == "remove_track") action = "remove"
    else if (action == "exclude_track") action = "remember removal"
    else if (action == "restore_track") action = "restore"
    printf "  %s: %s%s%s\n", action, $4, ($6 == "-" ? "" : " · "), ($6 == "-" ? "" : $6)
}' "$DETAIL_FILE"

[ "$REVIEW_ONLY" = false ] || { printf 'Review only; Spotify unchanged.\n'; exit 0; }
printf '\nApply these changes? [y/N] ' >/dev/tty
IFS= read -r answer </dev/tty
case "$answer" in y|Y|yes|YES|Yes) ;; *) printf 'Cancelled; Spotify unchanged.\n'; exit 0 ;; esac

iterations=0
while [ "$iterations" -lt 8 ]; do
    iterations=$((iterations + 1))
    create_plan
    [ "$(field operations "$PLAN_FILE")" -gt 0 ] || { printf 'Done. Spotify and Chordrift agree.\n'; exit 0; }
    phase=$(awk -F '\t' '$1 ~ /^[0-9]+$/ { print $2; exit }' "$DETAIL_FILE")
    case "$phase" in publish|reconcile|cleanup) ;; *) printf 'Stopped before unsupported phase %s.\n' "$phase" >&2; exit 3 ;; esac
    "$SCRIPT_DIR/chordrift-plan-phase.sh" --account "$ACCOUNT" --plan "$PLAN_ID" \
        --phase "$phase" --workflow-confirmation "$PLAN_ID" --concise
done

printf 'Stopped after eight convergence steps; inspect unexpected provider churn.\n' >&2
exit 3
