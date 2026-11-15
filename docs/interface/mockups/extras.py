# SPDX-FileCopyrightText: 2026 Desk contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""More views of version B and the pieces the three applications share:
other selections, open menus and popovers, dialogs, narrow windows, dark."""
import gi

gi.require_version('Gtk', '4.0')
gi.require_version('Adw', '1')
from gi.repository import Adw, GLib, Gtk  # noqa: E402

import canvas
import kit
import parts
import shell
import slides
import tables
import writer
from kit import (Scene, action_row, button_row, classes, combo_row, expand, group, hbox, icon_button, label,
                 menu_button, menu_model, spacer, spin_row, split_button, switch_row, tag, text_button,
                 toggle_group, vbox)


def popup(cid):
    """Open the menu or the popover of the button tagged `cid`."""
    return lambda win: kit.find(cid).popup()


# ------------------------------------------------------------- narrow Writer


def writer_narrow(win):
    """Version B in half of the smallest screen Desk supports (1280 × 720),
    below the 720 px breakpoint: the sidebar becomes an overlay and the
    insert buttons go into the Insert menu. The Inspector stays beside the
    page down to 520 px, and over it below that."""
    side = shell.nav_sidebar('writer', parts.thumbs_list('page', 5, 2), writer.SIDE_MODES, 'Adicionar página',
                             writer.ADD_PAGE)
    start = [icon_button('sidebar-show', 'Barra lateral', 'cab.lateral', toggle=True),
             parts.insert_menu_button('writer')]
    end = [menu_button(icon='open-menu', tip='Menu principal', cid='cab.menu', menu=parts.main_menu('writer')),
           icon_button('sidebar-show-right', 'Inspetor', 'cab.inspetor', toggle=True, active=True),
           icon_button('edit-find', 'Buscar', 'cab.localizar', toggle=True)]
    hb = Adw.HeaderBar()
    title = Adw.WindowTitle(title='Relatório anual 2026', subtitle='Documentos · Editado')
    hb.set_title_widget(tag(title, 'cab.titulo'))
    for w in start:
        hb.pack_start(w)
    for w in end:
        hb.pack_end(w)
    tag(hb, 'cab.barra')
    fbar = tag(parts.format_bar_text('fmt', slim=True, compact='medium'), 'fmt.barra')
    doc = writer.doc_view('text', rulers=False)
    body = shell.split(writer.inspector_b('text'), doc, shell.INSPECTOR_W, Gtk.PackType.END)
    bottom = shell.bottom_bar(parts.flat_menu('100 %', 'Zoom', 'inf.zoom', menu=parts.zoom_menu()), expand(),
                              parts.flat_menu('Página 6 de 41', 'Ir para e navegar por', 'inf.pagina',
                                              arrow=False, menu=menu_model(['Ir para a página…'])))
    content = shell.toolbar_view(body, top=[hb, fbar], bottom=[bottom])
    win.set_content(shell.split(side, content, shell.SIDEBAR_W, collapsed=True, show=False))


# ------------------------------------------------------------ light table


def slides_light_table(win):
    start = [split_button(icon='sidebar-show', tip='Barra lateral', drop_tip='Visualização',
                          cid='cab.visualizar', menu=parts.view_menu('slides')),
             spacer(), parts.insert_menu_button('slides')]
    end = [menu_button(icon='open-menu', tip='Menu principal', cid='cab.menu', menu=parts.main_menu('slides')),
           icon_button('sidebar-show-right', 'Inspetor', 'cab.inspetor', toggle=True, active=True),
           icon_button('edit-find', 'Buscar', 'cab.localizar', toggle=True),
           spacer(),
           split_button(icon='media-playback-start', tip='Reproduzir', drop_tip='Opções da apresentação',
                        cid='cab.reproduzir', menu=slides.PLAY_MENU)]
    hb = shell.header_bar('slides', start, end)
    table = Gtk.ScrolledWindow(child=canvas.light_table(12, 3, 196), hscrollbar_policy=Gtk.PolicyType.NEVER,
                               vexpand=True, hexpand=True)
    classes(table, 'canvas')
    tag(table, 'doc.mesa-de-luz')
    body = shell.split(slides.inspector_b('slide', shell.FORMAT), table, shell.INSPECTOR_W, Gtk.PackType.END)
    size = Gtk.Scale.new_with_range(Gtk.Orientation.HORIZONTAL, 0, 100, 1)
    size.set_value(55), size.set_size_request(140, -1), size.set_draw_value(False)
    size.set_tooltip_text('Tamanho das miniaturas')
    bottom = shell.bottom_bar(tag(size, 'inf.tamanho-miniaturas'), expand(),
                              tag(label('12 slides · 1 selecionado', 'dimmed', 'tiny', 'numeric', margin_end=8),
                                  'inf.contagem'))
    win.set_content(shell.toolbar_view(body, top=[hb], bottom=[bottom]))


# --------------------------------------------------------- template chooser


def template_chooser(win):
    """The chooser inside the window: what a new window shows before there is
    a document. Categories on the left, the templates as a grid."""
    hb = Adw.HeaderBar()
    hb.set_title_widget(Adw.WindowTitle(title='Novo documento', subtitle='Desk Writer'))
    open_ = Gtk.Button(child=Adw.ButtonContent(icon_name='document-open-symbolic', label='Abrir…'),
                       tooltip_text='Abrir um documento')
    hb.pack_start(tag(open_, 'modelos.abrir'))
    hb.pack_end(menu_button(icon='open-menu', tip='Menu principal', cid='cab.menu',
                            menu=parts.main_menu('writer')))
    hb.pack_end(tag(icon_button('edit-find', 'Buscar modelos', toggle=True), 'modelos.buscar'))
    sb = Adw.Sidebar()
    s1 = Adw.SidebarSection()
    for n, ic in (('Todos', 'view-grid'), ('Recentes', 'document-open-recent'), ('Meus modelos', 'user-bookmarks')):
        s1.append(Adw.SidebarItem(title=n, icon_name=ic + '-symbolic'))
    s2 = Adw.SidebarSection(title='Processamento de texto')
    for n in ('Em branco', 'Cartas', 'Envelopes', 'Formulários', 'Currículos', 'Relatórios', 'Tópicos'):
        s2.append(Adw.SidebarItem(title=n))
    s3 = Adw.SidebarSection(title='Layout de página')
    for n in ('Em branco', 'Boletins', 'Folhetos', 'Cartazes', 'Cartões e convites', 'Cartões de visita'):
        s3.append(Adw.SidebarItem(title=n))
    for s in (s1, s2, s3):
        sb.append(s)
    sb.set_selected(0)
    side = shell.toolbar_view(tag(sb, 'modelos.categorias'), top=[Adw.HeaderBar(show_title=False)])
    flow = Gtk.FlowBox(selection_mode=Gtk.SelectionMode.NONE, max_children_per_line=5, min_children_per_line=3,
                       column_spacing=10, row_spacing=12, margin_top=18, margin_bottom=18, margin_start=24,
                       margin_end=24, valign=Gtk.Align.START, homogeneous=True)
    names = ['Em branco', 'Em branco (paisagem)', 'Carta clássica', 'Carta moderna', 'Relatório escolar',
             'Relatório de projeto', 'Currículo', 'Proposta', 'Boletim', 'Folheto']
    kinds = ['text', 'text', 'text', 'text', 'cover', 'image', 'text', 'table', 'image', 'cover']
    for i, (n, k) in enumerate(zip(names, kinds)):
        th = canvas.page_thumb(i + 1, selected=False, kind=k if i > 1 else 'blank')
        th.set_halign(Gtk.Align.CENTER)
        card = vbox(th, label(n, xalign=0.5, wrap=True, justify=Gtk.Justification.CENTER, max_width_chars=16),
                    spacing=6)
        classes(card, 'template-card')
        if i == 2:
            classes(card, 'selected')
        flow.append(card)
    grid = Gtk.ScrolledWindow(child=flow, hscrollbar_policy=Gtk.PolicyType.NEVER, vexpand=True)
    classes(grid, 'view')
    tag(grid, 'modelos.grade')
    create = classes(Gtk.Button(label='Criar'), 'suggested-action', 'pill')
    create.set_size_request(120, -1)
    bar = hbox(tag(Gtk.CheckButton(label='Mostrar ao abrir o Desk Writer', active=True), 'modelos.mostrar-ao-abrir'),
               expand(), tag(create, 'modelos.criar'), spacing=8)
    classes(bar, 'toolbar')
    content = shell.toolbar_view(grid, top=[hb], bottom=[bar])
    win.set_content(tag(shell.split(side, content, 220), 'modelos.janela'))


# ------------------------------------------------------------------ dialogs


def close_dialog(win):
    d = Adw.AlertDialog(heading='Salvar as alterações?',
                        body='O documento “Relatório anual 2026” tem alterações que não foram salvas. '
                             'Se você não salvar, elas serão perdidas.')
    d.add_response('cancel', 'Cancelar')
    d.add_response('discard', 'Descartar')
    d.add_response('save', 'Salvar')
    d.set_response_appearance('discard', Adw.ResponseAppearance.DESTRUCTIVE)
    d.set_response_appearance('save', Adw.ResponseAppearance.SUGGESTED)
    d.set_default_response('save')
    d.set_close_response('cancel')
    tag(d, 'dialogo.fechar')
    d.present(win)


def preferences_dialog(win):
    d = Adw.PreferencesDialog()
    general = Adw.PreferencesPage(title='Geral', icon_name='emblem-system-symbolic')
    general.add(group('Novos documentos',
                      combo_row('Ao criar um documento', ['Mostrar o seletor de modelos', 'Usar o modelo escolhido']),
                      action_row('Modelo', subtitle='Em branco', arrow=True, prop=True), cid='pref.novos'))
    general.add(group('Edição', switch_row('Mostrar o tamanho e a posição ao mover objetos', True),
                      switch_row('Mostrar a contagem de palavras', True),
                      switch_row('Listas automáticas', True), cid='pref.edicao'))
    general.add(group('Salvamento', switch_row('Guardar um rascunho de recuperação', True,
                                               'A cada minuto, enquanto houver alterações'),
                      switch_row('Incluir a visualização no documento', True), cid='pref.salvamento'))
    general.add(group('Autor', Adw.EntryRow(title='Nome nos comentários e nas alterações', text='Ana Souza'),
                      cid='pref.autor'))
    rulers = Adw.PreferencesPage(title='Réguas', icon_name='desk-ruler-symbolic')
    rulers.add(group('Unidades', combo_row('Unidade das réguas', ['Centímetros', 'Milímetros', 'Polegadas',
                                                                    'Pontos', 'Paicas']),
                     switch_row('Origem no centro da régua'), switch_row('Unidades como porcentagem')))
    rulers.add(group('Guias de alinhamento', switch_row('Guias no centro do objeto', True),
                     switch_row('Guias nas bordas do objeto', True)))
    auto = Adw.PreferencesPage(title='Autocorreção', icon_name='tools-check-spelling-symbolic')
    auto.add(group('Texto', switch_row('Aspas tipográficas', True), switch_row('Capitalizar frases'),
                   switch_row('Detectar links e endereços', True)))
    for p in (general, rulers, auto):
        d.add(p)
    tag(d, 'dialogo.preferencias')
    d.present(win)


def shortcuts_dialog(win):
    d = Adw.ShortcutsDialog()
    data = [
        ('Documento', [('Novo documento', '<Control>n'), ('Abrir', '<Control>o'), ('Salvar', '<Control>s'),
                       ('Salvar como', '<Control><Shift>s'), ('Imprimir', '<Control>p'),
                       ('Fechar a janela', '<Control>w')]),
        ('Painéis', [('Barra lateral', 'F9'), ('Inspetor', '<Control><Alt>i'), ('Régua', '<Control>r'),
                     ('Barra de formatação', '<Control><Shift>r'), ('Tela cheia', 'F11'),
                     ('Buscar comandos', '<Control>slash')]),
        ('Texto', [('Negrito', '<Control>b'), ('Itálico', '<Control>i'), ('Sublinhado', '<Control>u'),
                   ('Estilos de parágrafo 1 a 8', '<Control>1'), ('Fontes', '<Control>t'),
                   ('Cores', '<Control><Shift>c'), ('Colar e manter o estilo do destino', '<Control><Shift>v')]),
        ('Objetos', [('Duplicar', '<Control>d'), ('Travar', '<Control>l'), ('Agrupar', '<Control><Alt>g'),
                     ('Trazer para a frente', '<Control><Shift>bracketright'), ('Enviar para trás', '<Control><Shift>bracketleft')]),
    ]
    for title, items in data:
        sec = Adw.ShortcutsSection(title=title)
        for name, acc in items:
            sec.add(Adw.ShortcutsItem(title=name, accelerator=acc))
        d.add(sec)
    tag(d, 'dialogo.atalhos')
    d.present(win)


def command_dialog(win):
    """Search the commands: every item the iWork menu bar had, by name."""
    d = Adw.Dialog(title='Buscar comandos', content_width=460, content_height=420)
    entry = Gtk.SearchEntry(text='quebra', placeholder_text='Buscar comandos…', hexpand=True)
    hb = Adw.HeaderBar()
    hb.set_title_widget(tag(entry, 'comandos.campo'))
    lb = Gtk.ListBox(selection_mode=Gtk.SelectionMode.SINGLE)
    classes(lb, 'navigation-sidebar')
    rows = [('Inserir quebra de página', 'Inserir', '<Control>Return'),
            ('Inserir quebra de coluna', 'Inserir', None),
            ('Inserir quebra de seção', 'Inserir', None),
            ('Inserir quebra de layout', 'Inserir', None),
            ('Mostrar invisíveis (quebras e espaços)', 'Visualização', '<Control><Shift>i'),
            ('Parágrafo começa em nova página', 'Formato › Texto', None),
            ('Remover quebras de página da seleção', 'Editar', None)]
    for i, (name, where, acc) in enumerate(rows):
        r = Adw.ActionRow(title=name, subtitle=where)
        if acc:
            r.add_suffix(Adw.ShortcutLabel(accelerator=acc, valign=Gtk.Align.CENTER))
        lb.append(r)
    lb.select_row(lb.get_row_at_index(0))
    sw = Gtk.ScrolledWindow(child=tag(lb, 'comandos.lista'), hscrollbar_policy=Gtk.PolicyType.NEVER, vexpand=True)
    d.set_child(shell.toolbar_view(sw, top=[hb]))
    tag(d, 'comandos.dialogo')
    d.present(win)


def wb(**kw):
    return lambda w: writer.version_b(w, **kw)


SCENES = [
    Scene('writer-b-imagem', 'Writer B · imagem selecionada', wb(context='image'), 'writer', 'B',
          note='A barra de formatação e o Inspetor trocam com a seleção.'),
    Scene('writer-b-forma', 'Writer B · forma selecionada', wb(context='shape'), 'writer', 'B'),
    Scene('writer-b-documento', 'Writer B · Documento', wb(level=shell.DOCUMENT), 'writer', 'B'),
    Scene('writer-b-busca', 'Writer B · busca e avisos', wb(find=True, banner=True), 'writer', 'B'),
    Scene('writer-b-formas', 'Writer B · popover Forma', wb(), 'writer', 'B', after=popup('cab.inserir.forma')),
    Scene('writer-b-menu', 'Writer B · menu principal', wb(), 'writer', 'B', after=popup('cab.menu')),
    Scene('writer-b-visualizacao', 'Writer B · menu Visualização', wb(), 'writer', 'B',
          after=popup('cab.visualizar')),
    Scene('writer-b-inserir', 'Writer B · menu Inserir', wb(), 'writer', 'B', after=popup('cab.inserir')),
    Scene('writer-b-fontes', 'Writer B · popover Fontes', wb(), 'writer', 'B', after=popup('fmt.fonte')),
    Scene('writer-b-cores', 'Writer B · popover Cores', wb(), 'writer', 'B', after=popup('fmt.cor-texto')),
    Scene('writer-c-inserir', 'Writer C · Inserir', lambda w: writer.version_c(w), 'writer', 'C',
          after=popup('cab.inserir-tudo')),
    Scene('tables-b-dados', 'Tables B · Dados', lambda w: tables.version_b(w, level=shell.DATA), 'tables', 'B'),
    Scene('tables-b-grafico', 'Tables B · gráfico selecionado', lambda w: tables.version_b(w, selection='chart'),
          'tables', 'B'),
    Scene('tables-b-formula', 'Tables B · editando uma fórmula',
          lambda w: tables.version_b(w, selection='cell', editing=True), 'tables', 'B'),
    Scene('tables-b-documento', 'Tables B · Documento', lambda w: tables.version_b(w, level=shell.DOCUMENT),
          'tables', 'B'),
    Scene('slides-b-slide', 'Slides B · nada selecionado', lambda w: slides.version_b(w, context='slide',
                                                                                     selection='slide'),
          'slides', 'B', note='Sem seleção, a barra de formatação e o Inspetor mostram o que é do slide.'),
    Scene('slides-b-transicao', 'Slides B · transição',
          lambda w: slides.version_b(w, context='slide', level=shell.ANIMATION, selection='slide'), 'slides', 'B'),
    Scene('slides-b-documento', 'Slides B · Documento', lambda w: slides.version_b(w, level=shell.DOCUMENT),
          'slides', 'B'),
    Scene('slides-b-animacao', 'Slides B · Animação', lambda w: slides.version_b(w, level=shell.ANIMATION),
          'slides', 'B'),
    Scene('slides-b-mesa', 'Slides B · mesa de luz', slides_light_table, 'slides', 'B'),
    Scene('comum-modelos', 'Seletor de modelos na janela', template_chooser, 'comum', 'B'),
    Scene('comum-fechar', 'Fechar com alterações', wb(), 'comum', 'B', after=close_dialog),
    Scene('comum-preferencias', 'Preferências', wb(), 'comum', 'B', after=preferences_dialog),
    Scene('comum-atalhos', 'Atalhos de teclado', wb(), 'comum', 'B', after=shortcuts_dialog),
    Scene('comum-comandos', 'Buscar comandos', wb(), 'comum', 'B', after=command_dialog),
    Scene('comum-media', 'Meia tela (640 × 720)', writer_narrow, 'comum', 'B', size=(640, 720),
          note='Metade da tela mínima, lado a lado com outra janela.'),
    Scene('comum-aviso', 'Tela menor que a mínima (1024 × 600)', wb(low_res=True), 'comum', 'B', size=(1024, 600),
          note='A faixa que avisa, na abertura, que a tela tem menos de 1280 × 720.'),
    Scene('comum-escuro', 'Estilo escuro', wb(), 'comum', 'B', dark=True),
]
