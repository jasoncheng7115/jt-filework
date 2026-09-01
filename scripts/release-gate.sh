#!/usr/bin/env bash
#
# The automated half of docs/RELEASE_CHECKLIST.md.
#
# Fails on the first problem and says which line failed, because a gate that
# reports six things at once gets skimmed. Everything here also runs in CI;
# this is the same gate, runnable before pushing.
set -euo pipefail

cd "$(dirname "$0")/.."

step() { printf '\n\033[1m== %s\033[0m\n' "$1"; }
fail() { printf '\033[31mFAILED: %s\033[0m\n' "$1" >&2; exit 1; }

version=$(awk -F'"' '/^version = "/ {print $2; exit}' Cargo.toml)
[ -n "$version" ] || fail "no version in Cargo.toml"
printf 'jt-filework %s\n' "$version"

step "formatting"
cargo fmt --all --check || fail "cargo fmt --check"

step "lints"
cargo clippy --workspace --all-targets --all-features -- -D warnings \
  || fail "cargo clippy"

step "tests"
cargo test --workspace || fail "cargo test"

step "dependencies"
if command -v cargo-deny >/dev/null 2>&1; then
  cargo deny check || fail "cargo deny"
else
  fail "cargo-deny is not installed: cargo install --locked cargo-deny"
fi

step "the version says the same thing everywhere"
# A release whose About box, changelog and download page disagree about which
# version it is cannot be reported against usefully.
for file in CHANGELOG.md CHANGELOG_zh-TW.md; do
  grep -q "^## \[$version\]" "$file" \
    || fail "$file has no section for $version (both languages, by hand)"
done
for file in README.md README_zh-TW.md docs/index.html; do
  grep -q "$version" "$file" || fail "$file does not mention $version"
done

step "the test plan is consistent"
# Ids unique, every case at a named layer, no case contradicting a rule the
# program follows. Run as part of the tests, listed here so the gate says it.
cargo test --test test_plan --quiet >/dev/null || fail "docs/UI_TEST_PLAN.md"

step "a fixture for every stored format version"
formats=$(grep -o 'SESSION_FORMAT_VERSION: u32 = [0-9]*' src/workspace/src/session.rs \
          | grep -o '[0-9]*$')
[ -n "$formats" ] || fail "could not read SESSION_FORMAT_VERSION"
for n in $(seq 1 "$formats"); do
  [ -f "tests/fixtures/session/v$n.json" ] \
    || fail "no fixture for session format v$n"
done

printf '\n\033[32mThe automated half of the gate passed.\033[0m\n'
printf 'The half a person has to do is docs/RELEASE_CHECKLIST.md 2 onward.\n'
