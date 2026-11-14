# SPDX-FileCopyrightText: 2026 Desk contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Desk Slides in the three versions."""
import gi

gi.require_version('Gtk', '4.0')
gi.require_version('Adw', '1')
from gi.repository import Adw, Gtk  # noqa: E402

import canvas
import parts
import shell
from kit import (Scene, action_row, button_row, classes, color_box, combo_row, dropdown, expand, group,
                 hbox, icon_button, label, menu_button, menu_model, page, scale_row, spacer, spin_row,
                 split_button, switch_row, tag, text_button, toggle_group, vbox)

APP = 'slides'
SIDE_MODES = ['Navegador', 'Tópicos', 'Slides mestres', 'Comentários', 'Resultados da busca']
LEVELS = [shell.FORMAT, shell.ANIMATION, shell.DOCUMENT]
PLAY_MENU = menu_model(['Reproduzir do início', 'Reproduzir a partir deste slide'],
                       ['Ensaiar a apresentação', 'Gravar a apresentação…'],
                       ['Tela do apresentador', 'Reproduzir em janela'])
THEMES = menu_model(['Branco', 'Preto', 'Gradiente', 'Jornal', 'Lousa'], ['Escolher tema…'])
MASTERS = ['Título e subtítulo', 'Título e marcadores', 'Título, marcadores e foto', 'Só o título',
           'Foto na horizontal', 'Em branco']
EFFECTS = ['Nenhum', 'Dissolver', 'Aparecer', 'Mover para dentro', 'Escala', 'Cubo', 'Virar', 'Movimento mágico']


def masters_popover(cid=None):
    """The masters of the theme, to add a slide or to change the one selected."""
    pop = Gtk.Popover()
    flow = Gtk.FlowBox(max_children_per_line=3, min_children_per_line=3, selection_mode=Gtk.SelectionMode.NONE,
                       row_spacing=8, column_spacing=8)
    for i, name in enumerate(MASTERS, 1):
        cell = vbox(canvas.slide_thumb(i if i < 6 else 2, w=96), label(name, 'tiny', xalign=0.5, wrap=True,
                                                                         max_width_chars=14,
                                                                         justify=Gtk.Justification.CENTER),
                    spacing=2)
        flow.append(cell)
    pop.set_child(vbox(flow, margin_top=10, margin_bottom=10, margin_start=10, margin_end=10))
    return tag(pop, cid) if cid else pop


def format_bar_slide(prefix='fmt'):
    """Version B, nothing selected: what belongs to the slide itself."""
    bar = hbox(spacing=4)
    classes(bar, 'toolbar', 'formatbar-slim')
    bar.append(menu_button(icon='desk-slide-layout', text='Título, marcadores e foto', tip='Slide mestre',
                           cid=f'{prefix}.mestre', popover=masters_popover()))
    bar.append(spacer())
    bar.append(menu_button(icon='desk-fill', tip='Fundo do slide', cid=f'{prefix}.fundo-slide',
                           popover=Gtk.Popover()))
    bar.append(spacer())
    bar.append(menu_button(icon='desk-animate', text='Dissolver', tip='Transição', cid=f'{prefix}.transicao',
                           popover=Gtk.Popover()))
    bar.append(icon_button('desk-skip-slide', 'Pular este slide', f'{prefix}.pular', toggle=True))
    bar.append(expand())
    bar.append(menu_button(icon='view-more-horizontal', tip='Mais opções de formatação', cid=f'{prefix}.mais',
                           menu=menu_model(['Editar o slide mestre', 'Reaplicar o mestre ao slide'],
                                           ['Mostrar no Inspetor'])))
    return bar


def notes_area():
    tv = Gtk.TextView(wrap_mode=Gtk.WrapMode.WORD_CHAR, top_margin=4, bottom_margin=8, left_margin=14,
                      right_margin=14)
    tv.get_buffer().set_text('Citar a pesquisa de fevereiro. Perguntar quem já usa o serviço nas três capitais '
                             'e mostrar o mapa antes de falar dos parceiros.')
    sw = Gtk.ScrolledWindow(child=tv, hscrollbar_policy=Gtk.PolicyType.NEVER, min_content_height=58)
    box = vbox(Gtk.Separator(), label('Notas do apresentador', 'notes-title', 'dimmed'), sw)
    classes(box, 'view')
    return tag(box, 'doc.notas')


def slide_view(selection='image', notes=True, overlays=()):
    cv = tag(canvas.slides_canvas(n=3, selection=selection), 'doc.slide')
    inner = cv
    if overlays:
        ov = Gtk.Overlay(child=cv)
        for w in overlays:
            ov.add_overlay(w)
        inner = ov
    return vbox(inner, notes_area()) if notes else inner


