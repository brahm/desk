#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Desk contributors
# SPDX-License-Identifier: GPL-3.0-or-later

# Extracts the strings to translate from the code into po/desk.pot and
# merges them into the translations listed in po/LINGUAS. With --check,
# changes nothing and fails when a string of the code is missing from
# the template, so that CI catches strings that were not extracted.
# It needs the gettext tools, which ci/setup-env.sh installs.

set -euo pipefail
cd "$(dirname "$0")/.."

extract() {
    # Rust strings read as C strings, once the lifetimes, which C would
    # take for character constants, are left out of copies of the files.
    # The markers are those of crates/desk-ui/src/i18n.rs.
    local copies output
    copies=$(mktemp -d)
    output=$(realpath "$1")
    for file in crates/desk-ui/src/*.rs crates/desk-writer/src/*.rs \
        crates/desk-tables/src/*.rs crates/desk-slides/src/*.rs; do
        mkdir -p "$copies/$(dirname "$file")"
        sed -E "s/'(static|[a-z])\b([^']|$)/\2/g" "$file" >"$copies/$file"
    done
    (
        cd "$copies"
        find crates -name '*.rs' -print0 | sort -z | xargs -0 \
            xgettext --from-code=UTF-8 --language=C --sort-by-file \
            --keyword= --keyword=tr --keyword=trn:1,2 --keyword=trc:1c,2 --keyword=n_ \
            --add-comments=Translators: --package-name=Desk \
            --msgid-bugs-address=https://github.com/brahm/desk/issues \
            --output="$output"
    )
    rm -rf "$copies"
}

if [[ ${1:-} == --check ]]; then
    fresh=$(mktemp)
    trap 'rm -f "$fresh"' EXIT
    extract "$fresh"
    if ! diff <(msgcat --no-location --sort-output "$fresh" | grep -v POT-Creation-Date) \
        <(msgcat --no-location --sort-output po/desk.pot | grep -v POT-Creation-Date) >/dev/null; then
        echo "po/desk.pot is out of date; run ci/update-translations.sh" >&2
        exit 1
    fi
    while read -r language; do
        [[ -z $language || $language == \#* ]] && continue
        msgfmt --check --output-file=/dev/null "po/$language.po"
        echo "$language: $(msgfmt --statistics --output-file=/dev/null "po/$language.po" 2>&1)"
    done <po/LINGUAS
    exit 0
fi

extract po/desk.pot
while read -r language; do
    [[ -z $language || $language == \#* ]] && continue
    if [[ -f po/$language.po ]]; then
        msgmerge --quiet --update --backup=none "po/$language.po" po/desk.pot
    else
        msginit --no-translator --locale="$language.UTF-8" --input=po/desk.pot \
            --output-file="po/$language.po"
    fi
done <po/LINGUAS
