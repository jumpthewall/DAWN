#!/bin/sh
set -e

version=$(grep -m1 '^version' Cargo.toml | sed 's/.*"\(.*\)"/\1/')
tag="v$version"

if git rev-parse "$tag" >/dev/null 2>&1; then
    echo "Tag $tag already exists"
    exit 1
fi

git tag "$tag"
git push origin "$tag"

echo "Tagged and pushed $tag"