# --------------------------------------------------------------- Inspector


def slide_page(prefix='insp.slide'):
    layout = action_row('Título, marcadores e foto', subtitle='Slide mestre', arrow=True,
                        prefix=canvas.slide_thumb(3, w=56))
    return page(
        group('Layout', layout, cid=prefix + '.mestre'),
        group('Aparência', switch_row('Título', True), switch_row('Corpo', True),
              switch_row('Número do slide'), cid=prefix + '.aparencia'),
        group('Fundo', combo_row('Preenchimento', ['Do slide mestre', 'Cor', 'Gradiente', 'Imagem']),
              cid=prefix + '.fundo'),
        group(None, button_row('Editar o slide mestre'), cid=prefix + '.editar-mestre'))


def format_pages(context='image'):
    if context == 'image':
        return [('estilo', 'Estilo', None, parts.object_style_page(image=True)),
                ('imagem', 'Imagem', None, parts.image_page()),
                ('organizar', 'Organizar', None, parts.arrange_page(writer=False))], 'estilo'
    return [('slide', 'Slide', None, slide_page())], 'slide'


def build_page(prefix, effect='Dissolver', order='2 de 4'):
    return page(
        group('Efeito', action_row('Efeito', subtitle=effect, arrow=True, prop=True),
              button_row('Pré-visualizar', 'media-playback-start'), cid=prefix + '.efeito'),
        group('Opções', spin_row('Duração', 1.0, 0, 60, 0.1, digits=2, subtitle='s'),
              combo_row('Direção', ['Da esquerda para a direita', 'Da direita para a esquerda', 'De cima',
                                    'De baixo']),
              combo_row('Entrega', ['Tudo de uma vez', 'Por marcador', 'Por grupo de marcadores']),
              cid=prefix + '.opcoes'),
        group('Ordem', action_row('Posição', subtitle=order, prop=True),
              combo_row('Iniciar', ['Ao clicar', 'Com a composição anterior', 'Após a composição anterior']),
              spin_row('Atraso', 0.0, 0, 60, 0.1, digits=2, subtitle='s'),
              button_row('Ordem das composições', None, 'go-next'), cid=prefix + '.ordem'))


def transition_page(prefix='insp.transicao'):
    return page(
        group('Transição', action_row('Efeito', subtitle='Dissolver', arrow=True, prop=True),
              spin_row('Duração', 1.0, 0, 60, 0.1, digits=2, subtitle='s'),
              combo_row('Direção', ['Da esquerda para a direita', 'Da direita para a esquerda']),
              button_row('Pré-visualizar', 'media-playback-start'), cid=prefix + '.efeito'),
        group('Iniciar transição', combo_row('Iniciar', ['Ao clicar', 'Automaticamente']),
              spin_row('Atraso', 0.5, 0, 600, 0.5, digits=2, subtitle='s'), cid=prefix + '.inicio'))


def animation_pages(context='image'):
    if context == 'image':
        action = page(group('Ação', combo_row('Efeito', ['Nenhum', 'Mover', 'Opacidade', 'Girar', 'Escala']),
                            cid='insp.acao.efeito'))
        return [('entrada', 'Entrada', None, build_page('insp.entrada')),
                ('acao', 'Ação', None, action),
                ('saida', 'Saída', None, build_page('insp.saida', 'Nenhum', '—'))], 'entrada'
    return [('transicao', 'Transição', None, transition_page())], 'transicao'


def document_pages():
    show = page(
        group('Apresentação', combo_row('Tipo', ['Normal', 'Somente links', 'Gravada', 'Reprodução automática']),
              switch_row('Reproduzir ao abrir'), switch_row('Repetir a apresentação'),
              switch_row('Reiniciar se ficar parada'), switch_row('Exigir senha para sair'),
              cid='insp.apres.opcoes'),
        group('Tamanho do slide', combo_row('Tamanho', ['1920 × 1080 (16:9)', '1280 × 720 (16:9)',
                                                         '1024 × 768 (4:3)', '800 × 600 (4:3)', 'Personalizado…']),
              cid='insp.apres.tamanho'),
        group('Tema', action_row('Branco', subtitle='Tema da apresentação', prop=True),
              button_row('Trocar de tema…'), cid='insp.apres.tema'))
    audio = page(
        group('Trilha sonora', combo_row('Reprodução', ['Desligada', 'Uma vez', 'Em repetição']),
              action_row('abertura.ogg', subtitle='2 min 14 s', prop=False), scale_row('Volume', 80),
              button_row('Escolher áudio…'), cid='insp.audio.trilha'),
        group('Gravação', action_row('Narração', subtitle='Nenhuma gravação', prop=True),
              button_row('Gravar…', 'media-record'), cid='insp.audio.gravacao'))
    return [('apres', 'Documento', None, show), ('audio', 'Áudio', None, audio),
            ('info', 'Info', None, parts.info_page())], 'apres'


