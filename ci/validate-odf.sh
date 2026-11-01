#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Desk contributors
# SPDX-License-Identifier: GPL-3.0-or-later

# Validates the ODF files that Desk writes against the OASIS ODF 1.3
# RelaxNG schemas, with Jing. The files come from the tests, which write
# their documents to a directory when DESK_ODF_OUT names one, from
# converting the seed documents of the fuzzer, and from converting the
# documents of Pages in the fixtures.
#
# The schemas come from the ODF Toolkit validator on Maven Central, at a
# pinned version and checksum, since the OASIS site rate limits and
# blocks automated downloads. Only the schema files are taken from it;
# nothing in it runs. It needs Jing, curl and unzip, which
# ci/setup-env.sh installs.

set -euo pipefail
cd "$(dirname "$0")/.."

validator=odfvalidator-0.13.0-jar-with-dependencies.jar
validator_url=https://repo1.maven.org/maven2/org/odftoolkit/odfvalidator/0.13.0/$validator
validator_sha256=5684feec5cbdcd5783998978c096ac9ccea53a454e2d6ae803ce482d2336d1dc
schema_sha256='40bad03efdbb02825230d357da0aa6ac679934c5bf56c6281752c0c24d58e4e6  OpenDocument-v1.3-schema.rng
8aee71f03484be112af972d622cc9031280c007b0a454ae8815e8e333c9bdd17  OpenDocument-v1.3-manifest-schema.rng'

schemas=target/odf-schema/1.3
if ! (cd "$schemas" 2>/dev/null && sha256sum --check --quiet <<<"$schema_sha256"); then
    echo "Fetching the ODF 1.3 schemas"
    download=$(mktemp -d)
    trap 'rm -rf "$download"' EXIT
    curl --fail --silent --show-error --location --retry 3 \
        --output "$download/$validator" "$validator_url"
    sha256sum --check --quiet <<<"$validator_sha256  $download/$validator"
    mkdir -p "$schemas"
    unzip -q -o -j "$download/$validator" \
        schema/odf1.3/OpenDocument-v1.3-schema.rng \
        schema/odf1.3/OpenDocument-v1.3-manifest-schema.rng -d "$schemas"
    (cd "$schemas" && sha256sum --check --quiet <<<"$schema_sha256")
fi

out=target/odf-validate
rm -rf "$out"
mkdir -p "$out/files" "$out/unpacked"
echo "Writing the documents of the tests"
DESK_ODF_OUT=$PWD/$out/files cargo test --locked --quiet \
    --package desk-odf --package desk-filters >/dev/null
cargo build --locked --quiet --package desk-convert
for seed in fuzz/seeds/odt_package/*.odt; do
    target/debug/desk-convert "$seed" "$out/files/converted-$(basename "$seed")"
done
# Every document that Pages saved in the fixtures, converted to ODT.
for pages in corpus/fixtures/pages/*.pages corpus/fixtures/pages/*.template; do
    name=$(basename "$pages")
    target/debug/desk-convert "$pages" "$out/files/pages-${name%.*}.odt" 2>/dev/null
done

status=0
count=0
for file in "$out"/files/*.odt; do
    dir=$out/unpacked/$(basename "$file" .odt)
    mkdir -p "$dir"
    unzip -q -o "$file" -d "$dir"
    # -i: the ODF schemas are not compatible with DTD ID types, as the
    # OASIS validator also assumes.
    if ! jing -i "$schemas/OpenDocument-v1.3-schema.rng" \
        "$dir/content.xml" "$dir/styles.xml" "$dir/meta.xml" ||
        ! jing -i "$schemas/OpenDocument-v1.3-manifest-schema.rng" \
            "$dir/META-INF/manifest.xml"; then
        echo "error: $(basename "$file") is not valid ODF 1.3" >&2
        status=1
    fi
    count=$((count + 1))
done
if [[ $count -eq 0 ]]; then
    echo "error: no documents were written to validate" >&2
    exit 1
fi
if [[ $status -eq 0 ]]; then
    echo "All $count documents are valid ODF 1.3."
fi
exit "$status"
