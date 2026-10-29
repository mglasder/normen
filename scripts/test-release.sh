#!/usr/bin/env bash
# Tests for scripts/release.sh. Run: bash scripts/test-release.sh
# All git work happens in temp repos, never in the real repo.
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RELEASE="$HERE/release.sh"
# shellcheck source=release.sh
source "$RELEASE"
set +e # release.sh sets -e; the tests check exit codes themselves

export GIT_AUTHOR_NAME=t GIT_AUTHOR_EMAIL=t@t GIT_COMMITTER_NAME=t GIT_COMMITTER_EMAIL=t@t
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_SYSTEM=/dev/null

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
PASS=0
FAIL=0

ok() { PASS=$((PASS + 1)); echo "ok   $1"; }
bad() { FAIL=$((FAIL + 1)); echo "FAIL $1${2:+: $2}"; }

assert_eq() { # name expected actual
  if [[ "$2" == "$3" ]]; then ok "$1"; else bad "$1" "expected '$2', got '$3'"; fi
}
assert_fails() { # name command...
  local name="$1"; shift
  if ( "$@" ) >/dev/null 2>&1; then bad "$name" "expected failure"; else ok "$name"; fi
}
assert_succeeds() {
  local name="$1"; shift
  if ( "$@" ) >/dev/null 2>&1; then ok "$name"; else bad "$name" "expected success"; fi
}

# new_repo <dir>: repo on dev with main, Cargo.toml 0.1.0, bare origin, tag v0.1.0.
new_repo() {
  local dir="$1"
  git init -q -b main "$dir"
  (
    cd "$dir" || exit 1
    printf '[package]\nname = "x"\nversion = "0.1.0"\n' >Cargo.toml
    git add . && git commit -qm "Initial commit."
    git tag v0.1.0
    git init -q --bare "$dir.origin"
    git remote add origin "$dir.origin"
    git checkout -q -b dev
    git push -q origin main dev
    git fetch -q origin
  )
}

commit() { # message
  echo "$1" >"$(echo "$1" | tr -c 'A-Za-z0-9\n' _).txt"
  git add . && git commit -qm "$1"
}

# --- next_version ---
assert_eq "next_version patch" "1.2.4" "$(next_version 1.2.3 patch)"
assert_eq "next_version minor" "1.3.0" "$(next_version 1.2.3 minor)"
assert_eq "next_version major" "2.0.0" "$(next_version 1.2.3 major)"
assert_fails "next_version rejects unknown bump" next_version 1.2.3 huge

# --- validate_version ---
R="$TMP/validate"
new_repo "$R"
cd "$R" || exit 1
git tag v0.3.0
assert_succeeds "validate custom greater" validate_version 0.1.0 0.2.0
assert_fails "validate custom lower" validate_version 0.2.0 0.1.0
assert_fails "validate custom equal" validate_version 0.1.0 0.1.0
assert_fails "validate malformed 1.2" validate_version 0.1.0 1.2
assert_fails "validate malformed v-prefix" validate_version 0.1.0 v1.2.3
assert_fails "validate malformed text" validate_version 0.1.0 abc
assert_fails "validate existing tag" validate_version 0.1.0 0.3.0
assert_succeeds "validate 0.10.0 > 0.9.0" validate_version 0.9.0 0.10.0

# --- suggest_bump ---
R="$TMP/bump"
new_repo "$R"
cd "$R" || exit 1
assert_eq "suggest default patch (no commits)" "patch" "$(suggest_bump)"
commit "Fix a typo."
commit "Update the README."
assert_eq "suggest patch for fixes" "patch" "$(suggest_bump)"
commit "Add a new command."
assert_eq "suggest minor for Add" "minor" "$(suggest_bump)"
git tag v0.2.0
commit "Support more input."
assert_eq "suggest minor for Support (after new tag)" "minor" "$(suggest_bump)"
git tag v0.3.0
commit "feat: something"
assert_eq "suggest minor for feat" "minor" "$(suggest_bump)"
commit "Change output format, BREAKING."
assert_eq "suggest major for BREAKING" "major" "$(suggest_bump)"
git tag v1.0.0
commit "Address review notes."
assert_eq "Address is not Add" "patch" "$(suggest_bump)"

# --- preflight failures ---
R="$TMP/pf"
new_repo "$R"
cd "$R" || exit 1
STUB="$TMP/stub"
mkdir -p "$STUB"
printf '#!/bin/sh\necho "$@" >>"%s/cargo.log"\nexit 0\n' "$TMP" >"$STUB/cargo"
chmod +x "$STUB/cargo"
export PATH="$STUB:$PATH"

assert_succeeds "preflight passes on clean dev" preflight

git checkout -q main
assert_fails "preflight fails on wrong branch" preflight
git checkout -q dev

echo x >dirty.txt
assert_fails "preflight fails on dirty tree" preflight
rm dirty.txt

# dev behind origin/dev: push a commit from a second clone
git clone -q "$R.origin" "$TMP/other"
(cd "$TMP/other" && git checkout -q dev && commit "Remote work." && git push -q origin dev)
assert_fails "preflight fails when dev is behind origin" preflight
git merge -q --ff-only origin/dev

# main ahead of dev: preflight merges it into dev
(cd "$TMP/other" && git checkout -q main && commit "Hotfix on main." && git push -q origin main)
assert_succeeds "preflight merges main into dev" preflight
assert_succeeds "dev contains origin/main afterwards" git merge-base --is-ancestor origin/main HEAD

# --- dry run does not mutate ---
R="$TMP/dry"
new_repo "$R"
cd "$R" || exit 1
git clone -q "$R.origin" "$TMP/other2"
(cd "$TMP/other2" && git checkout -q main && commit "Add feature on main." && git push -q origin main)
: >"$TMP/cargo.log"
snapshot() { git for-each-ref refs/heads refs/tags; git status --porcelain; git rev-parse --abbrev-ref HEAD; }
before="$(snapshot)"
out="$(bash "$RELEASE" --dry-run --yes 0.1.1 2>&1)"
rc=$?
after="$(snapshot)"
assert_eq "dry-run exits 0" "0" "$rc"
assert_eq "dry-run leaves refs, tree and branch unchanged" "$before" "$after"
case "$out" in *"cargo release 0.1.1 --execute --no-confirm"*) ok "dry-run prints the cargo release command" ;; *) bad "dry-run prints the cargo release command" "$out" ;; esac
case "$out" in *"git merge --ff-only dev"*) ok "dry-run prints the main merge" ;; *) bad "dry-run prints the main merge" ;; esac
if grep -qx "test" "$TMP/cargo.log"; then ok "dry-run runs cargo test for real"; else bad "dry-run runs cargo test for real" "$(cat "$TMP/cargo.log")"; fi
if grep -q -- '--execute' "$TMP/cargo.log"; then bad "dry-run never runs cargo release --execute"; else ok "dry-run never runs cargo release --execute"; fi
assert_fails "--yes without a version is rejected" bash "$RELEASE" --dry-run --yes

echo
echo "passed: $PASS, failed: $FAIL"
[[ "$FAIL" -eq 0 ]]
