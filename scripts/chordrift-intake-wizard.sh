#!/bin/sh

# Interactive operator assistant for mixed current-provider intake. All domain
# decisions and persistence remain in the installed Chordrift binary. The
# script requires a TTY, performs a fresh pull by default, isolates pre-existing
# managed-playlist exclusions before intake, and never handles new playlist,
# retirement, or new-artwork design work.

set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)
ACCOUNT=personal
SKIP_PULL=false
REVIEW_ONLY=false
ARTWORK_MANIFEST=$REPO_ROOT/artwork/canonical/drift-atlas-v4/manifest.json

usage() {
    printf '%s\n' \
        "Usage: scripts/chordrift-intake-wizard.sh [--account LABEL] [--skip-pull] [--review-only]" \
        "       [--artwork-manifest PATH]" \
        "" \
        "Walks through two isolated stages:" \
        "  1. record verified user removals as reversible exclusion intent and" \
        "     hold routine provider convergence until coverage is complete;" \
        "  2. review current Liked Songs and named intake tracks, place them in" \
        "     existing playlists, publish, reconcile, verify, and clean intake." \
        "" \
        "The wizard stops for unrelated publish/retirement work, incomplete" \
        "proposal coverage, a missing existing-playlist suggestion, or artwork" \
        "that cannot be reused unchanged. Every Spotify write retains exact" \
        "assessment confirmation." \
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
[ -t 0 ] && [ -t 1 ] || {
    printf 'This review wizard requires an interactive terminal.\n' >&2
    exit 2
}

CHORDRIFT_BIN=${CHORDRIFT_BIN:-chordrift}
command -v "$CHORDRIFT_BIN" >/dev/null 2>&1 || {
    printf "Chordrift is not installed or executable as '%s'.\n" "$CHORDRIFT_BIN" >&2
    exit 127
}
[ -x "$SCRIPT_DIR/chordrift-plan-phase.sh" ] || {
    printf 'Missing executable helper: %s/chordrift-plan-phase.sh\n' "$SCRIPT_DIR" >&2
    exit 1
}
[ -x "$SCRIPT_DIR/chordrift-manual-place.sh" ] || {
    printf 'Missing executable helper: %s/chordrift-manual-place.sh\n' "$SCRIPT_DIR" >&2
    exit 1
}
[ -x "$SCRIPT_DIR/chordrift-cluster-unresolved.sh" ] || {
    printf 'Missing executable helper: %s/chordrift-cluster-unresolved.sh\n' "$SCRIPT_DIR" >&2
    exit 1
}

WORK_DIR=$(mktemp -d "${TMPDIR:-/tmp}/chordrift-intake-wizard.XXXXXX")
PLAN_FILE=$WORK_DIR/plan.txt
PLAN_DETAILS_FILE=$WORK_DIR/plan-details.tsv
AUDIT_FILE=$WORK_DIR/intake-audit.tsv
CURRENT_INTAKE_IDS_FILE=$WORK_DIR/current-intake-ids.txt
STATUS_FILE=$WORK_DIR/proposal-status.txt
UNRESOLVED_FILE=$WORK_DIR/unresolved.tsv
RESTORE_FILE=$WORK_DIR/restore-ids.txt
AUTO_FILE=$WORK_DIR/automatic-ids.txt
DRAFT_REVIEW_FILE=$WORK_DIR/draft-review-ids.txt
READINESS_FILE=$WORK_DIR/readiness.txt
APPLY_FILE=$WORK_DIR/apply.txt
ARTWORK_STATUS_FILE=$WORK_DIR/artwork-status.txt
: >"$RESTORE_FILE"
: >"$AUTO_FILE"
: >"$DRAFT_REVIEW_FILE"

cleanup() {
    case "$WORK_DIR" in
        "${TMPDIR:-/tmp}"/chordrift-intake-wizard.*) rm -rf -- "$WORK_DIR" ;;
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
    entered=$(ask_value "Type the exact $label $expected: ")
    [ "$entered" = "$expected" ] || {
        printf 'Confirmation did not match. Nothing after this gate was changed.\n' >&2
        exit 1
    }
}

