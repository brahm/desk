#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Desk contributors
# SPDX-License-Identifier: GPL-3.0-or-later

# Installs everything that ci/check-all.sh needs, on Ubuntu 24.04 like the
# CI: the GTK and Libadwaita libraries, the fonts the tests measure with,
# the Poppler tools the PDF tests read back with,
# the stable Rust toolchain and the minimum supported one, cargo-deny and
# reuse. It is the setup script of the cloud environment and works on a
# developer machine too. Running it again only updates what is there.

set -euo pipefail

# Keep in sync with rust-version in Cargo.toml.
msrv=1.92

sudo=''
if [ "$(id -u)" -ne 0 ]; then
    sudo=sudo
fi

export DEBIAN_FRONTEND=noninteractive
$sudo apt-get update
$sudo apt-get install --yes --no-install-recommends \
    build-essential pkg-config curl ca-certificates git python3-pip \
    libgtk-4-dev libadwaita-1-dev libcairo2-dev libfontconfig-dev \
    fonts-liberation fonts-dejavu-core poppler-utils

if ! command -v rustup >/dev/null; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs |
        sh -s -- -y --profile minimal
fi
# shellcheck disable=SC1091
source "$HOME/.cargo/env"
rustup toolchain install stable --profile minimal --component rustfmt,clippy
rustup toolchain install "$msrv" --profile minimal

if ! command -v cargo-deny >/dev/null; then
    cargo install --locked cargo-deny
fi
if ! command -v reuse >/dev/null; then
    pip3 install --break-system-packages reuse ||
        pip3 install reuse
fi
