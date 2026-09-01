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
CONFIRMED_PLAN=
ARTWORK_MANIFEST=$REPO_ROOT/artwork/canonical/drift-atlas-v5-indian-surfaces/manifest.json
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
        --confirmed-plan) [ "$#" -ge 2 ] || { usage >&2; exit 2; }; CONFIRMED_PLAN=$2; shift 2 ;;
        --artwork-manifest) [ "$#" -ge 2 ] || { usage >&2; exit 2; }; ARTWORK_MANIFEST=$2; shift 2 ;;
        -h|--help) usage; exit 0 ;;
        *) printf 'Unknown option: %s\n\n' "$1" >&2; usage >&2; exit 2 ;;
    esac
done

case "$ACCOUNT" in ''|*[!A-Za-z0-9_-]*) printf 'Invalid account label.\n' >&2; exit 2 ;; esac
[ "$REVIEW_ONLY" = true ] || [ -n "$CONFIRMED_PLAN" ] || { [ -t 0 ] && [ -t 1 ]; } || {
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
    --require maintenance.bulk-plan-preview.v1 \
    --require maintenance.direct-managed-intake.v1 \
    --require maintenance.artwork-carry-forward.v1 \
    --require maintenance.provider-order-intent.v1 \
    --require maintenance.provider-baseline.v1 \
    --require maintenance.saved-intake-disposition.v1 \
    --require plan-origin.v1

WORK_DIR=$(mktemp -d "${TMPDIR:-/tmp}/chordrift-maintain.XXXXXX")
PLAN_FILE=$WORK_DIR/plan.txt
DETAIL_FILE=$WORK_DIR/details.tsv
AUDIT_FILE=$WORK_DIR/intake.tsv
AMBIGUOUS_FILE=$WORK_DIR/ambiguous.tsv
AUTO_MOVES_FILE=$WORK_DIR/automatic-moves.tsv
RESOLVED_AUTO_MOVES_FILE=$WORK_DIR/resolved-automatic-moves.tsv
MOVE_AMBIGUOUS_FILE=$WORK_DIR/ambiguous-moves.tsv
ORDER_DRIFT_FILE=$WORK_DIR/provider-order.tsv
LIKED_DECISIONS_FILE=$WORK_DIR/liked-decisions.tsv
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

find_ambiguous() {
    : >"$AMBIGUOUS_FILE"
    : >"$LIKED_DECISIONS_FILE"
    run intake audit --account "$ACCOUNT" >"$AUDIT_FILE"
    awk -F '\t' '
        FILENAME == ARGV[1] {
            if (NF == 5) inferred[$3] = 1
            next
        }
        ($1 == "previously_excluded" || $1 == "known_from_history" || $1 == "genuinely_new") &&
            !($11 in inferred) {
            print $1 "\t" $2 "\t" $3 "\t" $11
        }
        $1 == "direct_managed_addition" && index($5, " / ") > 0 {
            print $1 "\t" $2 "\t" $3 "\t" $11
        }
    ' "$AUTO_MOVES_FILE" "$AUDIT_FILE" >>"$AMBIGUOUS_FILE"

    awk -F '\t' '$1 == "direct_managed_addition" && index($5, " / ") == 0 {
        print $2 "\t" $3 "\t" $11 "\tNew intake\t" $5
    }' "$AUDIT_FILE" >>"$AUTO_MOVES_FILE"

    awk -F '\t' '$1 == "already_covered" && index($4, "Liked Songs") > 0 &&
        $5 != "" && $12 == "-" {
        print $2 "\t" $3 "\t" $11 "\t" $5
    }' "$AUDIT_FILE" >>"$LIKED_DECISIONS_FILE"

    cat "$MOVE_AMBIGUOUS_FILE" >>"$AMBIGUOUS_FILE"
    sort -u "$AMBIGUOUS_FILE" -o "$AMBIGUOUS_FILE"
}

