#!/bin/sh

# Shared, side-effect-free plan checks for the Re-evaluate operator workflow.
# This file is sourced by the wizard and directly exercised by regression tests.

reevaluate_unexpected_reconcile_operations() {
    selected_ids_file=$1
    plan_details_file=$2
    awk -F '\t' '
        FILENAME == ARGV[1] { selected[$1] = 1; next }
        $1 == "sequence" { operations = 1; next }
        operations && $1 ~ /^[0-9]+$/ && $2 == "reconcile" {
            if ($3 == "remove_track" && $4 != "Re-evaluate" &&
                ($6 in selected) && index($7, "Reason=managed_provider_drift") > 0) next
            print
        }
    ' "$selected_ids_file" "$plan_details_file"
}
