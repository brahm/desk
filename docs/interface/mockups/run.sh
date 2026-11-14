#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Desk contributors
# SPDX-License-Identifier: GPL-3.0-or-later

# Renders the mockups with the GTK and the Libadwaita of GNOME 51, off
# screen, and joins them with the catalogue.
# Usage: ./run.sh [output dir] [scene id prefix ...]
#
# desk-gnome-51 is the wrapper that ci/setup-env.sh installs; it sees the
# home and /tmp only, so the repository has to be under one of them. The
# scenes of the interface that Desk follows start with writer-b, tables-b,
# slides-b and comum; writer-a, writer-c and the like are the two versions
# that were not chosen.
set -euo pipefail
cd "$(dirname "$0")"
out=${1:-out}
shift || true
python3 make_icons.py
desk-gnome-51 env GSETTINGS_BACKEND=memory GSK_RENDERER=cairo GDK_BACKEND=x11 GDK_SCALE=2 \
    NO_AT_BRIDGE=1 LANG=pt_BR.UTF-8 LANGUAGE=pt_BR dbus-run-session -- \
    xvfb-run -a -s "-screen 0 3200x2200x24" python3 render.py "$out" "$@"
if [ "$#" -eq 0 ]; then
    python3 inventory.py "$out" ../componentes.tsv "$out/data.json" B
fi
