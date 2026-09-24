#!/usr/bin/env bash
#
# Run one CI step, and when it fails, raise the end of its output as an
# annotation:  packaging/ci-run.sh <command> [args...]
#
# A workflow's raw log can only be read by someone signed in to GitHub; the
# annotations on a check run are returned by the public API to anyone. So a
# failure that says why in an annotation can be diagnosed from anywhere, and
# one that does not says only "exit code 1".
set -uo pipefail

log="$(mktemp)"
"$@" 2>&1 | tee "$log"
status=${PIPESTATUS[0]}
if [ "$status" -ne 0 ]; then
    # One annotation, the last lines joined with the escaped newline the
    # workflow command format uses; colour codes stripped.
    body=$(tail -n 30 "$log" | sed 's/\x1b\[[0-9;]*m//g' \
        | sed 's/%/%25/g' | awk 'BEGIN{ORS="%0A"} {print}')
    printf '::error title=%s failed (exit %s)::%s\n' "$(basename "$1")" "$status" "$body"
fi
exit "$status"
