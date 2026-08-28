#!/bin/sh

# Apply one reviewed publish or reconcile phase through Chordrift's complete
# readiness, exact-confirmation, pull, receipt, and next-plan workflow.

set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)
ACCOUNT=personal
PLAN_ID=
PHASE=

usage() {
    printf '%s\n' \
        "Usage: scripts/chordrift-plan-phase.sh --plan PLAN_UUID --phase publish|reconcile [--account LABEL]" \
        "" \
        "Reviews and applies exactly one current publish or reconcile phase." \
        "It refuses cleanup, retirement, stale plans, failed readiness, and" \
        "reconcile while an earlier publish phase remains in the same plan." \
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
        --plan)
            [ "$#" -ge 2 ] || { usage >&2; exit 2; }
            PLAN_ID=$2
            shift 2
            ;;
        --phase)
            [ "$#" -ge 2 ] || { usage >&2; exit 2; }
            PHASE=$2
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
[ -n "$PLAN_ID" ] || { printf '%s\n' '--plan is required.' >&2; exit 2; }
case "$PHASE" in
    publish|reconcile) ;;
    cleanup|retirement)
        printf 'This helper refuses destructive phase %s; use the manual reviewed workflow.\n' \
            "$PHASE" >&2
        exit 2
        ;;
    *)
        printf '%s\n' '--phase must be publish or reconcile.' >&2
        exit 2
        ;;
esac

cd "$REPO_ROOT"
CHORDRIFT_BIN=${CHORDRIFT_BIN:-chordrift}
if ! command -v "$CHORDRIFT_BIN" >/dev/null 2>&1; then
    printf "Chordrift is not installed or executable as '%s'.\n" "$CHORDRIFT_BIN" >&2
    exit 127
fi

WORK_DIR=$(mktemp -d "${TMPDIR:-/tmp}/chordrift-plan-phase.XXXXXX")
DETAIL_FILE=$WORK_DIR/details.txt
READINESS_FILE=$WORK_DIR/readiness.txt
APPLY_FILE=$WORK_DIR/apply.txt
VERIFY_FILE=$WORK_DIR/verify.txt
NEXT_PLAN_FILE=$WORK_DIR/next-plan.txt

