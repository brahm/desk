# SPDX-FileCopyrightText: 2026 Desk contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Draws the mockups in another language, for the translations of the site.

The scenes are written in English. With DESK_MOCKUP_LANG=pt, render.py calls
install('pt_BR.tsv') before it imports the scene modules, and every string
constant of those modules that the table knows is replaced by its
translation as the module is compiled. Strings that the table does not know
(identifiers, CSS, icon names) are left alone, so a missing translation
shows as English text in the picture.

The table has three tab-separated columns: the English text, its
translation, and an optional context. An English word can need two
translations (None is Nenhum or Nenhuma in Portuguese, after the gender of
the noun it stands for), so a row with a context, `module` or
`module.function`, wins over the row without one inside that module or
that top-level function. A newline in a text is written as \\n.

Numbers are not in the table: 2,400, 3,474.83 and 125% are written the way
the language writes them (2.400, 3.474,83 and 125 % in Portuguese).
"""
import ast
import importlib.abc
import importlib.machinery
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
MODULES = ('writer', 'tables', 'slides', 'parts', 'shell', 'canvas', 'extras', 'proposals')

# A number written the English way: 2,400, 3,474.83, +11.7%, 125%.
_NUMBER = re.compile(r'^[+-]?(\d{1,3}(,\d{3})+|\d+)(\.\d+)?%?$')


def load(path):
    """The table as {(context, English): translation}; '' is no context."""
    table = {}
    with open(path, encoding='utf-8') as f:
        for n, line in enumerate(f, 1):
            line = line.rstrip('\n')
            if not line or line.startswith('#'):
                continue
            parts = line.split('\t')
            if len(parts) != 3:
                sys.exit(f'{path}:{n}: {len(parts)} columns, expected 3')
            english, text, context = (p.replace('\\n', '\n') for p in parts)
            table[(context, english)] = text
    return table


def _number(text):
    """The number in the Brazilian notation, or None when text is not one.

    A string that looks like a number but is not one in the picture, such
    as the version '4.0' of gi.require_version, has a row in the table that
    keeps it as it is.
    """
    if not _NUMBER.match(text) or text.isdigit():
        return None
    swapped = text.translate(str.maketrans('.,', ',.'))
    return swapped[:-1] + ' %' if swapped.endswith('%') else swapped


class _Translate(ast.NodeTransformer):
    """Replaces each string constant that the table knows by its translation."""

    def __init__(self, table, module):
        self.table = table
        self.module = module
        self.function = ''

    def visit_FunctionDef(self, node):
        top = not self.function
        if top:
            self.function = node.name
        self.generic_visit(node)
        if top:
            self.function = ''
        return node

    def visit_Constant(self, node):
        if not isinstance(node.value, str):
            return node
        t = self.table
        text = (t.get((f'{self.module}.{self.function}', node.value)) if self.function else None) \
            or t.get((self.module, node.value)) or t.get(('', node.value)) or _number(node.value)
        if text:
            return ast.copy_location(ast.Constant(text), node)
        return node


class _Loader(importlib.machinery.SourceFileLoader):
    table = {}

    def get_code(self, fullname):
        # Always from the source: a cached .pyc would hold the English text.
        path = self.get_filename(fullname)
        return self.source_to_code(self.get_data(path), path)

    def source_to_code(self, data, path, *, _optimize=-1):
        module = os.path.splitext(os.path.basename(path))[0]
        tree = _Translate(self.table, module).visit(ast.parse(data, path))
        return compile(ast.fix_missing_locations(tree), path, 'exec', dont_inherit=True,
                       optimize=_optimize)


class _Finder(importlib.abc.MetaPathFinder):
    def find_spec(self, name, path=None, target=None):
        if name not in MODULES:
            return None
        file = os.path.join(HERE, name + '.py')
        return importlib.machinery.ModuleSpec(name, _Loader(name, file), origin=file)


def install(table_file):
    """Makes the scene modules load translated. Call before importing them."""
    _Loader.table = load(os.path.join(HERE, table_file))
    sys.dont_write_bytecode = True      # the cache would hold the other language
    sys.meta_path.insert(0, _Finder())
