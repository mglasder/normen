#!/usr/bin/env bash
# Release automation: preflight, version choice, dev -> main, cargo release, sync back.
#
# Usage:
#   scripts/release.sh                       interactive
#   scripts/release.sh --yes 0.2.0           no prompts, explicit version
#   scripts/release.sh --dry-run [--yes X.Y.Z]  print the plan and commands, change nothing (still runs cargo test)
#
# Needs cargo-release (cargo install cargo-release). Pushing the tag starts the
# dist workflow, which publishes Homebrew and crates.io. Secrets on the GitHub
# repo: HOMEBREW_TAP_TOKEN, CARGO_REGISTRY_TOKEN.
set -euo pipefail

DRY_RUN=0
ASSUME_YES=0
REPO="mglasder/normen"

die() { echo "error: $*" >&2; exit 1; }

# Print the command; run it unless this is a dry run.
run() {
  echo "+ $*"
  [[ "$DRY_RUN" == 1 ]] || "$@"
}

current_version() {
  sed -n 's/^version *= *"\([0-9][0-9.]*\)".*/\1/p' Cargo.toml | head -n1
}

next_version() { # current bump
  local major minor patch
  IFS=. read -r major minor patch <<<"$1"
  case "$2" in
    patch) echo "$major.$minor.$((patch + 1))" ;;
    minor) echo "$major.$((minor + 1)).0" ;;
    major) echo "$((major + 1)).0.0" ;;
    *) echo "unknown bump: $2" >&2; return 1 ;;
  esac
}

# validate_version <current> <new>: X.Y.Z, greater than current, tag not taken.
validate_version() {
  local current="$1" new="$2"
  if [[ ! "$new" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "not a version of the form X.Y.Z: $new" >&2; return 1
  fi
  if [[ "$new" == "$current" ]] || [[ "$(printf '%s\n%s\n' "$current" "$new" | sort -V | tail -n1)" != "$new" ]]; then
    echo "$new is not greater than $current" >&2; return 1
  fi
  if git rev-parse -q --verify "refs/tags/v$new" >/dev/null; then
    echo "tag v$new already exists" >&2; return 1
  fi
}

latest_tag() { git describe --tags --match 'v*' --abbrev=0 2>/dev/null || true; }

commits_since_tag() {
  local tag
  tag="$(latest_tag)"
  if [[ -n "$tag" ]]; then git log --oneline "$tag..HEAD"; else git log --oneline; fi
}

# Print patch, minor or major. The commits are not Conventional Commits.
suggest_bump() {
  local bump=patch subject
  while IFS= read -r subject; do
    subject="${subject#* }" # drop the hash
    case "$subject" in
      *BREAKING*) echo major; return ;;
      "Add "* | "Support "* | feat*) bump=minor ;;
    esac
  done < <(commits_since_tag)
  echo "$bump"
}

preflight() {
  [[ "$(git rev-parse --abbrev-ref HEAD)" == dev ]] || die "not on dev"
  [[ -z "$(git status --porcelain)" ]] || die "working tree is not clean"
  git fetch origin || die "git fetch origin failed"
  git merge-base --is-ancestor origin/dev HEAD || die "dev is behind origin/dev; pull first"
  if ! git merge-base --is-ancestor origin/main HEAD; then
    echo "main is not in dev yet; merging origin/main into dev."
    run git merge --no-edit origin/main || die "merging main into dev failed; resolve and rerun"
  fi
  cargo test || die "cargo test failed"
}

require_cargo_release() {
  cargo release --version >/dev/null 2>&1 \
    || die "cargo-release is missing. Install it: cargo install cargo-release"
}

show_plan() { # current new
  cat <<PLAN

Release plan
  version:  $1 -> $2
  tag:      v$2
  branches: dev (merged into main, then updated with the release commit)
            main (release commit and tag, pushed by cargo release)
PLAN
}

choose_version() { # current -> prints the new version
  local current="$1" suggestion choice new
  suggestion="$(suggest_bump)"
  {
    echo "Commits since $(latest_tag):"
    commits_since_tag
    echo
    echo "Suggested bump: $suggestion"
  } >&2
  while true; do
    read -r -p "Bump [patch/minor/major/custom] (Enter = $suggestion): " choice
    choice="${choice:-$suggestion}"
    case "$choice" in
      patch | minor | major) new="$(next_version "$current" "$choice")" ;;
      custom)
        read -r -p "Version X.Y.Z: " new
        ;;
      *) echo "Unknown choice." >&2; continue ;;
    esac
    if validate_version "$current" "$new"; then
      echo "$new"
      return
    fi
  done
}

release() { # version
  run git checkout main
  run git pull --ff-only
  run git merge --ff-only dev
  run cargo release "$1" --execute --no-confirm
  run git checkout dev
  run git merge --ff-only main
  run git push origin dev
  echo
  echo "Watch the release:"
  echo "  gh run watch --workflow release.yml --repo $REPO"
}

main() {
  local version=""
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --dry-run) DRY_RUN=1 ;;
      --yes) ASSUME_YES=1 ;;
      -h | --help) sed -n '2,11p' "${BASH_SOURCE[0]}"; exit 0 ;;
      -*) die "unknown option: $1" ;;
      *) version="$1" ;;
    esac
    shift
  done
  [[ "$ASSUME_YES" == 0 || -n "$version" ]] || die "--yes needs an explicit version"

  cd "$(git rev-parse --show-toplevel)"
  require_cargo_release
  preflight

  local current
  current="$(current_version)"
  if [[ -z "$version" ]]; then
    version="$(choose_version "$current")"
  else
    validate_version "$current" "$version" || exit 1
  fi
  show_plan "$current" "$version"

  if [[ "$ASSUME_YES" == 0 && "$DRY_RUN" == 0 ]]; then
    local answer
    read -r -p "Continue? [y/N] " answer
    [[ "$answer" == y ]] || die "aborted"
  fi
  echo
  [[ "$DRY_RUN" == 0 ]] || echo "Dry run, commands that would run:"
  release "$version"
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  main "$@"
fi
