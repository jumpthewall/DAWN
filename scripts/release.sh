#!/usr/bin/env bash
set -e

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
version=$(nix run nixpkgs#python3 -- "$script_dir/get_version.py")
tag="v$version"

: "${GIT_REMOTE:=origin}"
remote="${1:-$GIT_REMOTE}"

if ! git remote get-url "$remote" >/dev/null 2>&1; then
    echo "Error: git remote '$remote' does not exist." >&2
    echo "Please specify a valid remote as the first argument or via GIT_REMOTE." >&2
    exit 1
fi

if git rev-parse "$tag" >/dev/null 2>&1; then
    echo "Tag $tag already exists"
    exit 1
fi

git tag "$tag"
git push "$remote" "$tag"

echo "Tagged and pushed $tag to $remote"