resolve_saved_intake() {
    [ -s "$LIKED_DECISIONS_FILE" ] || return 0
    while IFS="$(printf '\t')" read -r title artists spotify_id destinations; do
        printf '\n%s — %s\nAlready in: %s\n' "$title" "$artists" "$destinations"
        printf 'Keep it in Liked Songs too? [y/N] ' >/dev/tty
        IFS= read -r answer </dev/tty
        case "$answer" in
            y|Y|yes|YES|Yes)
                disposition=preserve
                printf 'Keeping it in Liked Songs and %s.\n' "$destinations"
                ;;
            *)
                disposition=clear-after-verified-assignment
                printf 'It will remain in %s and be removed from Liked Songs after confirmation.\n' \
                    "$destinations"
                ;;
        esac
        run intake liked-disposition --account "$ACCOUNT" \
            --spotify-id "$spotify_id" --disposition "$disposition" \
            --reason "Explicit ordinary-maintenance saved-intake decision" >/dev/null
    done <"$LIKED_DECISIONS_FILE"
}

find_managed_moves() {
    : >"$AUTO_MOVES_FILE"
    : >"$MOVE_AMBIGUOUS_FILE"
    awk -F '\t' '$1 ~ /^[0-9]+$/ && $11 == "direct_move" {
            print $9 "\t" $10 "\t" $6 "\t" $12 "\t" $13
        }
        $1 ~ /^[0-9]+$/ && $11 == "ambiguous_move" {
            print "managed_move\t" $9 "\t" $10 "\t" $6
        }' "$DETAIL_FILE" >"$WORK_DIR/managed-moves.tsv"
    awk -F '\t' 'NF == 5 { print }' "$WORK_DIR/managed-moves.tsv" >"$AUTO_MOVES_FILE"
    awk -F '\t' 'NF == 4 { print }' "$WORK_DIR/managed-moves.tsv" >"$MOVE_AMBIGUOUS_FILE"
}

# A single provider gesture can appear as both the removal and addition halves
# of a sync plan. Collapse matching evidence before an editable proposal is
# created. Never guess when one track appears to target different destinations.
normalize_automatic_moves() {
    normalized="$WORK_DIR/automatic-moves-normalized.tsv"
    if ! awk -F '\t' 'BEGIN { OFS = "\t" }
        NF == 5 {
            spotify_id = $3
            destination = $5
            pair = spotify_id SUBSEP destination
            if (!(spotify_id in first_destination)) {
                first_destination[spotify_id] = destination
            } else if (first_destination[spotify_id] != destination) {
                conflicts[spotify_id] = first_destination[spotify_id] " / " destination
            }
            if (!(pair in row) || (row[pair] ~ /\tNew intake\t/ && $4 != "New intake")) {
                row[pair] = $0
            }
        }
        END {
            failed = 0
            for (spotify_id in conflicts) {
                print "Conflicting inferred destinations for " spotify_id ": " conflicts[spotify_id] > "/dev/stderr"
                failed = 1
            }
            if (failed) exit 2
            for (pair in row) print row[pair]
        }
    ' "$AUTO_MOVES_FILE" >"$normalized"; then
        printf 'Stopped before changing Chordrift: observed moves do not have one unambiguous destination.\n' >&2
        exit 3
    fi
    LC_ALL=C sort -t "$(printf '\t')" -k5,5 -k1,1 -k3,3 "$normalized" >"$AUTO_MOVES_FILE"
}

find_provider_order_drift() {
    awk -F '\t' '$1 ~ /^[0-9]+$/ && $2 == "publish" && $3 == "reorder_playlist" {
        print $4
    }' "$DETAIL_FILE" | sort -u >"$ORDER_DRIFT_FILE"
}

ensure_editable_proposal() {
    run proposals status --account "$ACCOUNT" >"$STATUS_FILE"
    case "$(field proposal "$STATUS_FILE")" in
        proposed) ;;
        approved)
            printf 'Preparing an editable copy of the current playlist model…\n'
            run proposals extend --account "$ACCOUNT" --min-similarity 1 >/dev/null
            ;;
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

align_provider_orders() {
    while IFS= read -r destination; do
        stable_key=$(resolve_destination "$destination") || {
            printf 'Provider-order destination "%s" is no longer unique.\n' "$destination" >&2
            exit 3
        }
        printf 'Accepting current Spotify order: %s\n' "$destination"
        run proposals align-provider-order --account "$ACCOUNT" \
            --playlist "$stable_key" >/dev/null
    done <"$ORDER_DRIFT_FILE"
}

finalize_current_proposal() {
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
        rm -f -- "$ARTWORK_REVIEW_MANIFEST"
        ARTWORK_REVIEW_MANIFEST=
    fi
}

