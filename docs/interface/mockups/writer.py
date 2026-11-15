# SPDX-FileCopyrightText: 2026 Desk contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Desk Writer in the three versions."""
import gi

gi.require_version('Gtk', '4.0')
gi.require_version('Adw', '1')
from gi.repository import Adw, Gtk  # noqa: E402

import canvas
import parts
import shell
from kit import (Scene, action_row, classes, combo_row, dropdown, expand, group, hbox, icon_button,
                 inline_switcher, label, menu_button, menu_model, page, spacer, spin_row,
                 split_button, switch_row, tag, text_button, toggle_group, vbox, view_stack)

APP = 'writer'
SIDE_MODES = ['Páginas', 'Estilos', 'Comentários', 'Resultados da busca']
ADD_PAGE = menu_model(['Em branco'], [{'title': 'Páginas do modelo'}, 'Capa', 'Sumário', 'Capítulo', 'Texto'],
                      ['Quebra de seção'])


def doc_view(selection='text', rulers=True, overlays=(), comments=False):
    cv = tag(canvas.writer_canvas(selection=selection, comments=comments), 'doc.pagina')
    inner = cv
    if overlays:
        ov = Gtk.Overlay(child=cv)
        for w in overlays:
            ov.add_overlay(w)
        inner = ov
    if rulers:
        return vbox(tag(canvas.ruler('h'), 'doc.regua'), inner)
    return inner


# --------------------------------------------------------------- Inspector


def format_pages(context):
    """The pages of Format for what is selected, as (name, title, child)."""
    if context == 'text':
        return [('estilo', 'Estilo', None, parts.text_style_page()),
                ('layout', 'Layout', None, parts.text_layout_page()),
                ('mais', 'Mais', None, parts.text_more_page())], 'estilo'
    if context == 'image':
        return [('estilo', 'Estilo', None, parts.object_style_page(image=True)),
                ('imagem', 'Imagem', None, parts.image_page()),
                ('organizar', 'Organizar', None, parts.arrange_page())], 'organizar'
    if context == 'shape':
        return [('estilo', 'Estilo', None, parts.object_style_page()),
                ('texto', 'Texto', None, parts.text_style_page('insp.texto')),
                ('organizar', 'Organizar', None, parts.arrange_page())], 'estilo'
    raise ValueError(context)


def inspector_b(context='text', level=shell.FORMAT):
    levels = [shell.FORMAT, shell.DOCUMENT]
    if level == 'Documento':
        return shell.inspector_b(levels, level, pages=[
            ('doc', 'Documento', None, parts.document_page()),
            ('secao', 'Seção', None, parts.section_page()),
            ('info', 'Info', None, parts.info_page())], visible='doc')
    pages, visible = format_pages(context)
    if context == 'text':
        return shell.inspector_b(levels, level, 'Texto', parts.style_header(), pages, visible)
    return shell.inspector_b(levels, level, {'image': 'Imagem', 'shape': 'Forma'}[context], None, pages, visible)


def inspector_c(context='text', level=shell.FORMAT):
    pages, visible = format_pages(context)
    head = parts.style_header() if context == 'text' else None
    return shell.inspector_c(head, pages, visible)


A_ICONS = [('doc', 'x-office-document', 'Documento'), ('layout', 'desk-columns', 'Layout'),
           ('ajuste', 'desk-wrap', 'Ajuste'), ('texto', 'desk-character-style', 'Texto'),
           ('grafico', 'desk-fill', 'Gráfico'), ('metricas', 'desk-ruler', 'Métricas'),
           ('link', 'desk-link', 'Link')]


