#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Desk contributors
# SPDX-License-Identifier: GPL-3.0-or-later

# Assembles the website published on GitHub Pages into one directory
# (target/site by default): the pages in site/, plus the logo and the app
# icons copied from data/, so that the images have one source.
#
# The Flatpak repository is published on the same site, under paths that
# the pages must never use; site/README.md lists them.

set -euo pipefail
cd "$(dirname "$0")/.."

out=${1:-target/site}

rm -rf "$out"
mkdir -p "$out/img"

cp site/index.html site/style.css "$out/"
cp data/logo/desk.svg data/logo/desk-logo.svg data/logo/desk-logo-dark.svg \
    data/logo/desk-logo.png "$out/img/"
cp data/icons/hicolor/scalable/apps/br.com.brahm.Desk.*.svg "$out/img/"

# The site is plain files: tell GitHub Pages not to run Jekyll on it.
touch "$out/.nojekyll"

# The pages must not claim the paths of the Flatpak repository.
for reserved in repo flatpak; do
    if [[ -e "$out/$reserved" ]]; then
        echo "error: $out/$reserved is reserved for the Flatpak repository" >&2
        exit 1
    fi
done
if compgen -G "$out/*.flatpakrepo" >/dev/null || compgen -G "$out/*.flatpakref" >/dev/null; then
    echo "error: .flatpakrepo and .flatpakref files belong to the Flatpak repository" >&2
    exit 1
fi

echo "Site assembled in $out"
