# SPDX-FileCopyrightText: 2026 Desk contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Pieces the three applications share: menus, popovers, Inspector pages."""
import gi

gi.require_version('Gtk', '4.0')
gi.require_version('Adw', '1')
from gi.repository import Adw, Gtk  # noqa: E402

import canvas
from kit import (action_row, button_row, classes, color_box, combo_row, dropdown, expander_row,
                 expand, group, hbox, icon_button, inline_switcher, label, menu_button, menu_model,
                 page, scale_row, spacer, spin_row, split_button, swatch, switch_row, tag,
                 text_button, toggle_group, vbox, view_stack, vsep)

APPS = {
    'writer': {'name': 'Desk Writer', 'doc': 'Relatório anual 2026', 'ext': 'odt'},
    'tables': {'name': 'Desk Tables', 'doc': 'Orçamento 2027', 'ext': 'ods'},
    'slides': {'name': 'Desk Slides', 'doc': 'Plano de lançamento', 'ext': 'odp'},
}

# ------------------------------------------------------------------- menus


def main_menu(app, lean=False):
    """The primary menu: the file and the application, twelve items at most.
    What iWork kept in the menu bar and is not here is found by Search Commands."""
    name = APPS[app]['name']
    export = {'writer': ['PDF…', 'Word…', 'RTF…', 'Texto simples…', 'EPUB…'],
              'tables': ['PDF…', 'Excel…', 'CSV…'],
              'slides': ['PDF…', 'PowerPoint…', 'Imagens…', 'Filme…', 'HTML…']}[app]
    recent = ('Abrir recente', [['Relatório anual 2026', 'Ata de março', 'Proposta'], ['Limpar a lista']])
    files = ['Salvar como…', ('Exportar', [export, ['Enviar por e-mail…']]), 'Imprimir…']
    if lean:
        view = {'writer': ['Régua', 'Invisíveis', 'Layout', 'Comentários', 'Contagem de palavras'],
                'tables': ['Réguas', 'Barra de fórmulas', 'Comentários', 'Lista de fórmulas'],
                'slides': ['Réguas', 'Notas do apresentador', 'Comentários', 'Mesa de luz', 'Tópicos']}[app]
        sections = [['Novo documento', 'Abrir…', recent], files,
                    [('Visualização', [view, ['Tela cheia']]), 'Buscar comandos…']]
    else:
        sections = [['Novo documento', 'Abrir…', recent], ['Salvar'] + files, ['Buscar comandos…']]
    sections.append(['Preferências', 'Atalhos de teclado', 'Ajuda', f'Sobre o {name}'])
    return menu_model(*sections)


def view_menu(app):
    """The View menu of iWork, as the secondary menu of the window."""
    if app == 'writer':
        return menu_model(
            [{'title': 'Barra lateral'}, 'Miniaturas das páginas', 'Estilos', 'Comentários', 'Resultados da busca'],
            ['Régua', 'Barra de formatação', 'Layout', 'Invisíveis', 'Controle de alterações'],
            ['Modo de tópicos', 'Tela cheia'],
            [('Zoom', [['Ampliar', 'Reduzir', 'Tamanho real'], ['Ajustar à largura', 'Ajustar à página'],
                       ['Uma página', 'Duas páginas']])])
    if app == 'tables':
        return menu_model(
            [{'title': 'Barra lateral'}, 'Planilhas', 'Estilos de tabela', 'Comentários', 'Resultados da busca'],
            ['Réguas', 'Barra de formatação', 'Barra de fórmulas', 'Lista de fórmulas'],
            ['Visualização de impressão', 'Tela cheia'],
            [('Zoom', [['Ampliar', 'Reduzir', 'Tamanho real'], ['Ajustar à janela']])])
    return menu_model(
        [{'title': 'Modo'}, 'Navegador', 'Tópicos', 'Só o slide', 'Mesa de luz'],
        ['Slides mestres', 'Notas do apresentador', 'Réguas', 'Barra de formatação', 'Comentários'],
        ['Tela cheia'],
        [('Zoom', [['Ampliar', 'Reduzir', 'Tamanho real'], ['Ajustar à janela']])])


def insert_menu(app, objects=False):
    """What iWork keeps in the Insert menu besides the object buttons; with
    `objects`, the objects too (version A, and version B in a narrow window)."""
    obj = ['Tabela', 'Gráfico', 'Caixa de texto', 'Forma', 'Imagem, filme ou som…', 'Comentário']
    if app == 'writer':
        rest = [[('Quebra', [['De página', 'De coluna', 'De seção', 'De layout']]),
                 ('Campo', [['Número da página', 'Total de páginas', 'Data e hora', 'Nome do arquivo',
                             'Campo de mesclagem']])],
                ['Nota de rodapé', 'Sumário', 'Marcador', 'Link…'],
                ['Caracteres especiais…', 'Escolher…']]
    elif app == 'tables':
        rest = [['Planilha', ('Linhas e colunas', [['Linha acima', 'Linha abaixo'], ['Coluna antes', 'Coluna depois']])],
                ['Função', 'Data e hora', 'Nome do arquivo', 'Número da página'],
                ['Link…', 'Caracteres especiais…', 'Escolher…']]
    else:
        rest = [['Slide', 'Número do slide', 'Data e hora'],
                ['Composição inteligente', 'Link…', 'Visualização web'],
                ['Caracteres especiais…', 'Escolher…']]
    return menu_model(*(([obj] if objects else []) + rest))