def inspector_b(context='image', level=shell.FORMAT):
    title = {'image': 'Imagem', 'slide': 'Slide'}[context]
    if level == shell.ANIMATION:
        pages, vis = animation_pages(context)
    elif level == shell.DOCUMENT:
        pages, vis = document_pages()
        title = None
    else:
        pages, vis = format_pages(context)
    return shell.inspector_b(LEVELS, level, title, None, pages, vis)


def inspector_c(context='image', level=shell.FORMAT):
    pages, vis = {shell.ANIMATION: animation_pages, shell.FORMAT: format_pages}.get(
        level, lambda c: document_pages())(context)
    return shell.inspector_c(None, pages, vis)


A_ICONS = [('doc', 'x-office-document', 'Documento'), ('slide', 'desk-slide-layout', 'Slide'),
           ('composicao', 'desk-animate', 'Composição'), ('texto', 'desk-character-style', 'Texto'),
           ('grafica', 'desk-fill', 'Gráfico'), ('metricas', 'desk-ruler', 'Métricas'),
           ('tabela', 'desk-table', 'Tabela'), ('grafico', 'desk-chart', 'Gráfico (dados)'),
           ('link', 'desk-link', 'Link'), ('midia', 'desk-media', 'Mídia')]


def inspector_a():
    cg, cr = shell.compact_group, shell.compact_row
    preview = canvas.slide_thumb(3, w=150)
    preview.set_halign(Gtk.Align.CENTER)
    groups = [
        tag(preview, 'insp.a.previa'),
        cg('Transição', shell.combo_line('Efeito', EFFECTS, 1), cr('Duração', 1.0, 's', 2),
           shell.combo_line('Direção', ['Para a direita', 'Para a esquerda']), cid='insp.a.transicao'),
        cg('Iniciar transição', shell.combo_line('Iniciar', ['Ao clicar', 'Automaticamente']),
           cr('Atraso', 0.5, 's', 2), cid='insp.a.inicio'),
    ]
    return shell.inspector_a(A_ICONS, 1, 'Slide', ['Transição', 'Aparência'], groups)


# ---------------------------------------------------------------- versions


def navigator(count=6, selected=3, width=130, cid='lat.navegador'):
    return parts.thumbs_list('slide', count, selected, cid=cid, width=width)


def version_a(win, selection='image'):
    start = [text_button('Abrir', 'Abrir um documento', 'cab.abrir'),
             icon_button('list-add', 'Novo slide', 'cab.novo-slide'),
             icon_button('media-playback-start', 'Reproduzir', 'cab.reproduzir'),
             menu_button(text='Visualização', tip='Menu Visualização', cid='cab.visualizar-menu',
                         menu=parts.view_menu(APP)),
             menu_button(text='Temas', tip='Temas', cid='cab.temas', menu=THEMES),
             menu_button(text='Mestres', tip='Slides mestres', cid='cab.mestres', popover=masters_popover()),
             menu_button(text='Inserir', tip='Menu Inserir', cid='cab.inserir-menu',
                         menu=parts.insert_menu(APP, objects=True))]
    end = [menu_button(icon='open-menu', tip='Menu principal', cid='cab.menu', menu=parts.main_menu(APP)),
           icon_button('sidebar-show-right', 'Inspetor', 'cab.inspetor', toggle=True, active=True),
           icon_button('image-x-generic', 'Mídia', 'cab.midia', toggle=True)]
    hb = shell.header_bar(APP, start, end)
    fbar = tag(parts.format_bar_object('fmt', 'image'), 'fmt.barra')
    nav = navigator(6, 3, 104)
    nav.set_size_request(146, -1)
    inner = shell.split(inspector_a(), slide_view(selection), shell.INSPECTOR_A_W, Gtk.PackType.END)
    body = shell.split(nav, inner, 146)
    bottom = shell.bottom_bar(parts.flat_menu('58 %', 'Zoom', 'inf.zoom', menu=parts.zoom_menu()), expand(),
                              tag(label('Slide 3 de 12', 'dimmed', 'tiny', 'numeric', margin_end=8),
                                  'inf.contagem'))
    win.set_content(shell.toolbar_view(body, top=[hb, fbar], bottom=[bottom]))