create_plan() {
    run_chordrift sync plan --account "$ACCOUNT" >"$PLAN_FILE"
    PLAN_ID=$(field plan_id "$PLAN_FILE")
    [ -n "$PLAN_ID" ] || {
        printf 'Could not parse the current plan ID. Stop and inspect manually.\n' >&2
        exit 1
    }
    run_chordrift sync plan-show \
        --account "$ACCOUNT" --plan "$PLAN_ID" --details >"$PLAN_DETAILS_FILE"
    SNAPSHOT_CURRENT=$(field snapshot_current "$PLAN_DETAILS_FILE")
    [ "$SNAPSHOT_CURRENT" = true ] || {
        printf 'Plan %s is already stale. Run the wizard again for a fresh pull.\n' "$PLAN_ID" >&2
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

operation_count() {
    awk -F '\t' -v operation="$1" '
        $1 == "sequence" { operations = 1; next }
        operations && $1 ~ /^[0-9]+$/ && $3 == operation { count += 1 }
        END { print count + 0 }
    ' "$PLAN_DETAILS_FILE"
}

show_operation() {
    operation=$1
    awk -F '\t' -v operation="$operation" '
        BEGIN { OFS = "\t"; print "Phase", "Operation", "Playlist", "Track", "Safety" }
        $1 ~ /^[0-9]+$/ && $3 == operation { print $2, $3, $4, $6, $8 }
    ' "$PLAN_DETAILS_FILE"
}

audit_intake() {
    run_chordrift intake audit --account "$ACCOUNT" >"$AUDIT_FILE" || {
        printf '\nThe installed Chordrift binary does not provide the required read-only intake audit.\n' >&2
        printf 'Build/install the branch containing `chordrift intake audit`; the wizard will not query Neon or Spotify directly.\n' >&2
        exit 1
    }
    awk -F '\t' '
        $1 == "state" { rows = 1; next }
        rows && $11 != "" { print $11 }
    ' "$AUDIT_FILE" | sort -u >"$CURRENT_INTAKE_IDS_FILE"
    run_chordrift intake audit --account "$ACCOUNT"
}

unresolved_is_intake_only() {
    awk -F '\t' '
        FILENAME == ARGV[1] { intake[$1] = 1; next }
        FNR == 1 { next }
        !($5 in intake) { print }
    ' "$CURRENT_INTAKE_IDS_FILE" "$UNRESOLVED_FILE" \
        >"$WORK_DIR/unrelated-unresolved.tsv"
    [ ! -s "$WORK_DIR/unrelated-unresolved.tsv" ]
}

ensure_editable_proposal() {
    run_chordrift proposals status --account "$ACCOUNT" >"$STATUS_FILE"
    PROPOSAL_STATE=$(field proposal "$STATUS_FILE")
    case "$PROPOSAL_STATE" in
        proposed) ;;
        approved)
            ask_yes "Prepare an editable copy of the approved library for these intake decisions?" || exit 0
            run_chordrift proposals extend --account "$ACCOUNT" --min-similarity 1
            ;;
        *)
            printf 'Latest proposal state is %s; this wizard cannot prepare it safely.\n' \
                "$PROPOSAL_STATE" >&2
            exit 1
            ;;
    esac
    run_chordrift proposals status --account "$ACCOUNT" >"$STATUS_FILE"
    PROPOSAL_ID=$(field generation_id "$STATUS_FILE")
    [ "$(field proposal "$STATUS_FILE")" = proposed ] || {
        printf 'No editable proposal is available.\n' >&2
        exit 1
    }
}

if [ "$SKIP_PULL" = false ]; then
    stage "Observe" "fresh Spotify pull; provider state is authoritative"
    run_chordrift sync pull --account "$ACCOUNT"
else
    stage "Observe" "using the operator-confirmed current snapshot"
fi

stage "Separate changes" "classify existing work before changing intake intent"
create_plan
PENDING_EXCLUSIONS=$(operation_count exclude_track)
PUBLISH_COUNT=$(phase_count publish)
RECONCILE_COUNT=$(phase_count reconcile)
RETIREMENT_COUNT=$(phase_count retirement)
CREATE_COUNT=$(operation_count create_playlist)