def zoom_menu():
    return menu_model(['Ajustar à largura', 'Ajustar à página'],
                      ['50 %', '75 %', '100 %', '125 %', '150 %', '200 %', '400 %'],
                      ['Uma página', 'Duas páginas'])


def gallery_popover(kind):
    """The popover of an insert button: a small gallery, as in iWork."""
    pop = Gtk.Popover()
    box = vbox(spacing=10, margin_top=12, margin_bottom=12, margin_start=12, margin_end=12)
    if kind == 'shape':
        box.append(classes(Gtk.SearchEntry(placeholder_text='Buscar formas…'), 'gallery-search'))
        cats = toggle_group([('b', 'Básicas', None, None), ('g', 'Geometria', None, None),
                             ('s', 'Setas', None, None), ('o', 'Objetos', None, None)], 0, None, 'flat')
        box.append(cats)
        flow = Gtk.FlowBox(max_children_per_line=6, min_children_per_line=6,
                           selection_mode=Gtk.SelectionMode.NONE, row_spacing=4, column_spacing=4)
        for icon in ['desk-fill', 'desk-stroke', 'media-record', 'starred', 'go-next', 'go-previous',
                     'desk-comment', 'desk-shape', 'media-playback-start', 'pan-up', 'desk-animate',
                     'list-add', 'desk-bookmark', 'user-bookmarks', 'view-pin', 'emote-love',
                     'weather-clear', 'mail-attachment']:
            b = Gtk.Button(icon_name=icon + '-symbolic')
            classes(b, 'flat')
            b.get_first_child().set_pixel_size(24) if b.get_first_child() else None
            flow.append(b)
        box.append(flow)
        box.append(button_row_plain('Desenhar com a caneta'))
    elif kind in ('table', 'chart'):
        flow = Gtk.FlowBox(max_children_per_line=3, min_children_per_line=3,
                           selection_mode=Gtk.SelectionMode.NONE, row_spacing=8, column_spacing=8)
        if kind == 'chart':
            sw = toggle_group([('2d', '2D', None, None), ('3d', '3D', None, None),
                               ('i', 'Interativo', None, None)], 0, None)
            box.append(sw)
        cols = ['#3d5373', '#1c71d8', '#26a269', '#e66100', '#813d9c', '#5e5c64']
        for i, col in enumerate(cols):
            flow.append(color_box(f'linear-gradient(180deg, {col} 0 26%, white 26% 50%, '
                                  f'color-mix(in srgb, {col} 12%, white) 50% 74%, white 74%)'
                                  if kind == 'table' else
                                  f'linear-gradient(90deg, white 0 12%, {col} 12% 30%, white 30% 41%, '
                                  f'color-mix(in srgb, {col} 55%, white) 41% 59%, white 59% 70%, {col} 70% 88%, white 88%)',
                                  'style-chip', w=74, h=52))
        box.append(flow)
    elif kind == 'media':
        sw = toggle_group([('f', 'Fotos', None, None), ('m', 'Música', None, None),
                           ('v', 'Filmes', None, None)], 0, None)
        box.append(sw)
        flow = Gtk.FlowBox(max_children_per_line=4, min_children_per_line=4,
                           selection_mode=Gtk.SelectionMode.NONE, row_spacing=6, column_spacing=6)
        for col in ['#b5d6f2', '#f6d32d', '#8ff0a4', '#f66151', '#dc8add', '#cdab8f', '#99c1f1', '#57e389']:
            flow.append(color_box(f'linear-gradient(160deg, {col}, color-mix(in srgb, {col} 60%, black))',
                                  'style-chip', w=58, h=44))
        box.append(flow)
        box.append(button_row_plain('Escolher arquivo…'))
    pop.set_child(box)
    return pop


def button_row_plain(text):
    b = Gtk.Button(label=text)
    classes(b, 'flat')
    b.get_child().set_xalign(0)
    return b


INSERT = [  # cid suffix, icon, tooltip, popover kind
    ('tabela', 'desk-table', 'Tabela', 'table'),
    ('grafico', 'desk-chart', 'Gráfico', 'chart'),
    ('texto', 'desk-textbox', 'Caixa de texto', None),
    ('forma', 'desk-shape', 'Forma', 'shape'),
    ('midia', 'desk-media', 'Mídia', 'media'),
    ('comentario', 'desk-comment', 'Comentário', None),
]


def insert_menu_button(app, cid='cab.inserir'):
    """Insert, with an icon and a label: what the Insert menu of iWork keeps
    besides the objects, and the objects too when the window is narrow."""
    return menu_button(icon='desk-insert', text='Inserir', tip='Menu Inserir', cid=cid, arrow=False,
                       menu=insert_menu(app))


def insert_buttons(prefix='cab.inserir'):
    """Table, Chart, Text, Shape, Media and Comment, one button each."""
    out = []
    for suf, icon, tip, kind in INSERT:
        if kind:
            out.append(menu_button(icon=icon, tip=tip, cid=f'{prefix}.{suf}', popover=gallery_popover(kind)))
        else:
            out.append(icon_button(icon, tip, f'{prefix}.{suf}'))
    return out


# -------------------------------------------------------------- format bars


