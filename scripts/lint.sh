#!/bin/sh
nix develop --command cargo fmt --check
nix develop --command cargo clippy -- -D warnings