def inspector_a(context='text'):
    """The Text pane of the iWork '09 Inspector, in the compact rows Desk has today."""
    cg, cr = shell.compact_group, shell.compact_row
    groups = [
        cg('Cor e alinhamento',
           shell.plain_row(hbox(parts.swatch('#1e1e1e'), parts.align_group(None, 'flat'), spacing=10)),
           cid='insp.a.cor'),
        cg('Espaçamento', cr('Caractere', 0, '%', 0), cr('Linha', 1.2, 'lin.'),
           cr('Antes do parágrafo', 0, 'pt', 0), cr('Depois do parágrafo', 6, 'pt', 0),
           cid='insp.a.espacamento'),
        cg('Margem interna', cr('Margem', 0, 'pt', 0), cid='insp.a.margem'),
        cg('Recuos', cr('Primeira linha', 0, 'cm', 2), cr('Esquerda', 0, 'cm', 2), cr('Direita', 0, 'cm', 2),
           cid='insp.a.recuos'),
        cg('Hifenização', shell.switch_line('Hifenizar palavras', True), cid='insp.a.hifenizacao'),
    ]
    return shell.inspector_a(A_ICONS, 3, 'Texto', ['Texto', 'Lista', 'Tabulações', 'Mais'], groups)


# ---------------------------------------------------------------- versions


def version_a(win, context='text'):
    start = [text_button('Abrir', 'Abrir um documento', 'cab.abrir'),
             menu_button(text='Visualização', tip='Menu Visualização', cid='cab.visualizar-menu',
                         menu=parts.view_menu(APP)),
             menu_button(text='Inserir', tip='Menu Inserir', cid='cab.inserir-menu',
                         menu=parts.insert_menu(APP, objects=True))]
    end = [menu_button(icon='open-menu', tip='Menu principal', cid='cab.menu', menu=parts.main_menu(APP)),
           icon_button('sidebar-show-right', 'Inspetor', 'cab.inspetor', toggle=True, active=True),
           icon_button('image-x-generic', 'Mídia', 'cab.midia', toggle=True)]
    hb = shell.header_bar(APP, start, end)
    fbar = tag(parts.format_bar_text('fmt', full=True), 'fmt.barra')
    thumbs = parts.thumbs_list('page', 5, 2, cid='lat.miniaturas')
    thumbs.set_size_request(118, -1)
    inner = shell.split(inspector_a(context), doc_view(context), shell.INSPECTOR_A_W, Gtk.PackType.END)
    body = shell.split(thumbs, inner, 118)
    page_entry = Gtk.Entry(text='6', width_chars=3, max_width_chars=3, valign=Gtk.Align.CENTER)
    bottom = shell.bottom_bar(
        parts.flat_menu('125 %', 'Zoom', 'inf.zoom', menu=parts.zoom_menu()),
        icon_button('go-up', 'Página anterior', 'inf.anterior'),
        icon_button('go-down', 'Página seguinte', 'inf.seguinte'),
        parts.flat_menu('Página', 'Navegar por', 'inf.navegar-por',
                        menu=menu_model(['Página', 'Seção', 'Quebra de página', 'Comentário', 'Nota de rodapé',
                                         'Link', 'Estilo de parágrafo'])),
        tag(page_entry, 'inf.ir-para'),
        expand(),
        tag(label('26.171 palavras · 41 páginas', 'dimmed', 'tiny', 'numeric', margin_end=8), 'inf.contagem'))
    win.set_content(shell.toolbar_view(body, top=[hb, fbar], bottom=[bottom]))