PALETTE = [  # the GNOME palette, five shades of nine colours
    ['#99c1f1', '#8ff0a4', '#f9f06b', '#ffbe6f', '#f66151', '#dc8add', '#cdab8f', '#ffffff', '#77767b'],
    ['#62a0ea', '#57e389', '#f8e45c', '#ffa348', '#ed333b', '#c061cb', '#b5835a', '#f6f5f4', '#5e5c64'],
    ['#3584e4', '#33d17a', '#f6d32d', '#ff7800', '#e01b24', '#9141ac', '#986a44', '#deddda', '#3d3846'],
    ['#1c71d8', '#2ec27e', '#f5c211', '#e66100', '#c01c28', '#813d9c', '#865e3c', '#c0bfbc', '#241f31'],
    ['#1a5fb4', '#26a269', '#e5a50a', '#c64600', '#a51d2d', '#613583', '#63452c', '#9a9996', '#000000'],
]


def colors_popover(cid='cores.popover', title='Cor do texto'):
    """Colours as a popover: the palette, the colours of the document, the
    eyedropper and a custom colour. It takes the place of the Colors window."""
    pop = Gtk.Popover()
    grid = Gtk.Grid(row_spacing=3, column_spacing=3)
    for r, row in enumerate(PALETTE):
        for c, col in enumerate(row):
            grid.attach(color_box(col, 'palette-cell'), c, r, 1, 1)
    recent = hbox(spacing=3)
    for col in ['#3d5373', '#e66100', '#1e1e1e', '#26a269']:
        recent.append(color_box(col, 'palette-cell'))
    actions = hbox(tag(icon_button('color-select', 'Conta-gotas'), 'cores.conta-gotas'),
                   tag(text_button('Personalizada…', None, None, 'flat'), 'cores.personalizada'), expand(),
                   tag(text_button('Nenhuma', None, None, 'flat'), 'cores.nenhuma'), spacing=4)
    box = vbox(label(title, 'heading'), tag(grid, 'cores.paleta'), label('Do documento', 'dimmed', 'tiny'),
               tag(recent, 'cores.documento'), actions, spacing=8, margin_top=12, margin_bottom=10,
               margin_start=12, margin_end=12)
    pop.set_child(box)
    return tag(pop, cid)


def fonts_popover(cid='fontes.popover'):
    """Fonts as a popover: search, the fonts of the document, then all of them,
    each written in itself. It takes the place of the Fonts window."""
    pop = Gtk.Popover()
    entry = Gtk.SearchEntry(placeholder_text='Buscar fontes…')
    lb = Gtk.ListBox(selection_mode=Gtk.SelectionMode.SINGLE)
    classes(lb, 'navigation-sidebar')
    fams = ['TeX Gyre Termes', 'Adwaita Sans', 'Adwaita Mono', 'Carlito', 'Caladea', 'DejaVu Sans',
            'Liberation Serif', 'TeX Gyre Heros', 'TeX Gyre Pagella']
    for i, fam in enumerate(fams):
        lab = Gtk.Label(xalign=0)
        lab.set_markup(f'<span font_family="{fam}">{fam}</span>')
        classes(lab, 'font-sample')
        row = Gtk.ListBoxRow(child=hbox(lab, expand(), Gtk.Image(icon_name='object-select-symbolic') if i == 0
                                        else None))
        lb.append(row)
        if i == 1:
            lb.set_header_func(None)
    lb.select_row(lb.get_row_at_index(0))
    sw = Gtk.ScrolledWindow(child=lb, hscrollbar_policy=Gtk.PolicyType.NEVER, min_content_height=300,
                            min_content_width=250)
    foot = button_row_plain('Tipo, espaçamento e ligaduras no Inspetor')
    box = vbox(tag(entry, 'fontes.busca'), label('No documento e recentes', 'dimmed', 'tiny', margin_start=8),
               tag(sw, 'fontes.lista'), Gtk.Separator(), tag(foot, 'fontes.mais'), spacing=6, margin_top=10,
               margin_bottom=6, margin_start=6, margin_end=6)
    pop.set_child(box)
    return tag(pop, cid)



def align_group(cid=None, *css):
    return toggle_group([('l', None, 'format-justify-left', 'Alinhar à esquerda'),
                         ('c', None, 'format-justify-center', 'Centralizar'),
                         ('r', None, 'format-justify-right', 'Alinhar à direita'),
                         ('j', None, 'format-justify-fill', 'Justificar')], 3, cid, *css)


def biu(prefix, strike=False):
    out = [icon_button('format-text-bold', 'Negrito', f'{prefix}.negrito', toggle=True),
           icon_button('format-text-italic', 'Itálico', f'{prefix}.italico', toggle=True),
           icon_button('format-text-underline', 'Sublinhado', f'{prefix}.sublinhado', toggle=True)]
    if strike:
        out.append(icon_button('format-text-strikethrough', 'Tachado', f'{prefix}.tachado', toggle=True))
    return out


def size_spin(value=11, cid=None):
    s = Gtk.SpinButton.new_with_range(1, 400, 1)
    s.set_value(value)
    s.set_width_chars(3)
    s.set_tooltip_text('Tamanho da fonte')
    s.set_valign(Gtk.Align.CENTER)
    return tag(s, cid) if cid else s


