#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Desk contributors
# SPDX-License-Identifier: GPL-3.0-or-later

# Renders the mockups with the GTK and the Libadwaita of GNOME 51, off
# screen, and joins them with the catalogue.
# Usage: ./run.sh [output dir] [scene id prefix ...]
# The scenes come out in English; DESK_MOCKUP_LANG=pt draws them in
# Portuguese, for the Portuguese page of the site.
#
# desk-gnome-51 is the wrapper that ci/setup-env.sh installs; it sees the
# home and /tmp only, so the repository has to be under one of them. The
# window that Desk follows is M+ (writer-m-plus, tables-m-plus and
# slides-m-plus, drawn by proposals.py; decision D-008). The scenes that
# start with writer-b, tables-b, slides-b and common are interface B, which
# M+ replaced; its popovers and dialogs are still drawn only there until
# the secondary scenes of M+ are drawn (backlog item B-003). writer-a,
# writer-c, the N and M scenes and the like are versions that were not
# chosen. ../components.tsv is the inventory of the M+ scenes.
set -euo pipefail
cd "$(dirname "$0")"
out=${1:-out}
shift || true
python3 make_icons.py
desk-gnome-51 env GSETTINGS_BACKEND=memory GSK_RENDERER=cairo GDK_BACKEND=x11 GDK_SCALE=2 \
    NO_AT_BRIDGE=1 LANG=pt_BR.UTF-8 LANGUAGE=pt_BR DESK_MOCKUP_LANG="${DESK_MOCKUP_LANG:-en}" dbus-run-session -- \
    xvfb-run -a -s "-screen 0 3200x2200x24" python3 render.py "$out" "$@"
if [ "$#" -eq 0 ]; then
    python3 inventory.py "$out" ../components.tsv "$out/data.json" M+
fi
