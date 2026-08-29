#!/bin/sh

# Interactive operator assistant for resolving the provider-native Re-evaluate
# queue into existing canonical playlists. Domain decisions and persistence
# remain in the installed Chordrift binary. The script fails closed on
# unrelated proposal or provider work and requires exact confirmations before
# approval or Spotify cleanup.

set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)
. "$SCRIPT_DIR/chordrift-reevaluate-plan-audit.lib.sh"
ACCOUNT=personal
SKIP_PULL=false
REVIEW_ONLY=false
RESUME=false
ARTWORK_MANIFEST=$REPO_ROOT/artwork/canonical/drift-atlas-v4/manifest.json
ARTWORK_REVIEW_MANIFEST=

usage() {
    printf '%s\n' \
        "Usage: scripts/chordrift-reevaluate-wizard.sh [--account LABEL] [--skip-pull]" \
        "       scripts/chordrift-reevaluate-wizard.sh [--account LABEL] --review-only" \
        "       scripts/chordrift-reevaluate-wizard.sh [--account LABEL] --resume" \
        "       [--artwork-manifest PATH]" \
        "" \
        "Reviews current Re-evaluate tracks, records explicit replacement" \
        "destinations in an editable proposal, publishes only selected" \
        "placements, verifies them, and separately confirms removal from the" \
        "holding queue. Deferral leaves a track untouched." \
        "" \
        "The wizard refuses new playlists, retirement, unrelated unresolved" \
        "tracks, non-maintenance plans, and unexpected provider operations." \
        "" \
        "Environment:" \
        "  CHORDRIFT_BIN  Installed Chordrift executable; defaults to chordrift."
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --account)
            [ "$#" -ge 2 ] || { usage >&2; exit 2; }
            ACCOUNT=$2
            shift 2
            ;;
        --skip-pull)
            SKIP_PULL=true
            shift
            ;;
        --review-only)
            REVIEW_ONLY=true
            shift
            ;;
        --resume)
            RESUME=true
            shift
            ;;
        --artwork-manifest)
            [ "$#" -ge 2 ] || { usage >&2; exit 2; }
            ARTWORK_MANIFEST=$2
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
    ''|*[!A-Za-z0-9_-]*)
        printf 'Account labels may contain only letters, numbers, - and _.\n' >&2
        exit 2
        ;;
esac
[ "$REVIEW_ONLY" = true ] || { [ -t 0 ] && [ -t 1 ]; } || {
    printf 'This review wizard requires an interactive terminal.\n' >&2
    exit 2
}

CHORDRIFT_BIN=${CHORDRIFT_BIN:-chordrift}
command -v "$CHORDRIFT_BIN" >/dev/null 2>&1 || {
    printf "Chordrift is not installed or executable as '%s'.\n" "$CHORDRIFT_BIN" >&2
    exit 127
}
for helper in chordrift-require-capabilities.sh chordrift-manual-place.sh chordrift-plan-phase.sh; do
    [ -x "$SCRIPT_DIR/$helper" ] || {
        printf 'Missing executable helper: %s/%s\n' "$SCRIPT_DIR" "$helper" >&2
        exit 1
    }
done
"$SCRIPT_DIR/chordrift-require-capabilities.sh" "$CHORDRIFT_BIN" \
    --require maintenance.intake-workflow.v1 \
    --require maintenance.enumerated-playlist-additions.v1 \
    --require plan-origin.v1

WORK_DIR=$(mktemp -d "${TMPDIR:-/tmp}/chordrift-reevaluate-wizard.XXXXXX")
QUEUE_FILE=$WORK_DIR/queue.tsv
QUEUE_IDS_FILE=$WORK_DIR/queue-ids.txt
SELECTED_IDS_FILE=$WORK_DIR/selected-ids.txt
STATUS_FILE=$WORK_DIR/status.txt
UNRESOLVED_FILE=$WORK_DIR/unresolved.tsv
PLAN_FILE=$WORK_DIR/plan.txt
PLAN_DETAILS_FILE=$WORK_DIR/plan-details.tsv
ARTWORK_STATUS_FILE=$WORK_DIR/artwork-status.txt
READINESS_FILE=$WORK_DIR/readiness.txt
APPLY_FILE=$WORK_DIR/apply.txt
FINAL_QUEUE_FILE=$WORK_DIR/final-queue.tsv
: >"$SELECTED_IDS_FILE"

