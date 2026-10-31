#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Desk contributors
# SPDX-License-Identifier: GPL-3.0-or-later

# Runs every check of the CI on the working tree, in the same order, and
# stops at the first that fails. Run it before opening or updating a pull
# request; ci/setup-env.sh installs what it needs.
#
# The fuzz targets run for FUZZ_SECONDS each, 30 by default; the CI runs
# them for longer.

set -euo pipefail
cd "$(dirname "$0")/.."

# Keep in sync with rust-version in Cargo.toml.
msrv=1.92

step() {
    printf '\n==> %s\n' "$1"
}

step 'Formatting'
cargo fmt --all --check
cargo fmt --manifest-path fuzz/Cargo.toml --check
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
step 'ODF validation'
ci/validate-odf.sh
step 'Fuzzing'
ci/fuzz.sh "${FUZZ_SECONDS:-30}"
step 'Licenses and advisories of dependencies'
cargo deny check
step 'REUSE compliance'
reuse lint

printf '\nAll checks passed.\n'
