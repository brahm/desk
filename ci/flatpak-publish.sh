#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Desk contributors
# SPDX-License-Identifier: GPL-3.0-or-later

# Adds a build of Desk to the Flatpak repository that GitHub Pages
# publishes, and writes the files that install from it.
#
#   ci/flatpak-publish.sh BUILD-REPO SITE-DIR CHANNEL GPG-KEY-ID
#
# BUILD-REPO is the OSTree repository that flatpak-builder exported the
# build to. SITE-DIR holds what goes to the root of the site, as kept
# from the last run, or nothing on the first: the signed repository in
# repo/, the public key in flatpak/desk.gpg, desk.flatpakrepo and a
# .flatpakref for each channel. CHANNEL is the branch of the build:
# nightly or beta (stable from 1.0). The GPG key, already imported, signs
# the commits and the summary of the repository; site/README.md says
# how it is kept.

set -euo pipefail

if [[ $# -ne 4 ]]; then
    echo "usage: $0 BUILD-REPO SITE-DIR CHANNEL GPG-KEY-ID" >&2
    exit 2
fi
build=$1
site=$2
channel=$3
key=$4

case "$channel" in
    nightly | beta | stable) ;;
    *)
        echo "error: unknown channel $channel" >&2
        exit 2
        ;;
esac

url=${DESK_SITE_URL:-https://brahm.github.io/desk}
app=br.com.brahm.Desk.Writer
repo=$site/repo

mkdir -p "$site/flatpak"
if [[ ! -d "$repo/objects" ]]; then
    ostree init --mode=archive-z2 --repo="$repo"
fi

# The app and its locale data; the debug symbols stay out, to keep the
# site under the 1 GB that GitHub Pages allows.
mapfile -t refs < <(ostree refs --repo="$build" |
    grep -E "^(app|runtime)/$app(\.Locale)?/[^/]+/$channel\$" || true)
if [[ ${#refs[@]} -eq 0 ]]; then
    echo "error: $build has no $channel build of $app" >&2
    exit 1
fi
for ref in "${refs[@]}"; do
    flatpak build-commit-from --src-repo="$build" --gpg-sign="$key" \
        --no-update-summary "$repo" "$ref"
done

# Each branch keeps its last five builds, so that an update can go back
# one and the nightly builds do not fill the site.
flatpak build-update-repo --gpg-sign="$key" --prune --prune-depth=5 \
    --generate-static-deltas --static-delta-ignore-ref='*.Debug' \
    --title="Desk" --comment="The Desk office suite for GNOME" \
    --homepage="$url/" --icon="$url/img/desk.svg" \
    --default-branch=beta "$repo"

gpg --export "$key" >"$site/flatpak/desk.gpg"
gpgkey=$(base64 -w0 "$site/flatpak/desk.gpg")

cat >"$site/desk.flatpakrepo" <<REPO
[Flatpak Repo]
Title=Desk
Url=$url/repo/
Homepage=$url/
Comment=The Desk office suite for GNOME
Description=Desk Writer, Desk Tables and Desk Slides, from the Desk project. The beta branch has the test versions of each phase, and the nightly branch is built from the latest code every night.
Icon=$url/img/desk.svg
DefaultBranch=beta
GPGKey=$gpgkey
REPO

# One .flatpakref for each branch the repository has, to install the
# app with one click.
for ref in $(ostree refs --repo="$repo" | grep -E "^app/$app/[^/]+/" | sort -u); do
    branch=${ref##*/}
    title="Desk Writer"
    [[ $branch == stable ]] || title="Desk Writer ($branch)"
    cat >"$site/desk-writer-$branch.flatpakref" <<REF
[Flatpak Ref]
Name=$app
Branch=$branch
Title=$title
Url=$url/repo/
SuggestRemoteName=desk
Homepage=$url/
Icon=$url/img/br.com.brahm.Desk.Writer.svg
RuntimeRepo=https://dl.flathub.org/repo/flathub.flatpakrepo
IsRuntime=false
GPGKey=$gpgkey
REF
done

echo "Published the $channel build of $app in $repo"