cleanup() {
    if [ -n "$ARTWORK_REVIEW_MANIFEST" ] && [ -f "$ARTWORK_REVIEW_MANIFEST" ]; then
        rm -f -- "$ARTWORK_REVIEW_MANIFEST"
    fi
    case "$WORK_DIR" in
        "${TMPDIR:-/tmp}"/chordrift-reevaluate-wizard.*) rm -rf -- "$WORK_DIR" ;;
    esac
}
trap cleanup EXIT HUP INT TERM

cd "$REPO_ROOT"
run_chordrift() { "$CHORDRIFT_BIN" "$@"; }
field() { sed -n "s/^$1: //p" "$2" | tail -n 1; }

stage() {
    printf '\n\033[1;36m%s\033[0m  \033[2m— %s\033[0m\n' "$1" "$2"
}

ask_yes() {
    printf '%s [y/N] ' "$1" >/dev/tty
    IFS= read -r answer </dev/tty
    case "$answer" in y|Y|yes|YES|Yes) return 0 ;; *) return 1 ;; esac
}

ask_value() {
    prompt=$1
    printf '%s' "$prompt" >/dev/tty
    IFS= read -r value </dev/tty
    printf '%s' "$value"
}

require_exact() {
    label=$1
    expected=$2
    while true; do
        entered=$(ask_value "Type the exact $label $expected (or 'cancel'): ")
        if [ "$entered" = "$expected" ]; then
            return 0
        fi
        case "$entered" in
            cancel|CANCEL|Cancel)
                printf 'Cancelled. Nothing after this gate was changed.\n'
                exit 0
                ;;
        esac
        printf 'Confirmation did not match; please copy the complete value and try again.\n' >&2
    done
}

capture_queue() {
    run_chordrift playlists tracks --account "$ACCOUNT" --name Re-evaluate >"$1"
    awk -F '\t' '$1 ~ /^[0-9]+$/ && $5 != "" { print $5 }' "$1" | sort -u >"$2"
}

create_plan() {
    run_chordrift sync plan --account "$ACCOUNT" >"$PLAN_FILE"
    PLAN_ID=$(field plan_id "$PLAN_FILE")
    OPERATIONS=$(field operations "$PLAN_FILE")
    [ -n "$PLAN_ID" ] && [ -n "$OPERATIONS" ] || {
        printf 'Could not parse the current plan. Stop and inspect manually.\n' >&2
        exit 1
    }
    run_chordrift sync plan-show \
        --account "$ACCOUNT" --plan "$PLAN_ID" --details >"$PLAN_DETAILS_FILE"
    PLAN_ORIGIN=$(field plan_origin "$PLAN_DETAILS_FILE")
    [ "$PLAN_ORIGIN" = maintenance ] || {
        printf 'Re-evaluate maintenance refuses plan origin %s. No apply was attempted.\n' \
            "${PLAN_ORIGIN:-unknown}" >&2
        exit 3
    }
    [ "$(field snapshot_current "$PLAN_DETAILS_FILE")" = true ] || {
        printf 'Plan %s is stale. Run the wizard again after a fresh pull.\n' "$PLAN_ID" >&2
        exit 1
    }
}

phase_count() {
    awk -F '\t' -v phase="$1" '
        $1 == "sequence" { operations = 1; next }
        operations && $1 ~ /^[0-9]+$/ && $2 == phase { count += 1 }
        END { print count + 0 }
    ' "$PLAN_DETAILS_FILE"
}

unexpected_publish_operations() {
    awk -F '\t' '
        FILENAME == ARGV[1] { selected[$1] = 1; next }
        $1 == "sequence" { operations = 1; next }
        operations && $1 ~ /^[0-9]+$/ && $2 == "publish" {
            if (($3 == "add_track" || $3 == "restore_track") && ($6 in selected)) next
            print
        }
    ' "$SELECTED_IDS_FILE" "$PLAN_DETAILS_FILE"
}

unexpected_cleanup_operations() {
    awk -F '\t' '
        FILENAME == ARGV[1] { selected[$1] = 1; next }
        $1 == "sequence" { operations = 1; next }
        operations && $1 ~ /^[0-9]+$/ && $2 == "cleanup" {
            if ($3 == "remove_track" && $4 == "Re-evaluate" && ($6 in selected)) next
            print
        }
    ' "$SELECTED_IDS_FILE" "$PLAN_DETAILS_FILE"
}

if [ "$SKIP_PULL" = false ]; then
    stage "Observe" "fresh Spotify pull before reviewing the holding queue"
    run_chordrift sync pull --account "$ACCOUNT"
else
    stage "Observe" "using the operator-confirmed current snapshot"
fi