cleanup() {
    rm -f "$DETAIL_FILE" "$READINESS_FILE" "$APPLY_FILE" "$VERIFY_FILE" "$NEXT_PLAN_FILE"
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

stage "Review" "inspect exact plan $PLAN_ID"
run_chordrift sync plan-show --account "$ACCOUNT" --plan "$PLAN_ID" --details >"$DETAIL_FILE"
PARSED_PLAN_ID=$(field plan_id "$DETAIL_FILE")
SNAPSHOT_CURRENT=$(field snapshot_current "$DETAIL_FILE")
[ "$PARSED_PLAN_ID" = "$PLAN_ID" ] || {
    printf 'Chordrift returned a different plan identity. No apply was attempted.\n' >&2
    exit 1
}
[ "$SNAPSHOT_CURRENT" = true ] || {
    printf 'Plan %s is stale. Pull and create a new plan; no apply was attempted.\n' "$PLAN_ID" >&2
    exit 1
}

PLAN_PHASES=$(awk -F '\t' '
    $1 == "sequence" { in_operations = 1; next }
    in_operations && $1 ~ /^[0-9]+$/ { print $2 }
' "$DETAIL_FILE" | sort -u)
if ! printf '%s\n' "$PLAN_PHASES" | grep -Fx "$PHASE" >/dev/null 2>&1; then
    printf 'Plan %s contains no %s phase. No apply was attempted.\n' "$PLAN_ID" "$PHASE" >&2
    exit 1
fi
if [ "$PHASE" = reconcile ] && printf '%s\n' "$PLAN_PHASES" | grep -Fx publish >/dev/null 2>&1; then
    printf 'This plan still contains an earlier publish phase. Apply and verify publish first.\n' >&2
    exit 1
fi
run_chordrift sync plan-show --account "$ACCOUNT" --plan "$PLAN_ID" --details

if [ "$PHASE" = publish ]; then
    stage "Preflight" "validate publish artifacts and request estimates"
    run_chordrift sync apply-preflight --account "$ACCOUNT" --plan "$PLAN_ID"
fi

stage "Readiness" "assess exact plan and probe Spotify read-only"
run_chordrift sync readiness --account "$ACCOUNT" --plan "$PLAN_ID" --probe >"$READINESS_FILE"
ASSESSMENT_ID=$(field assessment_id "$READINESS_FILE")
READINESS_STATUS=$(field apply_readiness "$READINESS_FILE" | sed 's/ (already current)$//')
[ "$READINESS_STATUS" = ready ] && [ -n "$ASSESSMENT_ID" ] || {
    run_chordrift sync readiness-show --account "$ACCOUNT"
    printf 'Readiness did not pass. No apply was attempted.\n' >&2
    exit 1
}
run_chordrift sync readiness-show --account "$ACCOUNT" --assessment "$ASSESSMENT_ID"

printf '\nPlan %s is ready for phase %s.\n' "$PLAN_ID" "$PHASE"
printf 'Type the assessment UUID %s to authorize this one phase: ' "$ASSESSMENT_ID" >/dev/tty
IFS= read -r CONFIRMATION </dev/tty
[ "$CONFIRMATION" = "$ASSESSMENT_ID" ] || {
    printf 'Confirmation did not match. No apply was attempted.\n' >&2
    exit 1
}

stage "Apply" "execute only the exact confirmed $PHASE phase"
run_chordrift sync apply \
    --account "$ACCOUNT" \
    --assessment "$ASSESSMENT_ID" \
    --phase "$PHASE" \
    --confirm "$ASSESSMENT_ID" >"$APPLY_FILE"
APPLY_RUN_ID=$(field apply_run_id "$APPLY_FILE")
[ -n "$APPLY_RUN_ID" ] || {
    printf 'Apply returned no run ID. Stop and inspect manually.\n' >&2
    exit 1
}
run_chordrift sync apply-show --account "$ACCOUNT" --run "$APPLY_RUN_ID"

stage "Verify" "pull provider state and verify the exact receipt"
VERIFY_ATTEMPT=1
MAX_VERIFY_ATTEMPTS=4
while [ "$VERIFY_ATTEMPT" -le "$MAX_VERIFY_ATTEMPTS" ]; do
    run_chordrift sync pull --account "$ACCOUNT"
    run_chordrift sync apply-show --account "$ACCOUNT" --run "$APPLY_RUN_ID" \
        >"$VERIFY_FILE"
    run_chordrift sync apply-show --account "$ACCOUNT" --run "$APPLY_RUN_ID"
    APPLY_STATUS=$(field spotify_apply "$VERIFY_FILE" | sed 's/ (already current)$//')
    case "$APPLY_STATUS" in
        succeeded)
            break
            ;;
        awaiting_pull)
            if [ "$VERIFY_ATTEMPT" -ge "$MAX_VERIFY_ATTEMPTS" ]; then
                printf 'Spotify accepted the phase, but Chordrift could not observe it after %s pulls.\n' \
                    "$MAX_VERIFY_ATTEMPTS" >&2
                printf 'The receipt remains awaiting verification; no later phase was attempted.\n' >&2
                exit 3
            fi
            stage "Verification retry $((VERIFY_ATTEMPT + 1))/$MAX_VERIFY_ATTEMPTS" \
                "wait briefly for Spotify playlist observation"
            sleep 2
            ;;
        *)
            printf 'Apply receipt entered unexpected state %s; no later phase was attempted.\n' \
                "${APPLY_STATUS:-unknown}" >&2
            exit 3
            ;;
    esac
    VERIFY_ATTEMPT=$((VERIFY_ATTEMPT + 1))
done

stage "Next plan" "build from the newly observed provider snapshot"
run_chordrift sync plan --account "$ACCOUNT" >"$NEXT_PLAN_FILE"
NEXT_PLAN_ID=$(field plan_id "$NEXT_PLAN_FILE")
[ -n "$NEXT_PLAN_ID" ] || {
    printf 'Could not parse the next plan. Stop and inspect manually.\n' >&2
    exit 1
}
run_chordrift sync plan-show --account "$ACCOUNT" --plan "$NEXT_PLAN_ID" --details
printf '\nCompleted phase %s. Review the new plan before any later phase.\n' "$PHASE"
