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
SIDE_MODES = ['Navigator', 'Outline', 'Slide layouts', 'Comments', 'Search results']
LEVELS = [shell.FORMAT, shell.ANIMATION, shell.DOCUMENT]
PLAY_MENU = menu_model(['Play from Start', 'Play from This Slide'],
                       ['Rehearse Slideshow', 'Record Slideshow…'],
                       ['Presenter display', 'Play in Window'])
THEMES = menu_model(['White', 'Black', 'Gradient', 'Newsprint', 'Chalkboard'], ['Choose theme…'])
MASTERS = ['Title and subtitle', 'Title and bullets', 'Title, bullets and photo', 'Title only',
           'Photo, horizontal', 'Blank']
EFFECTS = ['None', 'Dissolve', 'Appear', 'Move in', 'Scale', 'Cube', 'Flip', 'Magic Move']


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
    bar.append(menu_button(icon='desk-slide-layout', text='Title, bullets and photo', tip='Slide layout',
                           cid=f'{prefix}.master', popover=masters_popover()))
    bar.append(spacer())
    bar.append(menu_button(icon='desk-fill', tip='Slide background', cid=f'{prefix}.slide-background',
                           popover=Gtk.Popover()))
    bar.append(spacer())
    bar.append(menu_button(icon='desk-animate', text='Dissolve', tip='Transition', cid=f'{prefix}.transition',
                           popover=Gtk.Popover()))
    bar.append(icon_button('desk-skip-slide', 'Skip this slide', f'{prefix}.skip', toggle=True))
    bar.append(expand())
    bar.append(menu_button(icon='view-more-horizontal', tip='More format options', cid=f'{prefix}.more',
                           menu=menu_model(['Edit slide layout', 'Reapply layout to slide'],
                                           ['Show in Inspector'])))
    return bar


def notes_area():
    tv = Gtk.TextView(wrap_mode=Gtk.WrapMode.WORD_CHAR, top_margin=4, bottom_margin=8, left_margin=14,
                      right_margin=14)
    tv.get_buffer().set_text('Mention the February survey. Ask who already uses the service in the three capitals '
                             'and show the map before talking about the partners.')
    sw = Gtk.ScrolledWindow(child=tv, hscrollbar_policy=Gtk.PolicyType.NEVER, min_content_height=58)
    box = vbox(Gtk.Separator(), label('Presenter notes', 'notes-title', 'dimmed'), sw)
    classes(box, 'view')
    return tag(box, 'doc.notes')


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
    layout = action_row('Title, bullets and photo', subtitle='Slide layout', arrow=True,
                        prefix=canvas.slide_thumb(3, w=56))
    return page(
        group('Layout', layout, cid=prefix + '.master'),
        group('Appearance', switch_row('Title', True), switch_row('Body', True),
              switch_row('Slide number'), cid=prefix + '.appearance'),
        group('Background', combo_row('Fill', ['From the slide layout', 'Color', 'Gradient', 'Image']),
              cid=prefix + '.back'),
        group(None, button_row('Edit slide layout'), cid=prefix + '.edit-master'))


def format_pages(context='image'):
    if context == 'image':
        return [('style', 'Style', None, parts.object_style_page(image=True)),
                ('image', 'Image', None, parts.image_page()),
                ('arrange', 'Arrange', None, parts.arrange_page(writer=False))], 'style'
    return [('slide', 'Slide', None, slide_page())], 'slide'


def build_page(prefix, effect='Dissolve', order='2 of 4'):
    return page(
        group('Effect', action_row('Effect', subtitle=effect, arrow=True, prop=True),
              button_row('Preview', 'media-playback-start'), cid=prefix + '.effect'),
        group('Options', spin_row('Duration', 1.0, 0, 60, 0.1, digits=2, subtitle='s'),
              combo_row('Direction', ['Left to right', 'Right to left', 'From top',
                                    'From bottom']),
              combo_row('Delivery', ['All at once', 'By bullet', 'By bullet group']),
              cid=prefix + '.options'),
        group('Order', action_row('Position', subtitle=order, prop=True),
              combo_row('Start', ['On click', 'With previous build', 'After previous build']),
              spin_row('Delay', 0.0, 0, 60, 0.1, digits=2, subtitle='s'),
              button_row('Build order', None, 'go-next'), cid=prefix + '.order'))


def transition_page(prefix='insp.transition'):
    return page(
        group('Transition', action_row('Effect', subtitle='Dissolve', arrow=True, prop=True),
              spin_row('Duration', 1.0, 0, 60, 0.1, digits=2, subtitle='s'),
              combo_row('Direction', ['Left to right', 'Right to left']),
              button_row('Preview', 'media-playback-start'), cid=prefix + '.effect'),
        group('Start transition', combo_row('Start', ['On click', 'Automatically']),
              spin_row('Delay', 0.5, 0, 600, 0.5, digits=2, subtitle='s'), cid=prefix + '.start'))


