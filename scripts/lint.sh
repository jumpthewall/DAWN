#!/bin/sh
nix develop --command cargo fmt
nix develop --command cargo clippy --