def version_b(win, context='text', level=shell.FORMAT, find=False, banner=False, low_res=False):
    side = shell.nav_sidebar(APP, parts.thumbs_list('page', 5, 2), SIDE_MODES, 'Adicionar página', ADD_PAGE)
    start = [split_button(icon='sidebar-show', tip='Barra lateral', drop_tip='Visualização',
                          cid='cab.visualizar', menu=parts.view_menu(APP)),
             spacer(),
             parts.insert_menu_button(APP)]
    start += parts.insert_buttons()
    end = [menu_button(icon='open-menu', tip='Menu principal', cid='cab.menu', menu=parts.main_menu(APP)),
           icon_button('sidebar-show-right', 'Inspetor', 'cab.inspetor', toggle=True, active=True),
           icon_button('edit-find', 'Buscar', 'cab.localizar', toggle=True, active=find)]
    hb = shell.header_bar(APP, start, end)
    if context in ('image', 'shape'):
        fbar = tag(parts.format_bar_object('fmt', context, slim=True, writer=True), 'fmt.barra')
    else:
        fbar = tag(parts.format_bar_text('fmt', slim=True), 'fmt.barra')
    tops = [hb, fbar]
    if find:
        tops.append(find_bar())
    if low_res:
        tops.append(tag(Adw.Banner(title='Tela menor que 1280 × 720 ‒ partes da janela podem não caber',
                                   button_label='Fechar', revealed=True), 'aviso.resolucao'))
    if banner:
        tops.append(tag(Adw.Banner(title='Este documento foi aberto com 3 avisos', button_label='Ver avisos',
                                   revealed=True), 'aviso.banner'))
    body = shell.split(inspector_b(context, level), doc_view(context), shell.INSPECTOR_W, Gtk.PackType.END)
    bottom = shell.bottom_bar(
        parts.flat_menu('125 %', 'Zoom', 'inf.zoom', menu=parts.zoom_menu()),
        expand(),
        icon_button('go-up', 'Página anterior', 'inf.anterior'),
        parts.flat_menu('Página 6 de 41', 'Ir para e navegar por', 'inf.pagina', arrow=False,
                        menu=menu_model(['Ir para a página…'],
                                        [{'title': 'Navegar por'}, 'Página', 'Seção', 'Comentário',
                                         'Nota de rodapé', 'Link', 'Estilo de parágrafo'])),
        icon_button('go-down', 'Página seguinte', 'inf.seguinte'),
        expand(),
        parts.flat_menu('26.171 palavras', 'Contagem', 'inf.contagem', arrow=False,
                        menu=menu_model(['Palavras', 'Caracteres', 'Caracteres sem espaços', 'Parágrafos',
                                         'Páginas'], ['Mostrar estatísticas'])))
    content = shell.toolbar_view(body, top=tops, bottom=[bottom])
    win.set_content(shell.split(side, content, shell.SIDEBAR_W))


def find_bar():
    entry = Gtk.SearchEntry(placeholder_text='Buscar no documento…', hexpand=True, text='custo')
    entry.set_max_width_chars(28)
    row1 = hbox(tag(entry, 'busca.campo'),
                tag(label('2 de 7', 'dimmed', 'numeric'), 'busca.contador'),
                icon_button('go-up', 'Anterior', 'busca.anterior'),
                icon_button('go-down', 'Seguinte', 'busca.seguinte'),
                menu_button(icon='emblem-system', tip='Opções da busca', cid='busca.opcoes',
                            menu=menu_model(['Diferenciar maiúsculas', 'Palavras inteiras'],
                                            [{'title': 'Avançado'}, 'Estilo de parágrafo…', 'Só na seleção',
                                             'Inserir tabulação', 'Inserir quebra de parágrafo'],
                                            ['Mostrar todos os resultados na barra lateral'])),
                spacing=6)
    rep = Gtk.Entry(placeholder_text='Substituir por…', hexpand=True)
    rep.set_max_width_chars(28)
    row2 = hbox(tag(rep, 'busca.substituir-campo'),
                text_button('Substituir', None, 'busca.substituir'),
                text_button('Substituir tudo', None, 'busca.substituir-tudo'), spacing=6)
    box = vbox(row1, row2, spacing=6, halign=Gtk.Align.CENTER, margin_top=6, margin_bottom=6)
    box.set_size_request(560, -1)
    wrap = Gtk.Box(halign=Gtk.Align.FILL)
    wrap.append(expand()), wrap.append(box), wrap.append(expand())
    classes(wrap, 'toolbar')
    return tag(wrap, 'busca.barra')


