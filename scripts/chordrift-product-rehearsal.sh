#!/bin/sh

# Provider-write-free V020-11 product rehearsal through one installed Chordrift
# binary. The Rust application boundaries own every audit, selection, and order.

set -eu

ACCOUNT=
RECIPE_REVISION=
ONBOARDING_FIXTURE=
SPIN_FIXTURE=

usage() {
    printf '%s\n' \
        "Usage: scripts/chordrift-product-rehearsal.sh --account UUID --recipe-revision UUID" \
        "       --onboarding-fixture FILE --spin-fixture FILE" \
        "" \
        "Runs inventory-only and enriched onboarding capture/audit, collection" \
        "and recipe review, provider-neutral recipe execution, and exact Spin" \
        "preview/replay through the installed development-line binary." \
        "" \
        "Requires CHORDRIFT_PRODUCT_REHEARSAL=1 and an isolated database with" \
        "migration 0046 already applied. It never invokes db migrate, Spotify," \
        "sync apply, publication approval, or any provider command." \
        "" \
        "Environment:" \
        "  CHORDRIFT_BIN  Installed development binary; defaults to chordrift."
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --account)
            [ "$#" -ge 2 ] || { usage >&2; exit 2; }
            ACCOUNT=$2
            shift 2
            ;;
        --recipe-revision)
            [ "$#" -ge 2 ] || { usage >&2; exit 2; }
            RECIPE_REVISION=$2
            shift 2
            ;;
        --onboarding-fixture)
            [ "$#" -ge 2 ] || { usage >&2; exit 2; }
            ONBOARDING_FIXTURE=$2
            shift 2
            ;;
        --spin-fixture)
            [ "$#" -ge 2 ] || { usage >&2; exit 2; }
            SPIN_FIXTURE=$2
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

[ -n "$ACCOUNT" ] || { printf '%s\n' '--account is required.' >&2; exit 2; }
[ -n "$RECIPE_REVISION" ] || { printf '%s\n' '--recipe-revision is required.' >&2; exit 2; }
[ -n "$ONBOARDING_FIXTURE" ] || { printf '%s\n' '--onboarding-fixture is required.' >&2; exit 2; }
[ -n "$SPIN_FIXTURE" ] || { printf '%s\n' '--spin-fixture is required.' >&2; exit 2; }
[ "${CHORDRIFT_PRODUCT_REHEARSAL:-}" = 1 ] || {
    printf '%s\n' 'Set CHORDRIFT_PRODUCT_REHEARSAL=1 only with an isolated migration-0046 database.' >&2
    exit 2
}

CHORDRIFT_BIN=${CHORDRIFT_BIN:-chordrift}
command -v "$CHORDRIFT_BIN" >/dev/null 2>&1 || {
    printf "Chordrift is not installed or executable as '%s'.\n" "$CHORDRIFT_BIN" >&2
    exit 127
}

WORK_DIR=$(mktemp -d "${TMPDIR:-/tmp}/chordrift-product-rehearsal.XXXXXX")
cleanup() {
    case "$WORK_DIR" in
        "${TMPDIR:-/tmp}"/chordrift-product-rehearsal.*) rm -rf -- "$WORK_DIR" ;;
    esac
}
trap cleanup EXIT HUP INT TERM

run_chordrift() { "$CHORDRIFT_BIN" "$@"; }
field() { sed -n "s/^$1: //p" "$2" | tail -n 1; }
stage() { printf '\n%s: %s\n' "$1" "$2"; }

run_chordrift product --help >/dev/null

stage "Inventory baseline" "capture and audit current inventory only"
run_chordrift product onboarding capture \
    --fixture "$ONBOARDING_FIXTURE" --mode inventory-only \
    >"$WORK_DIR/inventory-session.txt"
INVENTORY_SESSION=$(field session_id "$WORK_DIR/inventory-session.txt")
[ -n "$INVENTORY_SESSION" ] || { printf 'Inventory capture returned no session ID.\n' >&2; exit 1; }
run_chordrift product onboarding audit \
    --fixture "$ONBOARDING_FIXTURE" --session "$INVENTORY_SESSION" \
    --mode inventory-only >"$WORK_DIR/inventory-audit.txt"
cat "$WORK_DIR/inventory-audit.txt"

stage "Enriched comparison" "capture selected history and prove the same baseline"
run_chordrift product onboarding capture \
    --fixture "$ONBOARDING_FIXTURE" --mode enriched \
    >"$WORK_DIR/enriched-session.txt"
ENRICHED_SESSION=$(field session_id "$WORK_DIR/enriched-session.txt")
[ -n "$ENRICHED_SESSION" ] || { printf 'Enriched capture returned no session ID.\n' >&2; exit 1; }
run_chordrift product onboarding audit \
    --fixture "$ONBOARDING_FIXTURE" --session "$ENRICHED_SESSION" \
    --mode enriched >"$WORK_DIR/enriched-audit.txt"
cat "$WORK_DIR/enriched-audit.txt"

BASELINE_FINGERPRINT=$(field inventory_findings_fingerprint "$WORK_DIR/inventory-audit.txt")
ENRICHED_BASELINE=$(field inventory_findings_fingerprint "$WORK_DIR/enriched-audit.txt")
[ -n "$BASELINE_FINGERPRINT" ] && [ "$BASELINE_FINGERPRINT" = "$ENRICHED_BASELINE" ] || {
    printf 'Enriched audit did not retain the exact inventory-only baseline.\n' >&2
    exit 3
}

stage "Collections" "review account-owned collection boundaries"
run_chordrift product collections list --account "$ACCOUNT"

stage "Recipe" "review and execute the immutable provider-neutral revision"
run_chordrift product recipes show \
    --account "$ACCOUNT" --revision "$RECIPE_REVISION"
run_chordrift product recipes execute --fixture "$SPIN_FIXTURE"

stage "Spin preview" "persist and replay the exact Rust-owned order"
run_chordrift product spins preview --fixture "$SPIN_FIXTURE" \
    >"$WORK_DIR/spin-preview.txt"
cat "$WORK_DIR/spin-preview.txt"
SPIN_ID=$(field spin_id "$WORK_DIR/spin-preview.txt")
PREVIEW_FINGERPRINT=$(field preview_fingerprint "$WORK_DIR/spin-preview.txt")
[ -n "$SPIN_ID" ] && [ -n "$PREVIEW_FINGERPRINT" ] || {
    printf 'Spin preview returned no stable identity or fingerprint.\n' >&2
    exit 1
}
run_chordrift product spins show --account "$ACCOUNT" --spin "$SPIN_ID" \
    >"$WORK_DIR/spin-show.txt"
cat "$WORK_DIR/spin-show.txt"
[ "$(field preview_fingerprint "$WORK_DIR/spin-show.txt")" = "$PREVIEW_FINGERPRINT" ] || {
    printf 'Persisted Spin replay changed the preview fingerprint.\n' >&2
    exit 3
}

printf '\nRehearsal complete: inventory/enriched comparison and Spin replay agree.\n'
printf 'Provider writes: disabled. Publication: not requested.\n'
