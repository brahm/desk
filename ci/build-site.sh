#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Desk contributors
# SPDX-License-Identifier: GPL-3.0-or-later

# Assembles the website published on GitHub Pages into one directory
# (target/site by default): the pages in site/, plus the logo and the app
# icons copied from data/ and the English mockups of the window copied
# from docs/interface/proposals/, so that the images have one source, and the
# progress dashboard that ci/dashboard.py generates in painel/.
#
# The Flatpak repository is published on the same site, under paths that
# the pages must never use; site/README.md lists them.

set -euo pipefail
cd "$(dirname "$0")/.."

out=${1:-target/site}

rm -rf "$out"
mkdir -p "$out/img"

cp site/index.html site/style.css "$out/"
# One directory per translation, named by its language code. A translation
# may have an img/ of its own, with the mockups drawn in its language.
for page in site/*/index.html; do
    lang=$(basename "$(dirname "$page")")
    mkdir -p "$out/$lang"
    cp "$page" "$out/$lang/"
    if [[ -d "site/$lang/img" ]]; then
        cp -r "site/$lang/img" "$out/$lang/"
    fi
done
cp data/logo/desk.svg data/logo/desk-logo.svg data/logo/desk-logo-dark.svg \
    data/logo/desk-logo.png "$out/img/"
cp data/icons/hicolor/scalable/apps/br.com.brahm.Desk.*.svg "$out/img/"
# The mockups of the chosen window (M+, D-008), from the docs, for the
# same reason.
mkdir -p "$out/img/interface"
cp docs/interface/proposals/writer-m-plus.png docs/interface/proposals/tables-m-plus.png \
    docs/interface/proposals/slides-m-plus.png "$out/img/interface/"

# The progress dashboard, generated from the inventory, the roadmap and
# the backlog (docs/dashboard/README.md), so that it is current on every
# publication.
python3 ci/dashboard.py --out "$out/painel/index.html"

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