if [ "$RETIREMENT_COUNT" -gt 0 ] || [ "$CREATE_COUNT" -gt 0 ]; then
    printf 'The current plan contains retirement or new-playlist work. That requires its separate creative/destructive workflow.\n' >&2
    run_chordrift sync plan-show --account "$ACCOUNT" --plan "$PLAN_ID" --details
    exit 3
fi

if [ "$PENDING_EXCLUSIONS" -gt 0 ]; then
    printf '%s verified user removal(s) can be recorded as reversible exclusions before intake.\n' \
        "$PENDING_EXCLUSIONS"
    show_operation exclude_track
fi

if [ "$PUBLISH_COUNT" -gt 0 ] || [ "$RECONCILE_COUNT" -gt 0 ]; then
    printf '%s publish and %s reconciliation operation(s) are being held until intake coverage is complete.\n' \
        "$PUBLISH_COUNT" "$RECONCILE_COUNT"
fi

stage "Intake audit" "join the exact current intake snapshot with Neon intent and history"
audit_intake
ITEMS=$(field items "$AUDIT_FILE")
if [ "$REVIEW_ONLY" = true ]; then
    printf '\nReview complete. The fresh pull persisted current observation only; no intent, approval, or Spotify write was attempted.\n'
    exit 0
fi

if [ "$PENDING_EXCLUSIONS" -gt 0 ]; then
    stage "Record exclusions" "separate reversible Neon intent from later provider convergence"
    ask_yes "Record all $PENDING_EXCLUSIONS exact managed-playlist removals as reversible exclusions?" || {
        printf 'Stopped before intake placement; no exclusion intent was recorded.\n'
        exit 0
    }
    require_exact "baseline plan ID" "$PLAN_ID"
    awk -F '\t' '
        $1 ~ /^[0-9]+$/ && $3 == "exclude_track" { print $4 "\t" $6 }
    ' "$PLAN_DETAILS_FILE" >"$WORK_DIR/pending-exclusions.tsv"
    while IFS="$(printf '\t')" read -r playlist spotify_id; do
        run_chordrift tracks exclude \
            --account "$ACCOUNT" \
            --spotify-id "$spotify_id" \
            --reason "Removed from verified managed playlist: $playlist" \
            --confirm "$spotify_id"
    done <"$WORK_DIR/pending-exclusions.tsv"
    create_plan
    [ "$(operation_count exclude_track)" -eq 0 ] || {
        printf 'One or more exact exclusions remain in the new plan. Stop and inspect manually.\n' >&2
        exit 3
    }
    stage "Intake audit refreshed" "include newly recorded exclusion intent"
    audit_intake
    ITEMS=$(field items "$AUDIT_FILE")
fi

CURRENT_OPERATIONS=$(field operations "$PLAN_FILE")
if [ "$ITEMS" -eq 0 ] && [ "$CURRENT_OPERATIONS" -eq 0 ]; then
    printf 'No current intake decisions or provider convergence work remain.\n'
    exit 0
fi

PREVIOUSLY_EXCLUDED=$(field previously_excluded "$AUDIT_FILE")
KNOWN_FROM_HISTORY=$(field known_from_history "$AUDIT_FILE")
GENUINELY_NEW=$(field genuinely_new "$AUDIT_FILE")
SUGGESTED_IN_DRAFT=$(field suggested_in_draft "$AUDIT_FILE")
awk -F '\t' '
    $1 == "state" { rows = 1; next }
    rows && $1 == "suggested_in_draft" { print $11 }
' "$AUDIT_FILE" >"$DRAFT_REVIEW_FILE"
if [ "$PREVIOUSLY_EXCLUDED" -gt 0 ]; then
    stage "Exclusion review" "keep each exclusion or restore it for placement"
    awk -F '\t' '
        $1 == "state" { rows = 1; next }
        rows && $1 == "previously_excluded" { print $2 "\t" $3 "\t" $11 }
    ' "$AUDIT_FILE" >"$WORK_DIR/excluded.tsv"
    while IFS="$(printf '\t')" read -r title artists spotify_id; do
        printf '\nPreviously excluded: %s — %s [%s]\n' "$title" "$artists" "$spotify_id"
        if ask_yes "Restore this track because its new intake gesture means reconsider it?"; then
            printf '%s\n' "$spotify_id" >>"$RESTORE_FILE"
        else
            printf 'Keeping the active exclusion; verified cleanup may remove it from intake.\n'
        fi
    done <"$WORK_DIR/excluded.tsv"
