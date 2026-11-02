#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Desk contributors
# SPDX-License-Identifier: GPL-3.0-or-later

# Runs the UI tests of tests/ui, which drive the apps and read them
# through the accessibility tree, on a virtual display with its own
# session and accessibility buses. It needs Xvfb, xdotool, D-Bus, AT-SPI
# and PyGObject, which ci/setup-env.sh installs.

set -euo pipefail
cd "$(dirname "$0")/.."

cargo build --locked --quiet --package desk-writer --package desk-convert

# A Python of the system whose PyGObject reads the AT-SPI typelib.
python=
for candidate in /usr/bin/python3 /usr/bin/python3.*[0-9]; do
    if "$candidate" -I -c 'import gi' 2>/dev/null; then
        python=$candidate
        break
    fi
done
if [[ -z $python ]]; then
    echo "No Python with PyGObject found (python3-gi)" >&2
    exit 1
fi
launcher=
for candidate in /usr/libexec/at-spi-bus-launcher /usr/lib/at-spi2-core/at-spi-bus-launcher; do
    [[ -x $candidate ]] && launcher=$candidate && break
done
if [[ -z $launcher ]]; then
    echo "The AT-SPI bus launcher is missing (at-spi2-core)" >&2
    exit 1
fi

export GDK_BACKEND=x11 GSK_RENDERER=cairo LC_ALL=C.UTF-8 LANG=C.UTF-8
export GTK_A11Y=atspi NO_AT_BRIDGE=0
# Keep the settings of the runner out of the tests.
export XDG_CONFIG_HOME XDG_DATA_HOME
XDG_CONFIG_HOME=$(mktemp -d)
XDG_DATA_HOME=$(mktemp -d)

xvfb-run --auto-servernum --server-args='-screen 0 1280x900x24' \
    dbus-run-session -- bash -c "
        '$launcher' --launch-immediately &
        sleep 1
        status=0
        for test in tests/ui/*.py; do
            echo \"==> \$test\"
            '$python' -I \"\$test\" || status=1
        done
        exit \$status
    "
