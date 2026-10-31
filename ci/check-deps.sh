#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Desk contributors
# SPDX-License-Identifier: GPL-3.0-or-later

# Checks the dependency rules of the workspace that Cargo cannot express:
#
# - The format libraries are MPL-2.0 so that other programs can reuse them,
#   so they must not depend on the other Desk crates, which are
#   GPL-3.0-or-later.
# - Only desk-ui and the apps may depend on GTK, so that the converter and
#   the tests run without a display.

set -euo pipefail
cd "$(dirname "$0")/.."

mpl_crates=(desk-odf desk-iwork desk-formula)
gtk_crates=(desk-ui desk-writer desk-tables desk-slides)
gtk_packages='^(gdk4|gdk4-sys|gsk4|gsk4-sys|gtk4|gtk4-sys|libadwaita|libadwaita-sys)$'

# Prints the names of the normal and build dependencies of a crate, the
# crate itself included, one per line.
dependencies() {
    cargo tree --locked --package "$1" --edges normal,build \
        --prefix none --format '{p}' | awk '{ print $1 }' | sort -u
}

status=0

for crate in "${mpl_crates[@]}"; do
    found=$(dependencies "$crate" | grep '^desk-' | grep -vx "$crate" || true)
    if [[ -n $found ]]; then
        echo "error: $crate is MPL-2.0 and must not depend on other Desk crates:" $found >&2
        status=1
    fi
done

for dir in crates/*/; do
    crate=$(basename "$dir")
    if [[ " ${gtk_crates[*]} " == *" $crate "* ]]; then
        continue
    fi
    found=$(dependencies "$crate" | grep -E "$gtk_packages" || true)
    if [[ -n $found ]]; then
        echo "error: only desk-ui and the apps may depend on GTK, but $crate depends on:" $found >&2
        status=1
    fi
done

if [[ $status -eq 0 ]]; then
    echo "Dependency rules hold for all crates."
fi
exit "$status"