def version_c(win, context='text', level=shell.FORMAT):
    side = shell.nav_sidebar(APP, parts.thumbs_list('page', 5, 2), SIDE_MODES, 'Adicionar página', ADD_PAGE)
    start = [icon_button('sidebar-show', 'Barra lateral', 'cab.lateral', toggle=True, active=True),
             menu_button(icon='desk-insert', tip='Inserir', cid='cab.inserir-tudo',
                         popover=insert_all_popover())]
    end = [menu_button(icon='open-menu', tip='Menu principal', cid='cab.menu',
                       menu=parts.main_menu(APP, lean=True)),
           spacer(),
           icon_button('x-office-document', 'Documento', 'cab.documento', toggle=True,
                       active=(level == 'Documento')),
           icon_button('desk-format', 'Formatar', 'cab.formatar', toggle=True, active=(level == shell.FORMAT)),
           spacer(),
           icon_button('edit-find', 'Buscar', 'cab.localizar', toggle=True)]
    hb = shell.header_bar(APP, start, end)
    zoom = shell.osd_pill(icon_button('zoom-out', 'Reduzir'), label('125 %', 'numeric', margin_start=4, margin_end=4),
                          icon_button('zoom-in', 'Ampliar'), cid='osd.zoom',
                          halign=Gtk.Align.END, valign=Gtk.Align.END, margin_end=14, margin_bottom=12)
    words = shell.osd_pill(label('26.171 palavras', 'numeric', margin_start=10, margin_end=10, margin_top=5,
                                 margin_bottom=5), cid='osd.contagem',
                           halign=Gtk.Align.CENTER, valign=Gtk.Align.END, margin_bottom=12)
    body = shell.split(inspector_c(context, level), doc_view(context, rulers=False, overlays=(zoom, words)),
                       shell.INSPECTOR_W, Gtk.PackType.END)
    content = shell.toolbar_view(body, top=[hb])
    win.set_content(shell.split(side, content, shell.SIDEBAR_W))


def insert_all_popover(app=APP):
    """One Insert button: every kind of object behind a view switcher."""
    pop = Gtk.Popover()
    grid = Gtk.FlowBox(max_children_per_line=3, min_children_per_line=3, selection_mode=Gtk.SelectionMode.NONE,
                       row_spacing=8, column_spacing=8)
    for col in ['#3d5373', '#1c71d8', '#26a269', '#e66100', '#813d9c', '#5e5c64']:
        grid.append(parts.color_box(f'linear-gradient(180deg, {col} 0 26%, white 26% 50%, '
                                    f'color-mix(in srgb, {col} 12%, white) 50% 74%, white 74%)',
                                    'style-chip', w=74, h=52))
    stack = view_stack([('t', 'Tabela', 'desk-table', grid),
                        ('g', 'Gráfico', 'desk-chart', Gtk.Box()),
                        ('f', 'Forma', 'desk-shape', Gtk.Box()),
                        ('m', 'Mídia', 'desk-media', Gtk.Box())], 't')
    stack.set_vexpand(False)
    sw = inline_switcher(stack, 'ins.abas', mode=Adw.InlineViewSwitcherDisplayMode.ICONS, homogeneous=True)
    rest = Gtk.ListBox(selection_mode=Gtk.SelectionMode.NONE)
    classes(rest, 'boxed-list')
    for t, ic in (('Caixa de texto', 'desk-textbox'), ('Comentário', 'desk-comment'),
                  ('Quebra, campo ou nota…', 'desk-page-break')):
        rest.append(action_row(t, prefix=Gtk.Image(icon_name=ic + '-symbolic'), arrow=(ic == 'desk-page-break')))
    box = vbox(sw, stack, tag(rest, 'ins.lista'), spacing=10, margin_top=12, margin_bottom=12, margin_start=12,
               margin_end=12)
    pop.set_child(box)
    return tag(pop, 'ins.popover')


SCENES = [
    Scene('writer-a', 'Desk Writer · A Clássica', lambda w: version_a(w), APP, 'A'),
    Scene('writer-b', 'Desk Writer · B Equilibrada', lambda w: version_b(w), APP, 'B'),
    Scene('writer-c', 'Desk Writer · C Enxuta', lambda w: version_c(w), APP, 'C'),
]