def format_bar_text(prefix, slim=False, full=False, compact=False):
    """The format bar when text is selected.

    full: the text mode of the iWork '09 bar, in its order: styles drawer,
    paragraph and character style; font, typeface, size, the two colours;
    B I U; alignment; line spacing and columns; list style. slim: what is used
    all the time, the rest under More.
    """
    bar = hbox(spacing=4)
    classes(bar, 'toolbar', 'formatbar-slim' if slim else 'formatbar')
    if full:
        bar.append(icon_button('desk-styles', 'Gaveta de estilos', f'{prefix}.gaveta', toggle=True))
    bar.append(menu_button(icon='desk-pilcrow' if slim else None, text='Corpo', tip='Estilo de parágrafo',
                           cid=f'{prefix}.estilo-paragrafo', max_chars=9 if full else None,
                           menu=menu_model(['Título', 'Subtítulo', 'Cabeçalho 1', 'Cabeçalho 2', 'Corpo',
                                            'Legenda', 'Citação'], ['Novo estilo a partir da seleção…',
                                                                    'Redefinir estilo a partir da seleção'])))
    if full:
        bar.append(menu_button(text='Nenhum', tip='Estilo de caractere', cid=f'{prefix}.estilo-caractere',
                               max_chars=8,
                               menu=menu_model(['Nenhum', 'Ênfase', 'Forte', 'Sublinhado'])))
    bar.append(spacer())
    bar.append(menu_button(text='TeX Gyre Termes', tip='Fonte', cid=f'{prefix}.fonte', popover=fonts_popover(),
                           max_chars=13 if full else 9 if compact else 16, visible=compact != 'phone'))
    if full:
        bar.append(dropdown(['Regular', 'Itálico', 'Negrito', 'Negrito itálico'], 0, f'{prefix}.tipo',
                            tooltip_text='Tipo'))
    if compact != 'phone':
        bar.append(size_spin(11, f'{prefix}.tamanho'))
    color = menu_button(icon='desk-text-color', tip='Cor do texto', cid=f'{prefix}.cor-texto',
                        popover=colors_popover())
    if full:
        bar.append(color)
        bar.append(menu_button(icon='desk-highlight', tip='Cor de fundo do texto', cid=f'{prefix}.cor-fundo',
                               popover=Gtk.Popover()))
    bar.append(spacer())
    for b in biu(prefix, strike=full):
        bar.append(b)
    if compact:   # a narrow window: the rest goes under More
        bar.append(expand())
        bar.append(menu_button(icon='view-more-horizontal', tip='Mais opções de formatação',
                               cid=f'{prefix}.mais', menu=menu_model(['Cor do texto', 'Alinhamento',
                                                                      'Espaçamento entre linhas', 'Estilo de lista'],
                                                                     ['Mostrar no Inspetor'])))
        return bar
    if not full:
        bar.append(color)
    bar.append(spacer())
    bar.append(align_group(f'{prefix}.alinhamento', 'flat'))
    bar.append(spacer())
    bar.append(menu_button(icon='desk-line-spacing', tip='Espaçamento entre linhas', cid=f'{prefix}.entrelinha',
                           menu=menu_model(['0,5', '1,0', '1,5', '2,0'], ['Mostrar mais…'])))
    if full:
        bar.append(menu_button(icon='desk-columns', tip='Colunas', cid=f'{prefix}.colunas',
                               menu=menu_model(['1', '2', '3', '4'])))
    bar.append(menu_button(icon='view-list-bullet', tip='Estilo de lista', cid=f'{prefix}.lista',
                           menu=menu_model(['Nenhum', 'Marcador', 'Imagem', 'Numerada', 'Harvard', 'Legal'])))
    bar.append(expand())
    if full:
        bar.append(menu_button(text='Formato', tip='Menu Formato', cid=f'{prefix}.menu-formatar',
                               menu=menu_model([('Fonte', [['Negrito', 'Itálico', 'Sublinhado', 'Tachado', 'Contorno'],
                                                           ['Maior', 'Menor'], ['Espaçamento', 'Ligaduras',
                                                                                'Linha de base', 'Maiúsculas']]),
                                                ('Texto', [['Alinhar à esquerda', 'Centralizar',
                                                            'Alinhar à direita', 'Justificar']])],
                                               ['Copiar estilo de parágrafo', 'Copiar estilo de caractere',
                                                'Colar estilo'],
                                               [('Avançado', [['Definir como objeto mestre',
                                                               'Tornar objetos mestres selecionáveis']])])))
    else:
        bar.append(menu_button(icon='view-more-horizontal', tip='Mais opções de formatação',
                               cid=f'{prefix}.mais',
                               menu=menu_model(['Estilo de caractere', 'Tachado', 'Cor de fundo do texto',
                                                'Colunas'],
                                               ['Copiar estilo', 'Colar estilo'],
                                               ['Mostrar no Inspetor'])))
    return bar


ARRANGE_MENU = [['Trazer para a frente', 'Trazer uma camada para a frente', 'Enviar uma camada para trás',
                 'Enviar para trás'],
                [('Alinhar objetos', [['Esquerda', 'Centro', 'Direita'], ['Em cima', 'No meio', 'Embaixo']]),
                 ('Distribuir objetos', [['Horizontalmente', 'Verticalmente']])],
                ['Espelhar horizontalmente', 'Espelhar verticalmente'],
                ['Agrupar', 'Desagrupar'], ['Travar', 'Destravar']]


def stroke_popover():
    pop = Gtk.Popover()
    lb = Gtk.ListBox(selection_mode=Gtk.SelectionMode.NONE)
    classes(lb, 'boxed-list')
    lb.append(combo_row('Tipo', ['Nenhum', 'Linha', 'Tracejado', 'Pontilhado', 'Moldura de imagem'], 1))
    lb.append(spin_row('Espessura', 1, 0, 100, 0.25, digits=2, subtitle='pt'))
    lb.append(action_row('Cor', suffix=swatch('#1e1e1e')))
    lb.set_size_request(260, -1)
    box = vbox(lb, margin_top=10, margin_bottom=10, margin_start=10, margin_end=10)
    pop.set_child(box)
    return pop