resolve_ambiguous() {
    [ -s "$AMBIGUOUS_FILE" ] || [ -s "$AUTO_MOVES_FILE" ] || [ -s "$ORDER_DRIFT_FILE" ] || \
        [ -s "$LIKED_DECISIONS_FILE" ] || return 0
    [ "$REVIEW_ONLY" = false ] || return 0
    resolve_saved_intake
    [ -s "$AMBIGUOUS_FILE" ] || [ -s "$AUTO_MOVES_FILE" ] || [ -s "$ORDER_DRIFT_FILE" ] || return 0
    ensure_editable_proposal
    : >"$RESOLVED_AUTO_MOVES_FILE"
    while IFS="$(printf '\t')" read -r title artists spotify_id old_destination destination; do
        stable_key=$(resolve_destination "$destination") || {
            printf 'Inferred destination "%s" is no longer unique.\n' "$destination" >&2
            exit 3
        }
        printf '%s\t%s\t%s\t%s\t%s\t%s\n' \
            "$stable_key" "$title" "$artists" "$spotify_id" "$old_destination" "$destination" \
            >>"$RESOLVED_AUTO_MOVES_FILE"
        if [ "$old_destination" = "New intake" ]; then
            printf 'Detected direct intake: %s — %s → %s\n' \
                "$title" "$artists" "$destination"
        else
            printf 'Detected move: %s — %s · %s → %s\n' \
                "$title" "$artists" "$old_destination" "$destination"
        fi
    done <"$AUTO_MOVES_FILE"
    move_count=$(wc -l <"$RESOLVED_AUTO_MOVES_FILE" | tr -d ' ')
    if [ "$move_count" -gt 0 ]; then
        printf 'Recording %s inferred move(s) in Chordrift…\n' "$move_count"
        cut -f1 "$RESOLVED_AUTO_MOVES_FILE" | sort -u | while IFS= read -r stable_key; do
            set -- proposals assign --account "$ACCOUNT"
            while IFS="$(printf '\t')" read -r row_key _title _artists spotify_id _old _destination; do
                [ "$row_key" != "$stable_key" ] || set -- "$@" --spotify-id "$spotify_id"
            done <"$RESOLVED_AUTO_MOVES_FILE"
            set -- "$@" --playlist "$stable_key" --reason "Inferred from direct provider move"
            run "$@" >/dev/null
        done
        printf 'Recorded %s move(s) in Chordrift.\n' "$move_count"
    fi
    align_provider_orders
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

    finalize_current_proposal
}

if [ "$SKIP_PULL" = false ]; then
    printf 'Observing Spotify changes…\n'
    run sync pull --account "$ACCOUNT" >/dev/null
fi

printf 'Analyzing observed changes…\n'
create_plan
find_managed_moves
find_provider_order_drift
find_ambiguous
normalize_automatic_moves
if [ -s "$AMBIGUOUS_FILE" ] || [ -s "$AUTO_MOVES_FILE" ] || [ -s "$ORDER_DRIFT_FILE" ] || \
    [ -s "$LIKED_DECISIONS_FILE" ]; then
    if [ "$REVIEW_ONLY" = true ]; then
        if [ -s "$AUTO_MOVES_FILE" ]; then
            printf 'Inferred moves:\n'
            awk -F '\t' '{ print $1 " — " $2 " · " $4 " → " $5 }' "$AUTO_MOVES_FILE"
        fi
        if [ -s "$AMBIGUOUS_FILE" ]; then
            printf 'Needs a destination:\n'
            cut -f2-3 "$AMBIGUOUS_FILE"
        fi
        if [ -s "$ORDER_DRIFT_FILE" ]; then
            printf 'Provider order to accept:\n'
            sed 's/^/  /' "$ORDER_DRIFT_FILE"
        fi
        if [ -s "$LIKED_DECISIONS_FILE" ]; then
            printf 'Liked tracks already placed; choose whether Likes should remain:\n'
            awk -F '\t' '{ print "  " $1 " — " $2 " · already in " $4 }' "$LIKED_DECISIONS_FILE"
        fi
        printf 'Review only; Spotify unchanged.\n'
        exit 0
    fi
    resolve_ambiguous
    create_plan
fi