def version_b(win, context='image', level=shell.FORMAT, selection='image'):
    side = shell.nav_sidebar(APP, navigator(), SIDE_MODES, 'Adicionar slide', add_popover=masters_popover())
    start = [split_button(icon='sidebar-show', tip='Barra lateral', drop_tip='Visualização',
                          cid='cab.visualizar', menu=parts.view_menu(APP)),
             spacer(), parts.insert_menu_button(APP)]
    start += parts.insert_buttons()
    end = [menu_button(icon='open-menu', tip='Menu principal', cid='cab.menu', menu=parts.main_menu(APP)),
           icon_button('sidebar-show-right', 'Inspetor', 'cab.inspetor', toggle=True, active=True),
           icon_button('edit-find', 'Buscar', 'cab.localizar', toggle=True),
           spacer(),
           split_button(icon='media-playback-start', tip='Reproduzir', drop_tip='Opções da apresentação',
                        cid='cab.reproduzir', menu=PLAY_MENU)]
    hb = shell.header_bar(APP, start, end)
    if context == 'slide':
        fbar = tag(format_bar_slide('fmt'), 'fmt.barra')
    else:
        fbar = tag(parts.format_bar_object('fmt', 'image', slim=True), 'fmt.barra')
    body = shell.split(inspector_b(context, level), slide_view(selection), shell.INSPECTOR_W, Gtk.PackType.END)
    bottom = shell.bottom_bar(
        parts.flat_menu('58 %', 'Zoom', 'inf.zoom', menu=parts.zoom_menu()),
        expand(),
        icon_button('go-up', 'Slide anterior', 'inf.anterior'),
        parts.flat_menu('Slide 3 de 12', 'Ir para o slide', 'inf.pagina', arrow=False,
                        menu=menu_model(['Ir para o slide…'], ['Primeiro slide', 'Último slide'],
                                        ['Pular este slide'])),
        icon_button('go-down', 'Slide seguinte', 'inf.seguinte'),
        expand(),
        icon_button('desk-notes', 'Notas do apresentador', 'inf.notas', toggle=True, active=True))
    content = shell.toolbar_view(body, top=[hb, fbar], bottom=[bottom])
    win.set_content(shell.split(side, content, shell.SIDEBAR_W))


def version_c(win, context='image', level=shell.FORMAT, selection='image'):
    side = shell.nav_sidebar(APP, navigator(), SIDE_MODES, 'Adicionar slide', add_popover=masters_popover())
    start = [icon_button('sidebar-show', 'Barra lateral', 'cab.lateral', toggle=True, active=True),
             menu_button(icon='desk-insert', tip='Inserir', cid='cab.inserir-tudo', popover=Gtk.Popover()),
             icon_button('media-playback-start', 'Reproduzir', 'cab.reproduzir')]
    end = [menu_button(icon='open-menu', tip='Menu principal', cid='cab.menu',
                       menu=parts.main_menu(APP, lean=True)),
           spacer(),
           icon_button('x-office-document', 'Documento', 'cab.documento', toggle=True,
                       active=(level == shell.DOCUMENT)),
           icon_button('desk-animate', 'Animar', 'cab.animar', toggle=True, active=(level == shell.ANIMATION)),
           icon_button('desk-format', 'Formatar', 'cab.formatar', toggle=True, active=(level == shell.FORMAT)),
           spacer(),
           icon_button('edit-find', 'Buscar', 'cab.localizar', toggle=True)]
    hb = shell.header_bar(APP, start, end)
    zoom = shell.osd_pill(icon_button('zoom-out', 'Reduzir'), label('62 %', 'numeric', margin_start=4, margin_end=4),
                          icon_button('zoom-in', 'Ampliar'), cid='osd.zoom',
                          halign=Gtk.Align.END, valign=Gtk.Align.END, margin_end=14, margin_bottom=12)
    body = shell.split(inspector_c(context, level), slide_view(selection, notes=False, overlays=(zoom,)),
                       shell.INSPECTOR_W, Gtk.PackType.END)
    content = shell.toolbar_view(body, top=[hb])
    win.set_content(shell.split(side, content, shell.SIDEBAR_W))


SCENES = [
    Scene('slides-a', 'Desk Slides · A Clássica', lambda w: version_a(w), APP, 'A'),
    Scene('slides-b', 'Desk Slides · B Equilibrada', lambda w: version_b(w), APP, 'B'),
    Scene('slides-c', 'Desk Slides · C Enxuta', lambda w: version_c(w), APP, 'C'),
]