def opacity_popover(value=100):
    pop = Gtk.Popover()
    s = Gtk.Scale.new_with_range(Gtk.Orientation.HORIZONTAL, 0, 100, 1)
    s.set_value(value)
    s.set_size_request(180, -1)
    pop.set_child(hbox(s, label(f'{value} %', 'numeric'), spacing=8, margin_top=8, margin_bottom=8,
                       margin_start=12, margin_end=12))
    return pop


def format_bar_object(prefix, kind='image', slim=False, writer=False):
    """The format bar when an object is selected.

    Full (version A): the image or shape mode of the iWork '09 bar, in its
    order: stroke style, thickness and colour; fill (shapes); adjust image and
    mask (images); opacity, shadow, reflection; in Writer, inline or floating
    and the wrap. Slim (version B): one popover for the stroke, toggles for
    shadow and reflection, and the Arrange commands the iWork toolbar had
    (front, back, group), which the Desk header bar does not hold.
    """
    bar = hbox(spacing=4)
    classes(bar, 'toolbar', 'formatbar-slim' if slim else 'formatbar')
    if slim:
        bar.append(menu_button(icon='desk-styles', text='Estilo', tip='Estilos de objeto',
                               cid=f'{prefix}.estilo-objeto', popover=Gtk.Popover()))
        bar.append(spacer())
        if kind != 'image':
            bar.append(menu_button(icon='desk-fill', tip='Preenchimento', cid=f'{prefix}.preenchimento',
                                   popover=Gtk.Popover()))
        bar.append(menu_button(icon='desk-stroke', tip='Traço', cid=f'{prefix}.traco', popover=stroke_popover()))
        bar.append(icon_button('desk-shadow', 'Sombra', f'{prefix}.sombra', toggle=True, active=True))
        bar.append(icon_button('desk-reflection', 'Reflexo', f'{prefix}.reflexo', toggle=True))
        bar.append(menu_button(icon='desk-opacity', tip='Opacidade', cid=f'{prefix}.opacidade',
                               popover=opacity_popover()))
    else:
        bar.append(label('Traço', 'dimmed', 'tiny', margin_start=4))
        bar.append(dropdown(['Nenhum', '————', '- - - -', '· · · ·', 'Moldura'], 1, f'{prefix}.traco-tipo',
                            tooltip_text='Estilo do traço'))
        w = Gtk.SpinButton.new_with_range(0, 100, 0.25)
        w.set_digits(0), w.set_value(1), w.set_width_chars(2), w.set_valign(Gtk.Align.CENTER)
        w.set_tooltip_text('Espessura do traço')
        bar.append(tag(w, f'{prefix}.traco-espessura'))
        bar.append(swatch('#1e1e1e', f'{prefix}.traco-cor', tip='Cor do traço'))
        if kind != 'image':
            bar.append(spacer())
            bar.append(label('Preenchimento', 'dimmed', 'tiny'))
            bar.append(swatch('#1c71d8', f'{prefix}.preenchimento', tip='Preenchimento'))
    bar.append(spacer())
    if kind == 'image':
        bar.append(icon_button('desk-image-adjust', 'Ajustar imagem', f'{prefix}.ajustar-imagem'))
        bar.append(icon_button('desk-crop', 'Editar máscara', f'{prefix}.mascara'))
        if slim:
            bar.append(icon_button('desk-wand', 'Alfa instantâneo', f'{prefix}.alfa'))
        bar.append(spacer())
    if not slim:
        bar.append(label('Opacidade', 'dimmed', 'tiny'))
        bar.append(dropdown(['100 %', '75 %', '50 %', '25 %', '0 %'], 0, f'{prefix}.opacidade',
                            tooltip_text='Opacidade'))
        bar.append(tag(Gtk.CheckButton(label='Sombra', active=True), f'{prefix}.sombra'))
        bar.append(tag(Gtk.CheckButton(label='Reflexo'), f'{prefix}.reflexo'))
        bar.append(spacer())
    if writer:
        bar.append(toggle_group([('i', 'Em linha', None, None), ('f', 'Flutuante', None, None)], 1,
                                f'{prefix}.posicionamento', 'flat' if slim else 'round'))
        bar.append(menu_button(icon='desk-wrap', tip='Ajuste do texto', cid=f'{prefix}.ajuste',
                               menu=menu_model(['Nenhum', 'Automático', 'Em volta', 'Acima e abaixo',
                                                'À esquerda', 'À direita'])))
        bar.append(spacer())
    if slim:
        bar.append(icon_button('desk-front', 'Trazer para a frente', f'{prefix}.frente'))
        bar.append(icon_button('desk-back', 'Enviar para trás', f'{prefix}.fundo'))
        bar.append(menu_button(icon='desk-arrange', tip='Organizar', cid=f'{prefix}.organizar',
                               menu=menu_model(*ARRANGE_MENU)))
    bar.append(expand())
    if slim:
        bar.append(menu_button(icon='view-more-horizontal', tip='Mais opções de formatação', cid=f'{prefix}.mais',
                               menu=menu_model(['Substituir imagem…', 'Redefinir máscara', 'Tamanho original'],
                                               ['Copiar estilo', 'Colar estilo'], ['Mostrar no Inspetor'])))
    else:
        bar.append(menu_button(text='Formato', tip='Menu Formato', cid=f'{prefix}.menu-formatar',
                               menu=menu_model([('Imagem', [['Máscara', 'Mascarar com forma', 'Alfa instantâneo'],
                                                            ['Tamanho original']]),
                                                ('Forma', [['Tornar editável', 'Redefinir geometria do texto']])],
                                               ['Copiar estilo gráfico', 'Colar estilo gráfico'])))
    return bar