stage "Re-evaluate" "current provider queue"
run_chordrift reevaluate status --account "$ACCOUNT"
capture_queue "$QUEUE_FILE" "$QUEUE_IDS_FILE"
cat "$QUEUE_FILE"
QUEUE_COUNT=$(wc -l <"$QUEUE_IDS_FILE" | tr -d ' ')
[ "$QUEUE_COUNT" -gt 0 ] || {
    printf '\nThe Re-evaluate queue is empty. Nothing to review.\n'
    exit 0
}
if [ "$REVIEW_ONLY" = true ]; then
    printf '\nReview complete. No placement, approval, or Spotify write was attempted.\n'
    exit 0
fi

if [ "$RESUME" = true ]; then
    stage "Resume" "reuse the already-approved Re-evaluate correction proposal"
    cp "$QUEUE_IDS_FILE" "$SELECTED_IDS_FILE"
    SELECTED_COUNT=$QUEUE_COUNT
    run_chordrift proposals status --account "$ACCOUNT" >"$STATUS_FILE"
    PROPOSAL_ID=$(field generation_id "$STATUS_FILE")
    [ "$(field proposal "$STATUS_FILE")" = approved ] && [ -n "$PROPOSAL_ID" ] || {
        cat "$STATUS_FILE" >&2
        printf -- '--resume requires the already-approved correction proposal.\n' >&2
        exit 3
    }
    cat "$STATUS_FILE"
else
    stage "Editable proposal" "preserve the approved library before recording corrections"
    run_chordrift proposals status --account "$ACCOUNT" >"$STATUS_FILE"
    case "$(field proposal "$STATUS_FILE")" in
        proposed) ;;
        approved)
            ask_yes "Prepare an editable copy of the approved library for Re-evaluate corrections?" || exit 0
            run_chordrift proposals extend --account "$ACCOUNT" --min-similarity 1
            ;;
        *)
            printf 'The latest proposal is not editable or approved. No correction was recorded.\n' >&2
            exit 1
            ;;
    esac
    run_chordrift proposals status --account "$ACCOUNT"
    run_chordrift proposals list --account "$ACCOUNT"

    stage "Placement review" "choose an existing destination or defer each track"
    while IFS="$(printf '\t')" read -r position title artists album spotify_id; do
        case "$position" in ''|*[!0-9]*) continue ;; esac
        printf '\n%s — %s\nAlbum: %s\nSpotify ID: %s\n' "$title" "$artists" "$album" "$spotify_id"
        printf 'Choose [m]ove to an existing destination, [d]efer, [q] stop: ' >/dev/tty
        IFS= read -r choice </dev/tty
        case "$choice" in
            m|M)
                destination=$(ask_value 'Exact destination display name: ')
                [ -n "$destination" ] || { printf 'Destination is required.\n' >&2; exit 2; }
                reason=$(ask_value 'Reason for this correction: ')
                [ -n "$reason" ] || reason="Reviewed Re-evaluate correction"
                "$SCRIPT_DIR/chordrift-manual-place.sh" \
                    --account "$ACCOUNT" --to "$destination" \
                    --spotify-id "$spotify_id" --reason "$reason"
                printf '%s\n' "$spotify_id" >>"$SELECTED_IDS_FILE"
                ;;
            d|D)
                printf 'Deferred; the track remains in Re-evaluate.\n'
                ;;
            *)
                printf 'Stopped with existing edits unapproved and Spotify unchanged.\n'
                exit 0
                ;;
        esac
    done <"$QUEUE_FILE"

    sort -u "$SELECTED_IDS_FILE" -o "$SELECTED_IDS_FILE"
    SELECTED_COUNT=$(wc -l <"$SELECTED_IDS_FILE" | tr -d ' ')
    [ "$SELECTED_COUNT" -gt 0 ] || {
        printf 'No replacement destinations were selected. Spotify remains unchanged.\n'
        exit 0
    }

    stage "Proposal review" "approve only complete, unrelated-work-free intent"
    run_chordrift proposals unresolved --account "$ACCOUNT" --limit 10000 >"$UNRESOLVED_FILE"
    UNRESOLVED_COUNT=$(awk 'NR > 1 { count += 1 } END { print count + 0 }' "$UNRESOLVED_FILE")
    [ "$UNRESOLVED_COUNT" -eq 0 ] || {
        printf 'The editable proposal contains %s unresolved track(s). No approval was attempted.\n' \
            "$UNRESOLVED_COUNT" >&2
        cat "$UNRESOLVED_FILE" >&2
        exit 3
    }
    run_chordrift proposals status --account "$ACCOUNT" >"$STATUS_FILE"
    PROPOSAL_ID=$(field generation_id "$STATUS_FILE")
    [ "$(field proposal "$STATUS_FILE")" = proposed ] &&
        [ "$(field coverage_complete "$STATUS_FILE")" = true ] || {
            cat "$STATUS_FILE" >&2
            printf 'Proposal is not a complete editable generation. No approval was attempted.\n' >&2
            exit 3
        }
    cat "$STATUS_FILE"
    ask_yes "Approve this complete proposal containing the reviewed corrections?" || exit 0
    require_exact "proposal generation ID" "$PROPOSAL_ID"
    run_chordrift proposals approve --account "$ACCOUNT" --confirm "$PROPOSAL_ID"