def animation_pages(context='image'):
    if context == 'image':
        action = page(group('Action', combo_row('Effect', ['None', 'Move', 'Opacity', 'Rotate', 'Scale']),
                            cid='insp.action.effect'))
        return [('build-in', 'Build In', None, build_page('insp.build-in')),
                ('action', 'Action', None, action),
                ('build-out', 'Build Out', None, build_page('insp.build-out', 'None', '—'))], 'build-in'
    return [('transition', 'Transition', None, transition_page())], 'transition'


def document_pages():
    show = page(
        group('Presentation', combo_row('Type', ['Normal', 'Links only', 'Recorded', 'Self-playing']),
              switch_row('Play on open'), switch_row('Loop slideshow'),
              switch_row('Restart if idle'), switch_row('Require password to exit'),
              cid='insp.show.options'),
        group('Slide size', combo_row('Size', ['1920 × 1080 (16:9)', '1280 × 720 (16:9)',
                                                         '1024 × 768 (4:3)', '800 × 600 (4:3)', 'Custom…']),
              cid='insp.show.size'),
        group('Theme', action_row('White', subtitle='Presentation theme', prop=True),
              button_row('Change theme…'), cid='insp.show.theme'))
    audio = page(
        group('Soundtrack', combo_row('Playback', ['Off', 'Once', 'Loop']),
              action_row('opening.ogg', subtitle='2 min 14 s', prop=False), scale_row('Volume', 80),
              button_row('Choose audio…'), cid='insp.audio.soundtrack'),
        group('Recording', action_row('Narration', subtitle='No recording', prop=True),
              button_row('Record…', 'media-record'), cid='insp.audio.recording'))
    return [('show', 'Document', None, show), ('audio', 'Audio', None, audio),
            ('info', 'Info', None, parts.info_page())], 'show'


def inspector_b(context='image', level=shell.FORMAT):
    title = {'image': 'Image', 'slide': 'Slide'}[context]
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


A_ICONS = [('doc', 'x-office-document', 'Document'), ('slide', 'desk-slide-layout', 'Slide'),
           ('build', 'desk-animate', 'Build'), ('text', 'desk-character-style', 'Text'),
           ('graphic', 'desk-fill', 'Graphic'), ('metrics', 'desk-ruler', 'Metrics'),
           ('table', 'desk-table', 'Table'), ('chart', 'desk-chart', 'Chart (data)'),
           ('link', 'desk-link', 'Link'), ('media', 'desk-media', 'Media')]


def inspector_a():
    cg, cr = shell.compact_group, shell.compact_row
    preview = canvas.slide_thumb(3, w=150)
    preview.set_halign(Gtk.Align.CENTER)
    groups = [
        tag(preview, 'insp.a.preview'),
        cg('Transition', shell.combo_line('Effect', EFFECTS, 1), cr('Duration', 1.0, 's', 2),
           shell.combo_line('Direction', ['To the right', 'To the left']), cid='insp.a.transition'),
        cg('Start transition', shell.combo_line('Start', ['On click', 'Automatically']),
           cr('Delay', 0.5, 's', 2), cid='insp.a.start'),
    ]
    return shell.inspector_a(A_ICONS, 1, 'Slide', ['Transition', 'Appearance'], groups)


# ---------------------------------------------------------------- versions


def navigator(count=6, selected=3, width=130, cid='side.navigator'):
    return parts.thumbs_list('slide', count, selected, cid=cid, width=width)


def version_a(win, selection='image'):
    start = [text_button('Open', 'Open a document', 'head.open'),
             icon_button('list-add', 'New slide', 'head.new-slide'),
             icon_button('media-playback-start', 'Play', 'head.play'),
             menu_button(text='View', tip='View menu', cid='head.view-menu',
                         menu=parts.view_menu(APP)),
             menu_button(text='Themes', tip='Themes', cid='head.themes', menu=THEMES),
             menu_button(text='Layouts', tip='Slide layouts', cid='head.masters', popover=masters_popover()),
             menu_button(text='Insert', tip='Insert menu', cid='head.insert-menu',
                         menu=parts.insert_menu(APP, objects=True))]
    end = [menu_button(icon='open-menu', tip='Main menu', cid='head.menu', menu=parts.main_menu(APP)),
           icon_button('sidebar-show-right', 'Inspector', 'head.inspector', toggle=True, active=True),
           icon_button('image-x-generic', 'Media', 'head.media', toggle=True)]
    hb = shell.header_bar(APP, start, end)
    fbar = tag(parts.format_bar_object('fmt', 'image'), 'fmt.bar')
    nav = navigator(6, 3, 104)
    nav.set_size_request(146, -1)
    inner = shell.split(inspector_a(), slide_view(selection), shell.INSPECTOR_A_W, Gtk.PackType.END)
    body = shell.split(nav, inner, 146)
    bottom = shell.bottom_bar(parts.flat_menu('58%', 'Zoom', 'bottom.zoom', menu=parts.zoom_menu()), expand(),
                              tag(label('Slide 3 of 12', 'dimmed', 'tiny', 'numeric', margin_end=8),
                                  'bottom.count'))
    win.set_content(shell.toolbar_view(body, top=[hb, fbar], bottom=[bottom]))


