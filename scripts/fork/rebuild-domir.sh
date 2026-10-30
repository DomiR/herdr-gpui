#!/usr/bin/env bash
# Rebuilds the fork's `domir` branch from scratch: upstream's main with each
# branch listed in scripts/fork/branches merged in, in order, then
# force-pushes it to the fork. Nothing is ever committed on `domir` by hand,
# so it can always be thrown away and made again.
#
# Run it in the worktree that holds `domir` (or one with no local changes).
# Git's rerere replays every conflict resolution recorded on an earlier run.
# A conflict it has not seen stops the script: resolve it, `git commit
# --no-edit`, and run the script again, which then knows the answer.
# A conflicted Cargo.lock is rebuilt by Cargo rather than resolved by hand.
#
#   UPSTREAM=origin FORK=fork BRANCH=domir scripts/fork/rebuild-domir.sh
#
# The whole script is read before it runs, because checking out `domir`
# replaces this very file.
{
set -euo pipefail

upstream=${UPSTREAM:-origin}
fork=${FORK:-fork}
branch=${BRANCH:-domir}

fail() { printf 'rebuild-domir: %s\n' "$*" >&2; exit 1; }

root=$(git rev-parse --show-toplevel)
cd "$root"
[ -z "$(git status --porcelain --untracked-files=no)" ] || fail "commit or stash local changes first"
[ -z "$(git diff --name-only --diff-filter=U)" ] || fail "finish the merge in progress first"

branches=()
while IFS= read -r line; do
    line=${line%%#*}
    line=$(printf '%s' "$line" | tr -d '[:space:]')
    [ -n "$line" ] && branches+=("$line")
done < scripts/fork/branches
[ "${#branches[@]}" -gt 0 ] || fail "scripts/fork/branches lists no branches"

git fetch --quiet "$upstream"
git fetch --quiet "$fork"
for name in "${branches[@]}"; do
    git rev-parse --verify --quiet "refs/remotes/$fork/$name" >/dev/null \
        || fail "$fork/$name does not exist; push it first"
done

git checkout --quiet -B "$branch" "$upstream/main"
for name in "${branches[@]}"; do
    printf 'Merging %s\n' "$fork/$name"
    if git -c rerere.enabled=true -c rerere.autoUpdate=true \
        merge --no-ff --no-edit --quiet "$fork/$name"; then
        continue
    fi
    # Cargo writes the lockfile both sides agree on, from the merged
    # manifests.
    if git diff --name-only --diff-filter=U | grep -qx Cargo.lock; then
        git checkout --quiet --ours Cargo.lock
        cargo metadata --format-version 1 >/dev/null
        git add Cargo.lock
    fi
    conflicted=$(git diff --name-only --diff-filter=U)
    if [ -n "$conflicted" ]; then
        printf 'Conflicts merging %s:\n%s\n' "$fork/$name" "$conflicted" >&2
        fail "resolve them, run 'git commit --no-edit', and run this again"
    fi
    git commit --quiet --no-edit
done

git push --quiet --force-with-lease "$fork" "$branch"
printf '%s rebuilt from %s/main and %d branches, pushed to %s\n' \
    "$branch" "$upstream" "${#branches[@]}" "$fork"
exit
}
