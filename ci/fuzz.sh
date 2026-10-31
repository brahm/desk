#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Desk contributors
# SPDX-License-Identifier: GPL-3.0-or-later

# Runs each fuzz target in fuzz/ for a while, starting from the seeds in
# fuzz/seeds, and stops at the first input that makes one fail. The
# failing input is left in fuzz/artifacts/<target>/.
#
# Usage: ci/fuzz.sh [SECONDS [TARGET...]]
#
# SECONDS is how long each target runs, 60 by default. It needs a nightly
# toolchain and cargo-fuzz, which ci/setup-env.sh installs.

set -euo pipefail
cd "$(dirname "$0")/../fuzz"

seconds=${1:-60}
shift || true
targets=("$@")
if [[ ${#targets[@]} -eq 0 ]]; then
    mapfile -t targets < <(cargo +nightly fuzz list)
fi

# The seeds of each target: the directory named after it, or the one of
# the target it shares an input format with.
seeds_of() {
    case $1 in
        odt_import) echo seeds/odt_streams ;;
        *) echo "seeds/$1" ;;
    esac
}

# The dictionary of the format of each target's input.
dict_of() {
    case $1 in
        pages_*) echo iwork.dict ;;
        *) echo odf.dict ;;
    esac
}

for target in "${targets[@]}"; do
    printf '\n==> Fuzzing %s for %s seconds\n' "$target" "$seconds"
    # The corpus grows as the fuzzer runs; it stays out of the repository.
    corpus=../target/fuzz-corpus/$target
    mkdir -p "$corpus"
    cargo +nightly fuzz run "$target" "$corpus" "$(seeds_of "$target")" -- \
        -dict="$(dict_of "$target")" -max_len=65536 -max_total_time="$seconds" \
        -timeout=10 -rss_limit_mb=2048 -print_final_stats=1
done