# ---------------------------------------------------------- inspector pages


def style_header(name='Corpo', cid='insp.estilo-atual', changed=True):
    """The current paragraph style, at the top of the text pages."""
    row = action_row(name + ('*' if changed else ''), subtitle='Estilo de parágrafo')
    if changed:
        row.add_suffix(classes(Gtk.Button(label='Atualizar', valign=Gtk.Align.CENTER), 'flat'))
    row.add_suffix(Gtk.Image(icon_name='go-next-symbolic'))
    row.set_activatable(True)
    lb = Gtk.ListBox(selection_mode=Gtk.SelectionMode.NONE)
    classes(lb, 'boxed-list')
    lb.append(row)
    return tag(lb, cid)


def text_style_page(prefix='insp.texto'):
    font = group(
        'Fonte',
        action_row('Família', subtitle='TeX Gyre Termes', arrow=True),
        combo_row('Tipo', ['Regular', 'Itálico', 'Negrito', 'Negrito itálico']),
        spin_row('Tamanho', 11, 1, 400, 1, subtitle='pt'),
        action_row('Estilo', suffix=hbox(*biu(prefix + '.f', strike=True), spacing=2)),
        action_row('Cor do texto', suffix=swatch('#1e1e1e')),
        expander_row('Opções avançadas',
                     spin_row('Espaçamento entre caracteres', 0, -100, 100, 1, subtitle='%'),
                     combo_row('Linha de base', ['Padrão', 'Sobrescrito', 'Subscrito']),
                     combo_row('Maiúsculas', ['Nenhuma', 'Todas maiúsculas', 'Versalete', 'Título']),
                     switch_row('Ligaduras', True),
                     action_row('Cor de fundo do texto', suffix=swatch('transparent'))),
        cid=prefix + '.fonte')
    align = group('Alinhamento',
                  action_row('Horizontal', suffix=align_group()),
                  action_row('Recuo', suffix=hbox(icon_button('format-indent-less', 'Diminuir recuo'),
                                                  icon_button('format-indent-more', 'Aumentar recuo'),
                                                  spacing=2)),
                  cid=prefix + '.alinhamento')
    spacing = group('Espaçamento',
                    spin_row('Entre linhas', 1.2, 0.5, 4, 0.1, digits=1),
                    spin_row('Antes do parágrafo', 0, 0, 200, 1, subtitle='pt'),
                    spin_row('Depois do parágrafo', 6, 0, 200, 1, subtitle='pt'),
                    cid=prefix + '.espacamento')
    lists = group('Marcadores e listas',
                  combo_row('Estilo de lista', ['Nenhum', 'Marcador', 'Imagem', 'Numerada', 'Harvard']),
                  cid=prefix + '.listas')
    return page(font, align, spacing, lists)


def text_layout_page(prefix='insp.layout'):
    return page(
        group('Colunas', spin_row('Colunas', 1, 1, 10, 1), switch_row('Largura igual', True),
              cid=prefix + '.colunas'),
        group('Recuos', spin_row('Primeira linha', 0.0, 0, 20, 0.1, digits=2, subtitle='cm'),
              spin_row('Esquerda', 0.0, 0, 20, 0.1, digits=2, subtitle='cm'),
              spin_row('Direita', 0.0, 0, 20, 0.1, digits=2, subtitle='cm'), cid=prefix + '.recuos'),
        group('Tabulações', spin_row('Padrão', 1.25, 0, 20, 0.05, digits=2, subtitle='cm'),
              button_row('Adicionar tabulação', 'list-add'), cid=prefix + '.tabulacoes'),
        group('Bordas e fundo do parágrafo',
              combo_row('Borda', ['Nenhuma', 'Linha', 'Pontilhada', 'Tracejada']),
              action_row('Cor de fundo', suffix=swatch('transparent')), cid=prefix + '.bordas'))


def text_more_page(prefix='insp.mais'):
    return page(
        group('Paginação e quebras', switch_row('Manter linhas juntas'),
              switch_row('Manter com o parágrafo seguinte'),
              switch_row('Parágrafo começa em nova página'),
              switch_row('Evitar linhas viúvas e órfãs', True), cid=prefix + '.paginacao'),
        group('Hifenização e idioma', switch_row('Remover hifenização do parágrafo'),
              combo_row('Idioma', ['Português (Brasil)', 'Inglês (EUA)', 'Espanhol']),
              cid=prefix + '.idioma'),
        group('Estilo seguinte', combo_row('Parágrafo seguinte', ['Mesmo', 'Corpo', 'Cabeçalho 1']),
              cid=prefix + '.seguinte'))


def style_chips(cid='insp.estilos-objeto'):
    """Six ready-made object styles, as in the Format pane of iWork."""
    box = hbox(spacing=6, halign=Gtk.Align.CENTER)
    for col in ['#1c71d8', '#26a269', '#e66100', '#c01c28', '#813d9c', '#5e5c64']:
        b = Gtk.Button()
        b.set_child(color_box(col, 'style-chip'))
        classes(b, 'flat', 'chip-button')
        box.append(b)
    return tag(box, cid)