# One observed gesture can reveal another record-only delta after the proposal
# revision is approved. Keep absorbing exact membership-equal provider order
# until the maintenance plan stabilizes. This loop performs Neon intent writes
# only; align-provider-order refuses membership changes and no sync apply occurs.
ORDER_ALIGNMENT_PASSES=0
while true; do
    find_provider_order_drift
    [ -s "$ORDER_DRIFT_FILE" ] || break
    ORDER_ALIGNMENT_PASSES=$((ORDER_ALIGNMENT_PASSES + 1))
    [ "$ORDER_ALIGNMENT_PASSES" -le 4 ] || {
        printf 'Stopped: provider-order intent did not stabilize after 4 revisions; Spotify was not changed.\n' >&2
        exit 3
    }
    ensure_editable_proposal
    align_provider_orders
    finalize_current_proposal
    create_plan
done

UNSAFE=$(awk -F '\t' '$1 ~ /^[0-9]+$/ &&
    $3 !~ /^(add_track|remove_track|exclude_track|restore_track|remove_saved_track)$/ &&
    !($2 == "retirement" && $3 == "archive_playlist" &&
      $7 ~ /"surface":"retired_reevaluate"/ && $8 ~ /"queue_empty":true/) { print }' "$DETAIL_FILE")
[ -z "$UNSAFE" ] || {
    printf 'Stopped: this change includes work outside ordinary maintenance:\n%s\n' "$UNSAFE" >&2
    exit 3
}

OPERATIONS=$(awk -F '\t' '$1 ~ /^[0-9]+$/ && $2 != "retirement" { count++ } END { print count + 0 }' "$DETAIL_FILE")
[ "$OPERATIONS" -gt 0 ] || {
    # Record-only convergence is complete only after the exact ordered provider
    # state is durable as the next comparison baseline. This is a Neon-only
    # checkpoint and cannot write Spotify.
    [ "$REVIEW_ONLY" = true ] || run sync accept-current --account "$ACCOUNT" >/dev/null
    if awk -F '\t' '$1 ~ /^[0-9]+$/ && $2 == "retirement" { found = 1 } END { exit !found }' "$DETAIL_FILE"; then
        printf 'Ordinary maintenance is in sync. A separately reviewed retirement remains pending.\n'
    else
        printf 'Everything is already in sync.\n'
    fi
    exit 0
}

printf '\nChordrift will make these Spotify changes:\n'
awk -F '\t' '$1 ~ /^[0-9]+$/ && $2 != "retirement" { print $3 "\t" $4 "\t" $9 "\t" $10 }' \
    "$DETAIL_FILE" | while IFS="$(printf '\t')" read -r action playlist title artists; do
    if [ -n "$title" ] && [ "$title" != - ]; then
        label=$title
        [ -z "$artists" ] || [ "$artists" = - ] || label="$label — $artists"
    else
        label=Track
    fi
    case "$action" in
        add_track) printf '  Add: %s → %s\n' "$label" "$playlist" ;;
        restore_track) printf '  Restore: %s → %s\n' "$label" "$playlist" ;;
        remove_track) printf '  Remove: %s from %s\n' "$label" "$playlist" ;;
        exclude_track) printf '  Remember removal: %s from %s\n' "$label" "$playlist" ;;
        remove_saved_track) printf '  Remove from Likes: %s\n' "$label" ;;
        *) printf '  %s: %s\n' "$action" "$playlist" ;;
    esac
done

[ "$REVIEW_ONLY" = false ] || { printf 'Review only; Spotify unchanged.\n'; exit 0; }
if [ -n "$CONFIRMED_PLAN" ]; then
    [ "$CONFIRMED_PLAN" = "$PLAN_ID" ] || {
        printf 'The confirmed maintenance plan changed; nothing was applied.\n' >&2
        exit 3
    }
else
    printf '\nApply these changes? [y/N] ' >/dev/tty
    IFS= read -r answer </dev/tty
    case "$answer" in y|Y|yes|YES|Yes) ;; *) printf 'Cancelled; Spotify unchanged.\n'; exit 0 ;; esac
fi

phase=$(awk -F '\t' '$1 ~ /^[0-9]+$/ && $2 != "retirement" { print $2; exit }' "$DETAIL_FILE")
case "$phase" in publish|reconcile|cleanup) ;; *) printf 'Stopped before unsupported phase %s.\n' "$phase" >&2; exit 3 ;; esac
"$SCRIPT_DIR/chordrift-plan-phase.sh" --account "$ACCOUNT" --plan "$PLAN_ID" \
    --phase "$phase" --workflow-confirmation "$PLAN_ID" --concise

printf 'Done with the confirmed changes. Run maintenance again to review any newly observed work.\n'
