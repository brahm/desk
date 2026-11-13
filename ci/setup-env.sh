#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Desk contributors
# SPDX-License-Identifier: GPL-3.0-or-later

# Installs everything that ci/check-all.sh needs: the GTK and Libadwaita
# libraries of GNOME 51, which the Flatpak of Desk runs on, the fonts the
# tests measure with, the Poppler tools the PDF tests read back with, Jing
# for validating ODF, the stable Rust toolchain and the minimum supported
# one, a nightly toolchain and cargo-fuzz for the fuzz targets, cargo-deny
# and reuse. It is the setup script of the cloud environment and works on
# a developer machine too. Running it again only updates what is there.
#
# GNOME 51 has GTK 4.24 and Libadwaita 1.10, which Ubuntu has from 26.10
# on; the CI runs in that release. On an older Ubuntu, such as the 24.04
# of the cloud environment, the script puts Ubuntu 26.10 with those
# libraries in /opt/gnome-51 and installs desk-gnome-51, which runs a
# command there with the Rust toolchains and the files of this machine:
#
#     desk-gnome-51 ci/check-all.sh

set -euo pipefail

# Keep in sync with rust-version in Cargo.toml.
msrv=1.100
# The GTK of the GNOME 51 runtime; keep in sync with the features of gtk
# in Cargo.toml.
gtk_version=4.24
# Ubuntu 26.10, the first release with that GTK.
release=stonking
root=/opt/gnome-51

sudo=''
if [ "$(id -u)" -ne 0 ]; then
    sudo=sudo
fi

export DEBIAN_FRONTEND=noninteractive
packages=(
    build-essential pkg-config curl ca-certificates git python3-pip
    libgtk-4-dev libadwaita-1-dev libcairo2-dev libfontconfig-dev libpoppler-glib-dev
    fonts-liberation fonts-dejavu-core fonts-texgyre fonts-urw-base35
    fonts-crosextra-carlito fonts-crosextra-caladea poppler-utils jing unzip
    xvfb xauth xdotool dbus libglib2.0-bin at-spi2-core python3-gi gir1.2-atspi-2.0 gettext
    libgtk-4-media-gstreamer gstreamer1.0-plugins-good gstreamer1.0-libav
    desktop-file-utils appstream
    hunspell-en-us hunspell-pt-br hyphen-en-us hyphen-pt-br
)

# Writes desk-gnome-51, which runs a command in $root.
write_wrapper() {
    cat <<WRAPPER
#!/usr/bin/env bash
# Runs a command in $root, an Ubuntu 26.10 with the libraries of GNOME 51,
# in the current directory, with the home and the temporary files of this
# machine. Made by ci/setup-env.sh of Desk.
set -euo pipefail
root=$root
sudo=''
if [ "\$(id -u)" -ne 0 ]; then
    sudo=sudo
fi
for dir in /proc /dev /sys "\$HOME" /tmp; do
    if ! mountpoint -q "\$root\$dir"; then
        \$sudo mkdir -p "\$root\$dir"
        if [ "\$dir" = /proc ]; then
            \$sudo mount -t proc proc "\$root/proc"
        else
            \$sudo mount --rbind "\$dir" "\$root\$dir"
        fi
    fi
done
\$sudo cp /etc/resolv.conf "\$root/etc/resolv.conf"
exec \$sudo chroot "\$root" /usr/bin/env HOME="\$HOME" \\
    PATH="\$HOME/.cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin" \\
    bash -c 'cd "\$1" && shift && exec "\$@"' bash "\$PWD" "\$@"
WRAPPER
}

$sudo apt-get update
candidate=$(apt-cache policy libgtk-4-dev | sed -n 's/^ *Candidate: //p')
if dpkg --compare-versions "${candidate:-0}" ge "$gtk_version"; then
    $sudo apt-get install --yes --no-install-recommends "${packages[@]}"
else
    $sudo apt-get install --yes --no-install-recommends debootstrap curl ca-certificates python3-pip
    if [ ! -x "$root/usr/bin/dpkg" ]; then
        # Every recent Ubuntu release uses the script of gutsy. A package
        # that fails to configure here is fixed by apt-get below.
        $sudo debootstrap --variant=minbase --components=main,universe \
            "$release" "$root" http://archive.ubuntu.com/ubuntu \
            /usr/share/debootstrap/scripts/gutsy || true
    fi
    echo "deb http://archive.ubuntu.com/ubuntu $release main universe" |
        $sudo tee "$root/etc/apt/sources.list" >/dev/null
    write_wrapper | $sudo tee /usr/local/bin/desk-gnome-51 >/dev/null
    $sudo chmod +x /usr/local/bin/desk-gnome-51
    desk-gnome-51 apt-get update
    desk-gnome-51 apt-get install --yes --fix-broken --no-install-recommends "${packages[@]}"
fi

if ! command -v rustup >/dev/null; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs |
        sh -s -- -y --profile minimal
fi
# shellcheck disable=SC1091
source "$HOME/.cargo/env"
rustup toolchain install stable --profile minimal --component rustfmt,clippy
rustup toolchain install "$msrv" --profile minimal
rustup toolchain install nightly --profile minimal

if ! command -v cargo-deny >/dev/null; then
    cargo install --locked cargo-deny
fi
if ! command -v cargo-fuzz >/dev/null; then
    cargo install --locked cargo-fuzz
fi
if ! command -v reuse >/dev/null; then
    pip3 install --break-system-packages reuse ||
        pip3 install reuse
fi
