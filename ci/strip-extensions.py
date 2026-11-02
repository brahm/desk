# SPDX-FileCopyrightText: 2026 Desk contributors
# SPDX-License-Identifier: GPL-3.0-or-later

"""Removes the desk: extensions from ODF streams, in place.

ODF 1.3 calls a document with foreign elements and attributes an extended
document, and asks that it conform to the schema once they are removed
(part 3, section 3.17). Desk keeps what ODF cannot hold in the desk:
namespace, so ci/validate-odf.sh checks the streams without it. Any other
foreign namespace is left in, so that the schema refuses it.

Usage: strip-extensions.py STREAM...
"""

import sys
import xml.etree.ElementTree as ET

DESK = "https://brahm.com.br/ns/desk/1.0"
PREFIX = "{" + DESK + "}"


def strip(element):
    for name in [n for n in element.attrib if n.startswith(PREFIX)]:
        del element.attrib[name]
    for child in list(element):
        if child.tag.startswith(PREFIX):
            element.remove(child)
        else:
            strip(child)


def main(paths):
    for path in paths:
        # Keep the prefixes of the streams, for readable errors.
        for _, (prefix, uri) in ET.iterparse(path, events=["start-ns"]):
            if uri != DESK:
                ET.register_namespace(prefix, uri)
        tree = ET.parse(path)
        strip(tree.getroot())
        tree.write(path, encoding="UTF-8", xml_declaration=True)


if __name__ == "__main__":
    main(sys.argv[1:])