fi

if [ "$KNOWN_FROM_HISTORY" -gt 0 ] || [ "$GENUINELY_NEW" -gt 0 ] ||
   [ "$SUGGESTED_IN_DRAFT" -gt 0 ] || [ -s "$RESTORE_FILE" ]; then
    stage "Draft" "prepare reversible Neon intent; Spotify remains unchanged"
    ensure_editable_proposal
    while IFS= read -r spotify_id; do
        run_chordrift tracks restore \
            --account "$ACCOUNT" \
            --spotify-id "$spotify_id" \
            --reason "Current provider intake gesture requested reconsideration" \
            --confirm "$spotify_id"
    done <"$RESTORE_FILE"

    run_chordrift proposals unresolved --account "$ACCOUNT" --limit 10000 >"$UNRESOLVED_FILE"
    if ! unresolved_is_intake_only; then
        printf '\nThe editable proposal also contains unrelated unresolved tracks.\n' >&2
        cat "$WORK_DIR/unrelated-unresolved.tsv" >&2
        printf 'The wizard stops rather than mixing those decisions into this intake batch.\n' >&2
        exit 3
    fi

    UNRESOLVED_COUNT=$(awk 'NR > 1 { count += 1 } END { print count + 0 }' "$UNRESOLVED_FILE")
    if [ "$UNRESOLVED_COUNT" -gt 0 ]; then
        stage "Placement choice" "manual cultural intent first; normal suggestions second"
        run_chordrift proposals list --account "$ACCOUNT"
        while IFS="$(printf '\t')" read -r title artists source_classes source_playlists spotify_id; do
            [ "$spotify_id" != spotify_id ] || continue
            printf '\n%s — %s\nSources: %s / %s\nSpotify ID: %s\n' \
                "$title" "$artists" "$source_classes" "$source_playlists" "$spotify_id"
            printf 'Choose [a]utomatic existing-playlist suggestion, [m]anual destination, [x] exclude, [q] stop: ' >/dev/tty
            IFS= read -r choice </dev/tty
            case "$choice" in
                a|A)
                    printf '%s\n' "$spotify_id" >>"$AUTO_FILE"
                    ;;
                m|M)
                    destination=$(ask_value 'Exact destination display name: ')
                    [ -n "$destination" ] || { printf 'Destination is required.\n' >&2; exit 2; }
                    reason=$(ask_value 'Reason for this placement: ')
                    [ -n "$reason" ] || reason="Reviewed current intake placement"
                    "$SCRIPT_DIR/chordrift-manual-place.sh" \
                        --account "$ACCOUNT" --to "$destination" \
                        --spotify-id "$spotify_id" --reason "$reason"
                    ;;
                x|X)
                    reason=$(ask_value 'Reason for excluding this track: ')
                    [ -n "$reason" ] || reason="Rejected during current intake review"
                    run_chordrift tracks exclude \
                        --account "$ACCOUNT" --spotify-id "$spotify_id" \
                        --reason "$reason" --confirm "$spotify_id"
                    ;;
                *)
                    printf 'Stopped with the proposal editable and Spotify unchanged.\n'
                    exit 0
                    ;;
            esac
        done <"$UNRESOLVED_FILE"
    fi

    if [ -s "$AUTO_FILE" ]; then
        stage "Suggest" "audit existing-playlist fits before recording draft suggestions"
        "$SCRIPT_DIR/chordrift-cluster-unresolved.sh" \
            --account "$ACCOUNT" --include-intake
        run_chordrift proposals status --account "$ACCOUNT" >"$STATUS_FILE"
        PROPOSAL_ID=$(field generation_id "$STATUS_FILE")
        require_exact "proposal generation ID" "$PROPOSAL_ID"
        "$SCRIPT_DIR/chordrift-cluster-unresolved.sh" \
            --account "$ACCOUNT" --include-intake \
            --apply --confirm "$PROPOSAL_ID"
        cat "$AUTO_FILE" >>"$DRAFT_REVIEW_FILE"
    fi

    if [ -s "$DRAFT_REVIEW_FILE" ]; then
        sort -u "$DRAFT_REVIEW_FILE" >"$WORK_DIR/draft-review-sorted.txt"
        stage "Suggestion review" "keep or correct every proposed destination"
        audit_intake
        while IFS= read -r spotify_id; do
            row=$(awk -F '\t' -v id="$spotify_id" '$11 == id { print; exit }' "$AUDIT_FILE")
            [ -n "$row" ] || { printf 'Intake item %s disappeared; stop and inspect.\n' "$spotify_id" >&2; exit 1; }
            title=$(printf '%s\n' "$row" | cut -f2)
            artists=$(printf '%s\n' "$row" | cut -f3)
            destination=$(printf '%s\n' "$row" | cut -f6)
            [ -n "$destination" ] || {
                printf 'No existing-playlist suggestion was available for %s — %s.\n' "$title" "$artists" >&2
                printf 'Use manual placement or a separate new-playlist/artwork workflow.\n' >&2
                exit 3
            }
            printf '\nSuggested: %s — %s → %s\n' "$title" "$artists" "$destination"
            printf 'Choose [k]eep, [m]anual correction, [r]eturn to review, [q] stop: ' >/dev/tty
            IFS= read -r choice </dev/tty
            case "$choice" in
                k|K) ;;
                m|M)
                    replacement=$(ask_value 'Exact replacement destination display name: ')
                    reason=$(ask_value 'Reason for this correction: ')
                    [ -n "$reason" ] || reason="Corrected automatic intake suggestion"
                    "$SCRIPT_DIR/chordrift-manual-place.sh" \
                        --account "$ACCOUNT" --to "$replacement" \
                        --spotify-id "$spotify_id" --reason "$reason"
                    ;;
                r|R)
                    run_chordrift proposals review \
                        --account "$ACCOUNT" --spotify-id "$spotify_id" \
                        --reason "Suggestion deferred during intake review"
                    ;;
                *)
                    printf 'Stopped with draft suggestions in Neon and Spotify unchanged.\n'
                    exit 0
                    ;;
            esac
        done <"$WORK_DIR/draft-review-sorted.txt"
    fi

    run_chordrift proposals unresolved --account "$ACCOUNT" --limit 10000 >"$UNRESOLVED_FILE"
    REMAINING=$(awk 'NR > 1 { count += 1 } END { print count + 0 }' "$UNRESOLVED_FILE")
    [ "$REMAINING" -eq 0 ] || {
        printf '%s track(s) remain unresolved. No whole-proposal approval was attempted.\n' \
            "$REMAINING" >&2
        cat "$UNRESOLVED_FILE" >&2
        exit 3
    }

    run_chordrift proposals status --account "$ACCOUNT" >"$STATUS_FILE"
    PROPOSAL_ID=$(field generation_id "$STATUS_FILE")
    COVERAGE=$(field coverage_complete "$STATUS_FILE")
    [ "$COVERAGE" = true ] || {
        printf 'Proposal coverage is incomplete. No approval was attempted.\n' >&2
        exit 3
    }
    run_chordrift proposals status --account "$ACCOUNT"
    require_exact "complete proposal generation ID" "$PROPOSAL_ID"
    run_chordrift proposals approve --account "$ACCOUNT" --confirm "$PROPOSAL_ID"

    stage "Existing artwork" "reuse only the unchanged, already reviewed visual system"
    if run_chordrift artwork status --account "$ACCOUNT" >"$ARTWORK_STATUS_FILE" 2>/dev/null &&
       [ "$(field proposal_generation_id "$ARTWORK_STATUS_FILE")" = "$PROPOSAL_ID" ] &&
       [ "$(field artwork "$ARTWORK_STATUS_FILE")" = approved ]; then
        run_chordrift artwork status --account "$ACCOUNT"
    else
        [ -f "$ARTWORK_MANIFEST" ] || {
            printf 'No reusable artwork manifest exists at %s.\n' "$ARTWORK_MANIFEST" >&2
            printf 'Use the separate new-playlist/artwork workflow; no Spotify write occurred.\n' >&2
            exit 3
        }
        ask_yes "Reuse the existing reviewed artwork files unchanged for this proposal generation?" || exit 0
        ARTWORK_SOURCE_DIR=$(CDPATH= cd -- "$(dirname -- "$ARTWORK_MANIFEST")" && pwd)
        ARTWORK_REVIEW_DIR=$WORK_DIR/artwork-review
        mkdir "$ARTWORK_REVIEW_DIR"
        cp -R "$ARTWORK_SOURCE_DIR"/. "$ARTWORK_REVIEW_DIR"/
        sed -E "s/(\"proposal_generation_id\"[[:space:]]*:[[:space:]]*\")[^\"]*(\")/\\1$PROPOSAL_ID\\2/" \
            "$ARTWORK_REVIEW_DIR/$(basename -- "$ARTWORK_MANIFEST")" \
            >"$ARTWORK_REVIEW_DIR/manifest.next.json"
        mv "$ARTWORK_REVIEW_DIR/manifest.next.json" \
            "$ARTWORK_REVIEW_DIR/$(basename -- "$ARTWORK_MANIFEST")"
        run_chordrift artwork import --account "$ACCOUNT" \
            --manifest "$ARTWORK_REVIEW_DIR/$(basename -- "$ARTWORK_MANIFEST")"
        run_chordrift artwork status --account "$ACCOUNT" >"$ARTWORK_STATUS_FILE"
        BATCH_ID=$(field batch_id "$ARTWORK_STATUS_FILE")
        run_chordrift artwork status --account "$ACCOUNT"
        require_exact "artwork batch ID" "$BATCH_ID"
        run_chordrift artwork approve --account "$ACCOUNT" --confirm "$BATCH_ID"
    fi
