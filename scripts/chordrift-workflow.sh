#!/bin/sh

# Convenience wrapper for Chordrift's safe pull → plan → readiness → apply → verify loop.
# It deliberately preserves the Rust core's exact assessment confirmation and
# refuses cleanup/retirement phases, which require their own operator review.

set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)
ACCOUNT=personal
SKIP_INITIAL_PULL=false

usage() {
    printf '%s\n' \
        "Usage: scripts/chordrift-workflow.sh [--account LABEL] [--skip-initial-pull]" \
        "" \
        "Environment:" \
        "  CHORDRIFT_BIN  Optional executable name or exact path." \
        "                 Defaults to the installed 'chordrift' command." \
        "" \
        "The wrapper refuses cleanup, retirement, stale plans, failed readiness," \
        "and plans containing more than one apply phase."
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --account)
            [ "$#" -ge 2 ] || { usage >&2; exit 2; }
            ACCOUNT=$2
            shift 2
            ;;
        --skip-initial-pull)
            SKIP_INITIAL_PULL=true
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

WORK_DIR=$(mktemp -d "${TMPDIR:-/tmp}/chordrift-workflow.XXXXXX")
PLAN_FILE=$WORK_DIR/plan.txt
DETAIL_FILE=$WORK_DIR/details.txt
READINESS_FILE=$WORK_DIR/readiness.txt
APPLY_FILE=$WORK_DIR/apply.txt
FINAL_PLAN_FILE=$WORK_DIR/final-plan.txt

cleanup() {
    rm -f "$PLAN_FILE" "$DETAIL_FILE" "$READINESS_FILE" "$APPLY_FILE" "$FINAL_PLAN_FILE"
    rmdir "$WORK_DIR" 2>/dev/null || true
}
trap cleanup EXIT HUP INT TERM

if [ "$SKIP_INITIAL_PULL" = false ]; then
    stage "Observe" "pull current Spotify state into Neon"
    run_chordrift sync pull --account "$ACCOUNT"
fi

stage "Plan" "create an immutable provider-free plan"
run_chordrift sync plan --account "$ACCOUNT" >"$PLAN_FILE"
PLAN_ID=$(field plan_id "$PLAN_FILE")
OPERATIONS=$(field operations "$PLAN_FILE")
[ -n "$PLAN_ID" ] && [ -n "$OPERATIONS" ] || {
    printf 'Could not parse the generated plan. No apply was attempted.\n' >&2
    exit 1
}

run_chordrift sync plan-show --account "$ACCOUNT" --plan "$PLAN_ID" --details

if [ "$OPERATIONS" = 0 ]; then
    stage "Converged" "the current plan contains zero operations"
    exit 0
fi

run_chordrift sync plan-show --account "$ACCOUNT" --plan "$PLAN_ID" --details >"$DETAIL_FILE"
SNAPSHOT_CURRENT=$(field snapshot_current "$DETAIL_FILE")
[ "$SNAPSHOT_CURRENT" = true ] || {
    printf 'The plan became stale. No readiness assessment or apply was attempted.\n' >&2
    exit 1
}

PHASES=$(awk -F '\t' '
    $1 == "sequence" { in_operations = 1; next }
    in_operations && $1 ~ /^[0-9]+$/ { print $2 }
' "$DETAIL_FILE" | sort -u)
PHASE_COUNT=$(printf '%s\n' "$PHASES" | sed '/^$/d' | wc -l | tr -d ' ')
[ "$PHASE_COUNT" = 1 ] || {
    printf 'The plan spans %s phases. Review and apply each phase manually.\n' "$PHASE_COUNT" >&2
    exit 1
}
PHASE=$(printf '%s\n' "$PHASES" | sed -n '1p')
case "$PHASE" in
    publish|reconcile) ;;
    cleanup|retirement)
        printf 'The wrapper refuses destructive phase %s. Use the manual approval workflow.\n' "$PHASE" >&2
        exit 1
        ;;
    *)
        printf 'Unsupported or unrecognized apply phase: %s\n' "$PHASE" >&2
        exit 1
        ;;
esac

if [ "$PHASE" = publish ]; then
    stage "Preflight" "validate publish artifacts and request estimates"
    run_chordrift sync apply-preflight --account "$ACCOUNT" --plan "$PLAN_ID"
fi

stage "Readiness" "assess the exact plan and probe read-only Spotify scopes"
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
printf 'Type the assessment UUID to apply it: '
IFS= read -r CONFIRMATION </dev/tty
[ "$CONFIRMATION" = "$ASSESSMENT_ID" ] || {
    printf 'Confirmation did not match. No apply was attempted.\n' >&2
    exit 1
}

stage "Apply" "execute the exact confirmed $PHASE phase"
run_chordrift sync apply \
    --account "$ACCOUNT" \
    --assessment "$ASSESSMENT_ID" \
    --phase "$PHASE" \
    --confirm "$ASSESSMENT_ID" >"$APPLY_FILE"
APPLY_RUN_ID=$(field apply_run_id "$APPLY_FILE")
[ -n "$APPLY_RUN_ID" ] || {
    printf 'Apply returned no run ID. Stop and inspect state manually.\n' >&2
    exit 1
}
run_chordrift sync apply-show --account "$ACCOUNT" --run "$APPLY_RUN_ID"

stage "Verify" "pull provider state and verify the apply receipt"
run_chordrift sync pull --account "$ACCOUNT"
run_chordrift sync apply-show --account "$ACCOUNT" --run "$APPLY_RUN_ID"

stage "Convergence" "create and inspect the next immutable plan"
run_chordrift sync plan --account "$ACCOUNT" >"$FINAL_PLAN_FILE"
FINAL_PLAN_ID=$(field plan_id "$FINAL_PLAN_FILE")
FINAL_OPERATIONS=$(field operations "$FINAL_PLAN_FILE")
[ -n "$FINAL_PLAN_ID" ] && [ -n "$FINAL_OPERATIONS" ] || {
    printf 'Could not parse the final convergence plan. Inspect manually.\n' >&2
    exit 1
}
run_chordrift sync plan-show --account "$ACCOUNT" --plan "$FINAL_PLAN_ID" --details

if [ "$FINAL_OPERATIONS" = 0 ]; then
    stage "Complete" "provider and Neon converge with zero planned operations"
else
    stage "Review required" "$FINAL_OPERATIONS operation(s) remain; nothing further was applied"
    exit 2
fi
