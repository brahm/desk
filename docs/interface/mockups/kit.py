# SPDX-FileCopyrightText: 2026 Desk contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Building blocks of the Desk interface mockups.

The mockups are real GTK 4.24 / Libadwaita 1.10 windows, drawn by the
toolkit itself and saved as PNG, so that every component in them is a widget
that exists. Each widget that matters is tagged with the id of a row of the
component inventory; the renderer writes the place and the GType of each
tagged widget beside the picture.
"""
import json
import os

import gi

gi.require_version('Gtk', '4.0')
gi.require_version('Adw', '1')
gi.require_version('Gsk', '4.0')
gi.require_version('PangoCairo', '1.0')
from gi.repository import Adw, Gdk, Gio, GLib, GObject, Graphene, Gtk, Pango  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
SCALE = 2

CSS = """
:root { --accent-bg-color: #3584e4; --accent-fg-color: #ffffff; }
.canvas { background-color: color-mix(in srgb, var(--window-bg-color) 82%, black); }
.canvas-dark { background-color: #2b2b2e; }
.thumb-label { font-size: 0.8em; }
.thumb-row { padding: 6px 4px; }
.formatbar { padding: 4px 6px; border-spacing: 4px; }
.formatbar-slim { padding: 3px 6px; border-spacing: 2px; min-height: 0; }
.bottombar { padding: 2px 6px; border-spacing: 4px; min-height: 0; }
.bottombar button { min-height: 26px; padding: 0 8px; }
.inspector { background-color: var(--sidebar-bg-color); }
.inspector-page { padding: 12px; }
.inspector-switch { margin: 8px 12px 0 12px; }
.pane-title { font-weight: 700; font-size: 1.1em; }
.swatch { min-width: 22px; min-height: 22px; border-radius: 6px; border: 1px solid alpha(black, 0.18); }
.swatch-big { min-width: 34px; min-height: 26px; border-radius: 7px; border: 1px solid alpha(black, 0.18); }
.style-chip { min-width: 36px; min-height: 30px; border-radius: 6px; border: 1px solid alpha(black, 0.15); }
.chip-button { padding: 3px; min-width: 0; min-height: 0; }
.notes-title { font-size: 0.82em; font-weight: 700; padding: 6px 14px 0 14px; }
.slide-number { font-size: 0.8em; min-width: 18px; }
.kbd-hint { font-size: 0.82em; }
.template-card { padding: 8px; border-radius: 12px; }
.template-card.selected { background-color: alpha(var(--accent-bg-color), 0.18); }
.font-sample { font-size: 1.15em; }
.palette { border-spacing: 3px; }
.palette-cell { min-width: 24px; min-height: 24px; border-radius: 5px; border: 1px solid alpha(black, 0.12); }
.sheet-handle-bar { padding: 8px 14px; }
.formula-entry { font-family: 'Adwaita Mono', monospace; }
.cellref { min-width: 54px; }
.notes { background-color: var(--view-bg-color); padding: 10px 14px; }
.osd-pill { border-radius: 999px; padding: 2px 6px; }
.compact-row { min-height: 34px; }
.icon-tabs button { min-width: 30px; padding: 4px; }
.tiny { font-size: 0.82em; }
.gallery-item { padding: 6px; border-radius: 10px; }
.calc-chip { padding: 2px 10px; border-radius: 999px; background-color: alpha(currentColor, 0.08); }
.sheet-tab-strip { padding: 0 6px; }
"""

_REG = []


def init():
    # the accent of upstream GNOME, whatever the distribution of the build machine says
    try:
        Gio.Settings.new('org.gnome.desktop.interface').set_string('accent-color', 'blue')
    except Exception:
        pass
    Adw.init()
    st = Gtk.Settings.get_default()
    st.set_property('gtk-font-name', 'Adwaita Sans 11')
    st.set_property('gtk-icon-theme-name', 'Adwaita')
    display = Gdk.Display.get_default()
    Gtk.IconTheme.get_for_display(display).add_search_path(os.path.join(HERE, 'icons'))
    prov = Gtk.CssProvider()
    prov.load_from_string(CSS)
    Gtk.StyleContext.add_provider_for_display(display, prov, Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION)


# ---------------------------------------------------------------- inventory


def tag(widget, cid, region=None):
    """Mark a widget (or a rectangle inside it) as component `cid`."""
    _REG.append((widget, cid, region))
    return widget


def reset_tags():
    _REG.clear()


def find(cid):
    """The first widget tagged `cid` in the scene being built."""
    for widget, c, _region in _REG:
        if c == cid:
            return widget
    raise KeyError(cid)


def collect(win):
    """Places of the tagged widgets that are on screen, in reading order."""
    items = []
    seen = set()
    for widget, cid, region in _REG:
        if not widget.get_mapped():
            continue
        root = widget.get_root()
        ok, b = widget.compute_bounds(win if root is win else root)
        if not ok:
            continue
        x, y, w, h = b.get_x(), b.get_y(), b.get_width(), b.get_height()
        if root is not win:
            # a popover: place it by the position of its surface in the window
            surf = root.get_surface() if hasattr(root, 'get_surface') else None
            native = widget.get_native()
            ok, nb = widget.compute_bounds(native)
            s = native.get_surface()
            x, y = s.get_position_x() + nb.get_x(), s.get_position_y() + nb.get_y()
            w, h = nb.get_width(), nb.get_height()
        if region is not None:
            rx, ry, rw, rh = region
            x, y, w, h = x + rx, y + ry, rw, rh
        if w < 2 or h < 2 or (cid, round(x), round(y)) in seen:
            continue
        seen.add((cid, round(x), round(y)))
        items.append({'id': cid, 'gtype': type(widget).__gtype__.name,
                      'x': round(x, 1), 'y': round(y, 1), 'w': round(w, 1), 'h': round(h, 1)})
    items.sort(key=lambda i: (round(i['y'] / 28), i['x']))
    for n, item in enumerate(items, 1):
        item['n'] = n
    return items


def hidden(items):
    """Tagged components that the scene has but does not show (pages behind a tab)."""
    shown = {i['id'] for i in items}
    out = {}
    for widget, cid, _region in _REG:
        if cid not in shown and cid not in out:
            out[cid] = type(widget).__gtype__.name
    return [{'id': cid, 'gtype': gtype} for cid, gtype in out.items()]


# ------------------------------------------------------------------ capture


def _popovers(win):
    """Open popovers that belong to `win`, with their place in it."""
    found = []

    def walk(w):
        child = w.get_first_child()
        while child is not None:
            if isinstance(child, Gtk.Popover) and child.get_mapped():
                found.append(child)
            walk(child)
            child = child.get_next_sibling()
    walk(win)
    out = []
    for pop in found:
        surf = pop.get_native().get_surface()
        out.append((pop, surf.get_position_x(), surf.get_position_y()))
    return out


def capture(win, path):
    """Save the window, with its open popovers, as a PNG at SCALE."""
    w, h = win.get_width(), win.get_height()
    snap = Gtk.Snapshot()
    snap.scale(SCALE, SCALE)
    Gtk.WidgetPaintable.new(win).snapshot(snap, w, h)
    for pop, x, y in _popovers(win):
        snap.save()
        snap.translate(Graphene.Point().init(x, y))
        Gtk.WidgetPaintable.new(pop).snapshot(snap, pop.get_width(), pop.get_height())
        snap.restore()
    node = snap.to_node()
    renderer = win.get_native().get_renderer()
    texture = renderer.render_texture(node, Graphene.Rect().init(0, 0, w * SCALE, h * SCALE))
    texture.save_to_png(path)
    return w, h


class Scene:
    """One picture: a window, and what to do with it before the shot."""

    def __init__(self, sid, title, build, app='', version='', size=(1280, 800), after=None,
                 dark=False, note=''):
        self.sid, self.title, self.build = sid, title, build
        self.app, self.version, self.size = app, version, size
        self.after, self.dark, self.note = after, dark, note


def run(scenes, outdir, only=None):
    """Render every scene to `outdir`/<id>.png and <id>.json."""
    os.makedirs(outdir, exist_ok=True)
    app = Adw.Application(application_id='br.com.brahm.Desk.Mockups',
                          flags=Gio.ApplicationFlags.NON_UNIQUE)
    queue = [s for s in scenes if not only or any(s.sid.startswith(o) for o in only)]
    state = {}

    def next_scene():
        if state.get('win') is not None:
            state['win'].destroy()
            state['win'] = None
        if not queue:
            app.quit()
            return False
        scene = queue.pop(0)
        reset_tags()
        Adw.StyleManager.get_default().set_color_scheme(
            Adw.ColorScheme.FORCE_DARK if scene.dark else Adw.ColorScheme.FORCE_LIGHT)
        win = Adw.ApplicationWindow(application=app, default_width=scene.size[0],
                                    default_height=scene.size[1], title=scene.title)
        tag(win, 'janela')
        actions = Gio.SimpleActionGroup()
        actions.add_action(Gio.SimpleAction.new('none', None))
        win.insert_action_group('mock', actions)
        state['win'], state['scene'] = win, scene
        scene.build(win)
        win.present()
        win.remove_css_class('solid-csd')
        win.add_css_class('csd')
        GLib.timeout_add(700, after_layout)
        return False

    def after_layout():
        scene = state['scene']
        if scene.after is not None:
            scene.after(state['win'])
            GLib.timeout_add(600, shoot)
        else:
            shoot()
        return False

    def shoot():
        scene, win = state['scene'], state['win']
        png = os.path.join(outdir, scene.sid + '.png')
        w, h = capture(win, png)
        items = collect(win)
        data = {'id': scene.sid, 'title': scene.title, 'app': scene.app, 'version': scene.version,
                'dark': scene.dark, 'note': scene.note, 'width': w, 'height': h, 'scale': SCALE,
                'items': items, 'hidden': hidden(items)}
        with open(os.path.join(outdir, scene.sid + '.json'), 'w') as f:
            json.dump(data, f, ensure_ascii=False, indent=1)
        print(f'{scene.sid}: {w}x{h}, {len(data["items"])} components', flush=True)
        GLib.timeout_add(80, next_scene)
        return False

    app.connect('activate', lambda a: (a.hold(), GLib.idle_add(next_scene)))
    app.run([])


# ------------------------------------------------------------------ widgets


def classes(widget, *names):
    for n in names:
        widget.add_css_class(n)
    return widget


def hbox(*children, spacing=6, **kw):
    box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=spacing, **kw)
    for ch in children:
        if ch is not None:
            box.append(ch)
    return box


def vbox(*children, spacing=0, **kw):
    box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=spacing, **kw)
    for ch in children:
        if ch is not None:
            box.append(ch)
    return box


def spacer():
    return classes(Gtk.Separator(orientation=Gtk.Orientation.VERTICAL), 'spacer')


def vsep():
    return Gtk.Separator(orientation=Gtk.Orientation.VERTICAL, margin_top=6, margin_bottom=6)


def expand():
    return Gtk.Box(hexpand=True)


def label(text, *css, xalign=0.0, **kw):
    lab = Gtk.Label(label=text, xalign=xalign, **kw)
    return classes(lab, *css)


def icon_button(icon, tip, cid=None, toggle=False, active=False, **kw):
    cls = Gtk.ToggleButton if toggle else Gtk.Button
    b = cls(icon_name=icon + '-symbolic', tooltip_text=tip, **kw)
    if toggle:
        b.set_active(active)
    return tag(b, cid) if cid else b


def text_button(text, tip=None, cid=None, *css, **kw):
    b = classes(Gtk.Button(label=text, tooltip_text=tip, **kw), *css)
    return tag(b, cid) if cid else b


def menu_model(*sections):
    """A GMenu from sections; each section is a list of labels or (label, submenu)."""
    menu = Gio.Menu()
    for sec in sections:
        part = Gio.Menu()
        title = None
        for entry in sec:
            if isinstance(entry, dict):
                title = entry.get('title')
            elif isinstance(entry, tuple):
                part.append_submenu(entry[0], menu_model(*entry[1]))
            else:
                part.append(entry, 'mock.none')
        menu.append_section(title, part)
    return menu


def menu_button(icon=None, text=None, tip=None, cid=None, menu=None, popover=None, arrow=None,
                max_chars=None, **kw):
    b = Gtk.MenuButton(tooltip_text=tip, **kw)
    if text and max_chars:
        lab = Gtk.Label(label=text, ellipsize=Pango.EllipsizeMode.END, max_width_chars=max_chars, xalign=0)
        box = Gtk.Box(spacing=4)
        if icon:
            box.append(Gtk.Image(icon_name=icon + '-symbolic'))
        box.append(lab)
        box.append(Gtk.Image(icon_name='pan-down-symbolic'))
        b.set_child(box)
        arrow = False
    elif icon and text:
        b.set_child(Adw.ButtonContent(icon_name=icon + '-symbolic', label=text))
        if arrow is None:
            arrow = True
    elif icon:
        b.set_icon_name(icon + '-symbolic')
    elif text:
        b.set_label(text)
        if arrow is None:
            arrow = True
    if arrow:
        b.set_always_show_arrow(True)
    if popover is not None:
        b.set_popover(popover)
    else:
        b.set_menu_model(menu if menu is not None else menu_model(['—']))
    return tag(b, cid) if cid else b


def split_button(icon=None, text=None, tip=None, drop_tip=None, cid=None, menu=None, popover=None):
    b = Adw.SplitButton(tooltip_text=tip)
    if icon and text:
        b.set_child(Adw.ButtonContent(icon_name=icon + '-symbolic', label=text))
    elif icon:
        b.set_icon_name(icon + '-symbolic')
    else:
        b.set_label(text)
    if drop_tip:
        b.set_dropdown_tooltip(drop_tip)
    if popover is not None:
        b.set_popover(popover)
    else:
        b.set_menu_model(menu if menu is not None else menu_model(['—']))
    return tag(b, cid) if cid else b


def toggle_group(items, active=0, cid=None, *css, **kw):
    """items: (name, label or None, icon or None, tooltip)."""
    g = Adw.ToggleGroup(**kw)
    for name, text, icon, tip in items:
        t = Adw.Toggle(name=name)
        if text:
            t.set_label(text)
        if icon:
            t.set_icon_name(icon + '-symbolic')
        if tip:
            t.set_tooltip(tip)
        g.add(t)
    g.set_active(active)
    classes(g, *css)
    return tag(g, cid) if cid else g


def dropdown(items, selected=0, cid=None, *css, **kw):
    d = Gtk.DropDown.new_from_strings(items)
    d.set_selected(selected)
    for k, v in kw.items():
        d.set_property(k.replace('_', '-'), v)
    classes(d, *css)
    return tag(d, cid) if cid else d


def swatch(color, cid=None, big=False, tip=None):
    b = Gtk.Button(tooltip_text=tip, valign=Gtk.Align.CENTER)
    inner = Gtk.Box()
    classes(inner, 'swatch-big' if big else 'swatch')
    prov = Gtk.CssProvider()
    prov.load_from_string(f'box {{ background: {color}; }}')
    inner.get_style_context().add_provider(prov, Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION + 1)
    b.set_child(inner)
    classes(b, 'flat')
    return tag(b, cid) if cid else b


def color_box(color, *css, w=None, h=None):
    inner = Gtk.Box()
    classes(inner, *css)
    if w:
        inner.set_size_request(w, h or w)
    prov = Gtk.CssProvider()
    prov.load_from_string(f'box {{ background: {color}; }}')
    inner.get_style_context().add_provider(prov, Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION + 1)
    return inner


# ----------------------------------------------------------- inspector rows


def group(title=None, *rows, cid=None, description=None, suffix=None):
    g = Adw.PreferencesGroup()
    if title:
        g.set_title(title)
    if description:
        g.set_description(description)
    if suffix is not None:
        g.set_header_suffix(suffix)
    for r in rows:
        if r is not None:
            g.add(r)
    return tag(g, cid) if cid else g


def combo_row(title, items, selected=0, subtitle=None):
    r = Adw.ComboRow(title=title, model=Gtk.StringList.new(items), selected=selected)
    if subtitle:
        r.set_subtitle(subtitle)
    return r


def spin_row(title, value, lo=0, hi=1000, step=1, digits=0, subtitle=None):
    r = Adw.SpinRow.new_with_range(lo, hi, step)
    r.set_title(title)
    r.set_digits(digits)
    r.set_value(value)
    if subtitle:
        r.set_subtitle(subtitle)
    return r


def switch_row(title, active=False, subtitle=None):
    r = Adw.SwitchRow(title=title, active=active)
    if subtitle:
        r.set_subtitle(subtitle)
    return r


def action_row(title, subtitle=None, suffix=None, prefix=None, arrow=False, prop=False):
    r = Adw.ActionRow(title=title)
    if subtitle:
        r.set_subtitle(subtitle)
    if prefix is not None:
        r.add_prefix(prefix)
    if suffix is not None:
        if isinstance(suffix, (list, tuple)):
            for s in suffix:
                s.set_valign(Gtk.Align.CENTER)
                r.add_suffix(s)
        else:
            suffix.set_valign(Gtk.Align.CENTER)
            r.add_suffix(suffix)
    if arrow:
        r.add_suffix(Gtk.Image(icon_name='go-next-symbolic'))
        r.set_activatable(True)
    if prop:
        r.add_css_class('property')
    return r


def expander_row(title, *rows, subtitle=None, expanded=False):
    r = Adw.ExpanderRow(title=title, expanded=expanded)
    if subtitle:
        r.set_subtitle(subtitle)
    for row in rows:
        r.add_row(row)
    return r


def button_row(title, icon=None, end_icon=None, *css):
    r = Adw.ButtonRow(title=title)
    if icon:
        r.set_start_icon_name(icon + '-symbolic')
    if end_icon:
        r.set_end_icon_name(end_icon + '-symbolic')
    return classes(r, *css)


def scale_row(title, value, lo=0, hi=100):
    s = Gtk.Scale.new_with_range(Gtk.Orientation.HORIZONTAL, lo, hi, 1)
    s.set_value(value)
    s.set_size_request(120, -1)
    s.set_draw_value(False)
    return action_row(title, suffix=[s, label(f'{value} %', 'numeric', 'dimmed')])


def page(*groups, spacing=18):
    box = vbox(*groups, spacing=spacing)
    classes(box, 'inspector-page')
    sw = Gtk.ScrolledWindow(hscrollbar_policy=Gtk.PolicyType.NEVER, vexpand=True)
    sw.set_child(box)
    return sw


def view_stack(pages, visible=None):
    """pages: (name, title, icon or None, child)."""
    st = Adw.ViewStack(vexpand=True)
    for name, title, icon, child in pages:
        if icon:
            st.add_titled_with_icon(child, name, title, icon + '-symbolic')
        else:
            st.add_titled(child, name, title)
    if visible:
        st.set_visible_child_name(visible)
    return st


def inline_switcher(stack, cid=None, *css, mode=None, **kw):
    sw = Adw.InlineViewSwitcher(stack=stack, **kw)
    if mode is not None:
        sw.set_display_mode(mode)
    classes(sw, *css)
    return tag(sw, cid) if cid else sw