fi

stage "Cleanup policy" "Liked Songs clears only after verified assignment or exclusion"
if ask_yes "Use Liked Songs as temporary intake for this account?"; then
    run_chordrift spotify library-policy \
        --account "$ACCOUNT" --liked-songs clear-after-verified-assignment
else
    printf 'Liked Songs will be preserved; named intake playlists may still be cleaned after verification.\n'
fi

ask_yes "Proceed to exact Spotify publication/verification and later intake cleanup?" || {
    printf 'Stopped before provider writes. Approved Neon intent remains available for later planning.\n'
    exit 0
}

stage "Execute" "one reviewed phase per fresh provider snapshot"
iterations=0
while [ "$iterations" -lt 5 ]; do
    iterations=$((iterations + 1))
    create_plan
    OPERATIONS=$(field operations "$PLAN_FILE")
    [ "$OPERATIONS" -gt 0 ] || {
        printf 'Converged: the latest plan has zero operations.\n'
        exit 0
    }
    if [ "$(operation_count create_playlist)" -gt 0 ] || [ "$(phase_count retirement)" -gt 0 ]; then
        printf 'The plan requires new-playlist or retirement work, which this wizard intentionally isolates.\n' >&2
        run_chordrift sync plan-show --account "$ACCOUNT" --plan "$PLAN_ID" --details
        exit 3
    fi
    if [ "$(phase_count publish)" -gt 0 ]; then
        "$SCRIPT_DIR/chordrift-plan-phase.sh" \
            --account "$ACCOUNT" --plan "$PLAN_ID" --phase publish
        continue
    fi
    if [ "$(phase_count reconcile)" -gt 0 ]; then
        "$SCRIPT_DIR/chordrift-plan-phase.sh" \
            --account "$ACCOUNT" --plan "$PLAN_ID" --phase reconcile
        continue
    fi
    if [ "$(phase_count cleanup)" -gt 0 ]; then
        stage "Destructive intake cleanup" "review exact verified removals"
        run_chordrift sync plan-show --account "$ACCOUNT" --plan "$PLAN_ID" --details
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
            printf 'Cleanup returned no apply run ID. Stop and inspect manually.\n' >&2
            exit 1
        }
        run_chordrift sync apply-show --account "$ACCOUNT" --run "$APPLY_RUN_ID"
        run_chordrift sync pull --account "$ACCOUNT"
        run_chordrift sync apply-show --account "$ACCOUNT" --run "$APPLY_RUN_ID"
        continue
    fi
    printf 'The plan contains an unsupported phase. No further apply was attempted.\n' >&2
    run_chordrift sync plan-show --account "$ACCOUNT" --plan "$PLAN_ID" --details
    exit 3
done

printf 'The wizard reached its five-phase safety bound. Run it again to reassess fresh state.\n' >&2
exit 3
