#!/bin/sh
nix build .#release
curl -F "file=@result/dawn-bundle.zip" https://temp.sh/upload
echo