def object_style_page(prefix='insp.estilo', image=False):
    fill = None if image else group('Preenchimento',
                 combo_row('Tipo', ['Nenhum', 'Cor', 'Gradiente', 'Gradiente avançado', 'Imagem',
                                    'Imagem tingida'], 1),
                 action_row('Cor', suffix=swatch('#1c71d8')), cid=prefix + '.preenchimento')
    stroke = group('Traço',
                   combo_row('Tipo', ['Nenhum', 'Linha', 'Moldura de imagem'], 1),
                   action_row('Estilo e cor', suffix=[dropdown(['———', '- - -', '· · ·'], 0), swatch('#1e1e1e')]),
                   spin_row('Espessura', 1, 0, 100, 0.25, digits=2, subtitle='pt'),
                   cid=prefix + '.traco')
    shadow = group('Sombra', expander_row_switch('Sombra',
                                                 spin_row('Deslocamento', 4, 0, 100, 1, subtitle='pt'),
                                                 spin_row('Desfoque', 6, 0, 100, 1, subtitle='pt'),
                                                 spin_row('Ângulo', 315, 0, 360, 1, subtitle='°'),
                                                 action_row('Cor', suffix=swatch('#000000'))),
                   switch_row('Reflexo'), cid=prefix + '.sombra')
    opacity = group('Opacidade', scale_row('Opacidade', 100), cid=prefix + '.opacidade')
    return page(style_chips(), *[g for g in (fill, stroke, shadow, opacity) if g is not None])


def expander_row_switch(title, *rows, enabled=True):
    r = Adw.ExpanderRow(title=title, show_enable_switch=True, enable_expansion=enabled)
    for row in rows:
        r.add_row(row)
    return r


def arrange_page(prefix='insp.organizar', writer=True):
    groups = []
    if writer:
        groups.append(group(
            'Posicionamento',
            action_row('Objeto', suffix=toggle_group([('i', 'Em linha', None, None),
                                                      ('f', 'Flutuante', None, None)], 1)),
            cid=prefix + '.posicionamento'))
        groups.append(group(
            'Ajuste do texto',
            combo_row('Ajuste', ['Nenhum', 'Automático', 'Em volta', 'Acima e abaixo', 'Esquerda', 'Direita'], 1),
            action_row('Contorno', suffix=toggle_group([('r', 'Retângulo', None, None),
                                                        ('c', 'Contorno', None, None)], 0)),
            spin_row('Espaço extra', 12, 0, 200, 1, subtitle='pt'),
            cid=prefix + '.ajuste'))
    groups.append(group(
        'Tamanho', spin_row('Largura', 7.2, 0, 100, 0.1, digits=2, subtitle='cm'),
        spin_row('Altura', 5.1, 0, 100, 0.1, digits=2, subtitle='cm'),
        switch_row('Manter proporções', True), cid=prefix + '.tamanho'))
    groups.append(group(
        'Posição e giro', spin_row('X', 9.4, 0, 100, 0.1, digits=2, subtitle='cm'),
        spin_row('Y', 12.0, 0, 100, 0.1, digits=2, subtitle='cm'),
        spin_row('Ângulo', 0, 0, 359, 1, subtitle='°'),
        action_row('Espelhar', suffix=[icon_button('object-flip-horizontal', 'Espelhar horizontalmente'),
                                       icon_button('object-flip-vertical', 'Espelhar verticalmente')]),
        cid=prefix + '.metricas'))
    groups.append(group(
        'Ordem e alinhamento',
        action_row('Ordem', suffix=hbox(icon_button('desk-front', 'Trazer para a frente'),
                                        icon_button('go-up', 'Trazer uma camada para a frente'),
                                        icon_button('go-down', 'Enviar uma camada para trás'),
                                        icon_button('desk-back', 'Enviar para trás'), spacing=2)),
        combo_row('Alinhar', ['Esquerda', 'Centro', 'Direita', 'Em cima', 'Meio', 'Embaixo'], 0),
        combo_row('Distribuir', ['Horizontalmente', 'Verticalmente'], 0),
        cid=prefix + '.ordem'))
    groups.append(group(
        'Grupo e trava',
        action_row('Grupo', suffix=hbox(icon_button('desk-group', 'Agrupar'),
                                        icon_button('desk-ungroup', 'Desagrupar'), spacing=2)),
        switch_row('Travar o objeto'), cid=prefix + '.grupo'))
    return page(*groups)


def _mini_spin(value, digits=1):
    s = Gtk.SpinButton.new_with_range(0, 999, 0.1)
    s.set_digits(digits)
    s.set_value(value)
    s.set_width_chars(4)
    s.set_valign(Gtk.Align.CENTER)
    return s


def image_page(prefix='insp.imagem'):
    return page(
        group('Arquivo', action_row('paisagem.jpg', subtitle='1.920 × 1.080 px · 412 kB', prop=False),
              button_row('Substituir…'), cid=prefix + '.arquivo'),
        group('Máscara e transparência', button_row('Editar máscara', 'desk-crop'),
              button_row('Alfa instantâneo', 'desk-wand'), cid=prefix + '.mascara'),
        group('Ajustar imagem', scale_row('Exposição', 50), scale_row('Saturação', 50),
              scale_row('Contraste', 50),
              expander_row('Mais ajustes', scale_row('Temperatura', 50), scale_row('Nitidez', 0)),
              action_row('', suffix=hbox(text_button('Melhorar', None, None, 'flat'),
                                         text_button('Redefinir', None, None, 'flat'))),
              cid=prefix + '.ajustar'),
        group('Acessibilidade', Adw.EntryRow(title='Descrição da imagem'), cid=prefix + '.descricao'))


