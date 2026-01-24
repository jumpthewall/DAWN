#!/bin/sh
nix build .#fullBundle
curl -F "file=@result/dawn-bundle.zip" https://temp.sh/upload
