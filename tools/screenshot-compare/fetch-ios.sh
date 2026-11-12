#!/usr/bin/env bash
# Copies the iOS run's screenshots (ios.yml's till-ios-screenshots artifact)
# for commit <sha> into <out>, for screenshots.yml's page. It waits for that
# run to finish; when the commit has no iOS run (the change didn't touch iOS
# or the core), it takes the last one on <branch> that has screenshots, else
# main's.
#
#   GH_TOKEN=… tools/screenshot-compare/fetch-ios.sh <sha> <out> [<branch>]
set -euo pipefail

sha="${1:?the commit}"
out="${2:?the folder for the screenshots}"
branch="${3:-}"
repo="${GITHUB_REPOSITORY:?the repository, owner/name}"
mkdir -p "$out"

run=""
# Up to 110 minutes: the iOS run's timeout and its queue.
for attempt in $(seq 1 110); do
    found="$(gh run list -R "$repo" --workflow ios.yml --commit "$sha" --limit 1 --json databaseId,status,conclusion \
        --jq '.[0] | select(.) | "\(.databaseId) \(.status) \(.conclusion)"')"
    if [ -z "$found" ]; then
        # A pull request's runs start together; after five minutes with none, there is none.
        if [ "$attempt" -ge 5 ]; then break; fi
    else
        read -r id status conclusion <<< "$found"
        if [ "$status" = completed ]; then
            echo "iOS run $id for $sha: $conclusion"
            run="$id"
            break
        fi
        echo "iOS run $id for $sha: $status"
    fi
    sleep 60
done

if [ -z "$run" ]; then
    for fallback in $branch main; do
        # Finished runs only, newest first; the first whose artifact is there.
        for id in $(gh run list -R "$repo" --workflow ios.yml --branch "$fallback" --status completed --limit 10 \
            --json databaseId --jq '.[].databaseId'); do
            if gh api "repos/$repo/actions/runs/$id/artifacts" --jq '.artifacts[].name' | grep -qx till-ios-screenshots; then
                run="$id"
                break 2
            fi
        done
    done
    if [ -z "$run" ]; then
        echo "::warning::No iOS screenshots: no iOS run for $sha, nor one with screenshots on ${branch:-main}"
        exit 0
    fi
    echo "::notice::No iOS run for $sha: the iOS screenshots are from run $run"
fi

work="$(mktemp -d)"
if gh run download "$run" -R "$repo" -n till-ios-screenshots -D "$work"; then
    rm -f "$work/index.html"
    cp -R "$work"/. "$out/"
else
    echo "::warning::iOS run $run has no till-ios-screenshots artifact"
fi
rm -rf "$work"