def version_b(win, context='image', level=shell.FORMAT, selection='image'):
    side = shell.nav_sidebar(APP, navigator(), SIDE_MODES, 'Add slide', add_popover=masters_popover())
    start = [split_button(icon='sidebar-show', tip='Sidebar', drop_tip='View',
                          cid='head.view', menu=parts.view_menu(APP)),
             spacer(), parts.insert_menu_button(APP)]
    start += parts.insert_buttons()
    end = [menu_button(icon='open-menu', tip='Main menu', cid='head.menu', menu=parts.main_menu(APP)),
           icon_button('sidebar-show-right', 'Inspector', 'head.inspector', toggle=True, active=True),
           icon_button('edit-find', 'Find', 'head.find', toggle=True),
           spacer(),
           split_button(icon='media-playback-start', tip='Play', drop_tip='Slideshow options',
                        cid='head.play', menu=PLAY_MENU)]
    hb = shell.header_bar(APP, start, end)
    if context == 'slide':
        fbar = tag(format_bar_slide('fmt'), 'fmt.bar')
    else:
        fbar = tag(parts.format_bar_object('fmt', 'image', slim=True), 'fmt.bar')
    body = shell.split(inspector_b(context, level), slide_view(selection), shell.INSPECTOR_W, Gtk.PackType.END)
    bottom = shell.bottom_bar(
        parts.flat_menu('58%', 'Zoom', 'bottom.zoom', menu=parts.zoom_menu()),
        expand(),
        icon_button('go-up', 'Previous slide', 'bottom.previous'),
        parts.flat_menu('Slide 3 of 12', 'Go to slide', 'bottom.page', arrow=False,
                        menu=menu_model(['Go to slide…'], ['First slide', 'Last slide'],
                                        ['Skip this slide'])),
        icon_button('go-down', 'Next slide', 'bottom.next'),
        expand(),
        icon_button('desk-notes', 'Presenter notes', 'bottom.notes', toggle=True, active=True))
    content = shell.toolbar_view(body, top=[hb, fbar], bottom=[bottom])
    win.set_content(shell.split(side, content, shell.SIDEBAR_W))


def version_c(win, context='image', level=shell.FORMAT, selection='image'):
    side = shell.nav_sidebar(APP, navigator(), SIDE_MODES, 'Add slide', add_popover=masters_popover())
    start = [icon_button('sidebar-show', 'Sidebar', 'head.sidebar', toggle=True, active=True),
             menu_button(icon='desk-insert', tip='Insert', cid='head.insert-all', popover=Gtk.Popover()),
             icon_button('media-playback-start', 'Play', 'head.play')]
    end = [menu_button(icon='open-menu', tip='Main menu', cid='head.menu',
                       menu=parts.main_menu(APP, lean=True)),
           spacer(),
           icon_button('x-office-document', 'Document', 'head.document', toggle=True,
                       active=(level == shell.DOCUMENT)),
           icon_button('desk-animate', 'Animate', 'head.animate', toggle=True, active=(level == shell.ANIMATION)),
           icon_button('desk-format', 'Format', 'head.format', toggle=True, active=(level == shell.FORMAT)),
           spacer(),
           icon_button('edit-find', 'Find', 'head.find', toggle=True)]
    hb = shell.header_bar(APP, start, end)
    zoom = shell.osd_pill(icon_button('zoom-out', 'Zoom out'), label('62%', 'numeric', margin_start=4, margin_end=4),
                          icon_button('zoom-in', 'Zoom in'), cid='osd.zoom',
                          halign=Gtk.Align.END, valign=Gtk.Align.END, margin_end=14, margin_bottom=12)
    body = shell.split(inspector_c(context, level), slide_view(selection, notes=False, overlays=(zoom,)),
                       shell.INSPECTOR_W, Gtk.PackType.END)
    content = shell.toolbar_view(body, top=[hb])
    win.set_content(shell.split(side, content, shell.SIDEBAR_W))


SCENES = [
    Scene('slides-a', 'Desk Slides · A Classic', lambda w: version_a(w), APP, 'A'),
    Scene('slides-b', 'Desk Slides · B Equilibrada', lambda w: version_b(w), APP, 'B'),
    Scene('slides-c', 'Desk Slides · C Enxuta', lambda w: version_c(w), APP, 'C'),
]