fi

stage "Existing artwork" "reuse only the unchanged reviewed visual system"
ARTWORK_STATE=missing
if run_chordrift artwork status --account "$ACCOUNT" >"$ARTWORK_STATUS_FILE" 2>/dev/null &&
   [ "$(field proposal_generation_id "$ARTWORK_STATUS_FILE")" = "$PROPOSAL_ID" ]; then
    ARTWORK_STATE=$(field artwork "$ARTWORK_STATUS_FILE")
fi
if [ "$ARTWORK_STATE" = pending ]; then
    PENDING_CONTACT_SHEET=$(field contact_sheet "$ARTWORK_STATUS_FILE")
    if [ -z "$PENDING_CONTACT_SHEET" ] || [ ! -f "$PENDING_CONTACT_SHEET" ]; then
        printf 'The pending artwork batch references an expired temporary review copy.\n'
        printf 'A new review will use the persistent original artwork files.\n'
        ARTWORK_STATE=missing
    fi
fi
case "$ARTWORK_STATE" in
approved)
    cat "$ARTWORK_STATUS_FILE"
    ;;
pending)
    cat "$ARTWORK_STATUS_FILE"
    BATCH_ID=$(field batch_id "$ARTWORK_STATUS_FILE")
    [ -n "$BATCH_ID" ] || {
        printf 'The pending artwork review has no batch ID. No provider write occurred.\n' >&2
        exit 3
    }
    require_exact "artwork batch ID" "$BATCH_ID"
    run_chordrift artwork approve --account "$ACCOUNT" --confirm "$BATCH_ID"
    ;;
missing)
    [ -f "$ARTWORK_MANIFEST" ] || {
        printf 'No reusable artwork manifest exists at %s. No Spotify write occurred.\n' \
            "$ARTWORK_MANIFEST" >&2
        exit 3
    }
    ask_yes "Reuse the existing reviewed artwork files unchanged for this proposal generation?" || exit 0
    ARTWORK_SOURCE_DIR=$(CDPATH= cd -- "$(dirname -- "$ARTWORK_MANIFEST")" && pwd)
    ARTWORK_REVIEW_MANIFEST=$(mktemp "$ARTWORK_SOURCE_DIR/.chordrift-manifest.XXXXXX")
    sed -E "s/(\"proposal_generation_id\"[[:space:]]*:[[:space:]]*\")[^\"]*(\")/\\1$PROPOSAL_ID\\2/" \
        "$ARTWORK_MANIFEST" >"$ARTWORK_REVIEW_MANIFEST"
    run_chordrift artwork import --account "$ACCOUNT" \
        --manifest "$ARTWORK_REVIEW_MANIFEST"
    run_chordrift artwork status --account "$ACCOUNT" >"$ARTWORK_STATUS_FILE"
    BATCH_ID=$(field batch_id "$ARTWORK_STATUS_FILE")
    cat "$ARTWORK_STATUS_FILE"
    require_exact "artwork batch ID" "$BATCH_ID"
    run_chordrift artwork approve --account "$ACCOUNT" --confirm "$BATCH_ID"
    ;;
*)
    cat "$ARTWORK_STATUS_FILE" >&2
    printf 'Artwork is in unsupported state %s. No provider write occurred.\n' "$ARTWORK_STATE" >&2
    exit 3
    ;;
esac

ask_yes "Publish the selected destinations, verify them, and then review exact Re-evaluate cleanup?" || {
    printf 'Stopped before provider writes. Approved Neon intent remains available.\n'
    exit 0
}