def document_page(prefix='insp.doc'):
    return page(
        group('Papel', combo_row('Tamanho', ['A4', 'Carta', 'Ofício', 'A5', 'Personalizado…']),
              action_row('Orientação', suffix=toggle_group([('r', 'Retrato', None, None),
                                                            ('p', 'Paisagem', None, None)], 0)),
              cid=prefix + '.papel'),
        group('Margens', spin_row('Superior', 2.5, 0, 20, 0.1, digits=2, subtitle='cm'),
              spin_row('Inferior', 2.5, 0, 20, 0.1, digits=2, subtitle='cm'),
              spin_row('Esquerda', 3.0, 0, 20, 0.1, digits=2, subtitle='cm'),
              spin_row('Direita', 2.0, 0, 20, 0.1, digits=2, subtitle='cm'),
              switch_row('Páginas opostas'), cid=prefix + '.margens'),
        group('Cabeçalho e rodapé', switch_row('Cabeçalho', True), switch_row('Rodapé', True),
              cid=prefix + '.cabecalho'),
        group('Notas', combo_row('Tipo', ['Notas de rodapé', 'Notas de fim de documento',
                                          'Notas de fim de seção']),
              combo_row('Formato', ['1, 2, 3', 'i, ii, iii', 'a, b, c', '*, †, ‡']),
              cid=prefix + '.notas'),
        group('Texto do documento', switch_row('Hifenizar', True), switch_row('Usar ligaduras', True),
              cid=prefix + '.texto'),
        group('Segurança', switch_row('Exigir senha para abrir'), cid=prefix + '.senha'))


def section_page(prefix='insp.secao'):
    return page(
        group('Numeração de páginas', action_row('Numeração', suffix=toggle_group(
            [('c', 'Continuar', None, None), ('i', 'Iniciar em', None, None)], 0)),
            combo_row('Formato', ['1, 2, 3', 'i, ii, iii', 'I, II, III', 'a, b, c']),
            cid=prefix + '.numeracao'),
        group('Cabeçalhos e rodapés', switch_row('Primeira página diferente'),
              switch_row('Páginas esquerda e direita diferentes'),
              switch_row('Usar os da seção anterior', True), cid=prefix + '.config'),
        group('Início da seção', combo_row('A seção começa em', ['Qualquer página', 'Página esquerda',
                                                                  'Página direita']),
              cid=prefix + '.inicio'))


def info_page(prefix='insp.info'):
    return page(
        group('Informações', Adw.EntryRow(title='Autor', text='Ana Souza'),
              Adw.EntryRow(title='Título', text='Relatório anual 2026'),
              Adw.EntryRow(title='Palavras-chave'), Adw.EntryRow(title='Comentários'),
              cid=prefix + '.campos'),
        group('Estatísticas', action_row('Palavras', subtitle='26.171', prop=True),
              action_row('Páginas', subtitle='41', prop=True),
              action_row('Parágrafos', subtitle='612', prop=True),
              action_row('Caracteres', subtitle='168.304', prop=True),
              combo_row('Intervalo', ['Documento', 'Seleção']), cid=prefix + '.estatisticas'))


# --------------------------------------------------------------- bottom bar


def flat_menu(text, tip, cid, menu=None, popover=None, arrow=True):
    b = menu_button(text=text, tip=tip, cid=cid, menu=menu, popover=popover, direction=Gtk.ArrowType.UP,
                    arrow=arrow)
    if not arrow:
        b.set_child(Gtk.Label(label=text))   # a custom child: no arrow
    classes(b, 'flat', 'numeric')
    return b


def thumbs_list(kind='page', count=6, selected=2, cid='lat.miniaturas', width=132):
    """The list of page or slide thumbnails of the sidebar."""
    lb = Gtk.ListBox(selection_mode=Gtk.SelectionMode.NONE)
    classes(lb, 'navigation-sidebar')
    kinds = ['cover', 'text', 'image', 'text', 'table', 'text', 'text', 'image']
    for i in range(1, count + 1):
        if kind == 'page':
            th = canvas.page_thumb(i, selected=(i == selected), kind=kinds[(i - 1) % len(kinds)])
        else:
            th = canvas.slide_thumb(i, selected=(i == selected), w=width)
        th.set_halign(Gtk.Align.CENTER)
        num = label(str(i + (4 if kind == 'page' else 0)), 'thumb-label', 'dimmed', xalign=0.5)
        if kind == 'page':
            row = vbox(th, num, spacing=2)
        else:
            row = hbox(classes(label(str(i), 'thumb-label', 'dimmed', xalign=1.0, width_chars=2), 'numeric'),
                       th, spacing=4)
            row.get_first_child().set_valign(Gtk.Align.START)
        classes(row, 'thumb-row')
        lb.append(row)
    sw = Gtk.ScrolledWindow(hscrollbar_policy=Gtk.PolicyType.NEVER, vexpand=True)
    sw.set_child(lb)
    return tag(sw, cid)
