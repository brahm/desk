#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Desk contributors
# SPDX-License-Identifier: GPL-3.0-or-later

# Runs every check of the CI on the working tree, in the same order, and
# stops at the first that fails. Run it before opening or updating a pull
# request; ci/setup-env.sh installs what it needs.

set -euo pipefail
cd "$(dirname "$0")/.."

# Keep in sync with rust-version in Cargo.toml.
msrv=1.92

step() {
    printf '\n==> %s\n' "$1"
}

step 'Formatting'
cargo fmt --all --check
step 'Clippy'
cargo clippy --workspace --all-targets --locked -- --deny warnings
step 'Tests'
cargo test --workspace --locked
step 'Documentation'
RUSTDOCFLAGS='--deny warnings' cargo doc --workspace --no-deps --locked
step 'Dependency rules'
ci/check-deps.sh
step "Minimum Rust version ($msrv)"
cargo "+$msrv" check --workspace --all-targets --locked
step 'Licenses and advisories of dependencies'
cargo deny check
step 'REUSE compliance'
reuse lint

printf '\nAll checks passed.\n'