stage "Converge" "publish destinations, reconcile old placements, then clean up the holding queue"
iterations=0
while [ "$iterations" -lt 16 ]; do
    iterations=$((iterations + 1))
    create_plan
    [ "$OPERATIONS" -gt 0 ] || break
    run_chordrift sync plan-show --account "$ACCOUNT" --plan "$PLAN_ID" --details

    PUBLISH_COUNT=$(phase_count publish)
    CLEANUP_COUNT=$(phase_count cleanup)
    RECONCILE_COUNT=$(phase_count reconcile)
    RETIREMENT_COUNT=$(phase_count retirement)
    [ "$RETIREMENT_COUNT" -eq 0 ] || {
        printf 'The plan contains retirement work. No apply was attempted.\n' >&2
        exit 3
    }

    if [ "$PUBLISH_COUNT" -gt 0 ]; then
        UNEXPECTED=$(unexpected_publish_operations)
        [ -z "$UNEXPECTED" ] || {
            printf 'The publish phase contains unexpected work. No apply was attempted:\n%s\n' \
                "$UNEXPECTED" >&2
            exit 3
        }
        "$SCRIPT_DIR/chordrift-plan-phase.sh" \
            --account "$ACCOUNT" --plan "$PLAN_ID" --phase publish
        continue
    fi

    if [ "$RECONCILE_COUNT" -gt 0 ]; then
        UNEXPECTED=$(reevaluate_unexpected_reconcile_operations \
            "$SELECTED_IDS_FILE" "$PLAN_DETAILS_FILE")
        [ -z "$UNEXPECTED" ] || {
            printf 'The reconcile phase contains unexpected work. No apply was attempted:\n%s\n' \
                "$UNEXPECTED" >&2
            exit 3
        }
        "$SCRIPT_DIR/chordrift-plan-phase.sh" \
            --account "$ACCOUNT" --plan "$PLAN_ID" --phase reconcile
        continue
    fi

    if [ "$CLEANUP_COUNT" -gt 0 ]; then
        UNEXPECTED=$(unexpected_cleanup_operations)
        [ -z "$UNEXPECTED" ] || {
            printf 'The cleanup phase contains unexpected work. No apply was attempted:\n%s\n' \
                "$UNEXPECTED" >&2
            exit 3
        }
        stage "Destructive holding-queue cleanup" "remove only verified selected Re-evaluate memberships"
        run_chordrift sync readiness \
            --account "$ACCOUNT" --plan "$PLAN_ID" --probe >"$READINESS_FILE"
        ASSESSMENT_ID=$(field assessment_id "$READINESS_FILE")
        READINESS=$(field apply_readiness "$READINESS_FILE" | sed 's/ (already current)$//')
        [ "$READINESS" = ready ] && [ -n "$ASSESSMENT_ID" ] || {
            run_chordrift sync readiness-show --account "$ACCOUNT"
            printf 'Cleanup readiness is blocked. No cleanup was attempted.\n' >&2
            exit 3
        }
        run_chordrift sync readiness-show \
            --account "$ACCOUNT" --assessment "$ASSESSMENT_ID"
        require_exact "cleanup assessment ID" "$ASSESSMENT_ID"
        run_chordrift sync apply \
            --account "$ACCOUNT" --assessment "$ASSESSMENT_ID" \
            --phase cleanup --confirm "$ASSESSMENT_ID" --allow-destructive \
            >"$APPLY_FILE"
        APPLY_RUN_ID=$(field apply_run_id "$APPLY_FILE")
        [ -n "$APPLY_RUN_ID" ] || {
            printf 'Cleanup apply returned no run ID. Stop and inspect.\n' >&2
            exit 1
        }
        run_chordrift sync apply-show --account "$ACCOUNT" --run "$APPLY_RUN_ID"
        run_chordrift sync pull --account "$ACCOUNT"
        run_chordrift sync apply-show --account "$ACCOUNT" --run "$APPLY_RUN_ID"
        continue
    fi

    printf 'The plan has operations outside the supported Re-evaluate phases. No apply was attempted.\n' >&2
    exit 3
done
[ "$iterations" -lt 16 ] || {
    printf 'The Re-evaluate workflow did not converge within 16 plans. Stop and inspect.\n' >&2
    exit 3
}

capture_queue "$FINAL_QUEUE_FILE" "$WORK_DIR/final-queue-ids.txt"
STILL_PRESENT=$(awk '
    FILENAME == ARGV[1] { selected[$1] = 1; next }
    $1 in selected { print }
' "$SELECTED_IDS_FILE" "$WORK_DIR/final-queue-ids.txt")
[ -z "$STILL_PRESENT" ] || {
    printf 'Selected track(s) remain in Re-evaluate after verification:\n%s\n' \
        "$STILL_PRESENT" >&2
    exit 3
}

stage "Complete" "$SELECTED_COUNT reviewed track(s) published, verified, and removed from Re-evaluate"
cat "$FINAL_QUEUE_FILE"
