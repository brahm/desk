# SPDX-FileCopyrightText: 2026 Desk contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The document, the sheet and the slide that the mockups show.

These stand for the content views Desk already draws itself (desk-render):
they are not part of the interface inventory, except for what is drawn over
the content, like rulers, selection handles and reference tabs.
"""
import math

import gi

gi.require_version('Gtk', '4.0')
gi.require_version('PangoCairo', '1.0')
from gi.repository import Gtk, Pango, PangoCairo  # noqa: E402

ACCENT = (0.208, 0.518, 0.894)          # Adwaita blue, the default accent
SERIF = 'TeX Gyre Termes'
SANS = 'Adwaita Sans'

LOREM = (
    'A receita cresceu doze por cento em relação ao ano anterior, puxada pelas vendas diretas e '
    'pela abertura de duas lojas. O custo de logística caiu depois da troca de transportadora, e a '
    'margem voltou ao patamar de 2023. ',
    'Para o próximo ciclo, a prioridade é consolidar os canais digitais sem perder o atendimento de '
    'balcão, que continua sendo a origem de metade dos pedidos recorrentes. As metas por região '
    'estão na tabela da página seguinte. ',
    'O investimento em treinamento dobrou, e a rotatividade da equipe foi a menor desde a fundação. '
    'Os indicadores de satisfação, medidos a cada trimestre, ficaram acima de nove pontos em todas '
    'as unidades. ',
)


def _text(cr, x, y, text, font, size, width=None, color=(0.1, 0.1, 0.1), markup=False,
          align=None, spacing=None):
    layout = PangoCairo.create_layout(cr)
    fd = Pango.FontDescription(f'{font} {size}')
    layout.set_font_description(fd)
    if width:
        layout.set_width(int(width * Pango.SCALE))
        layout.set_wrap(Pango.WrapMode.WORD)
    if align:
        layout.set_alignment(align)
    if spacing:
        layout.set_line_spacing(spacing)
    if markup:
        layout.set_markup(text, -1)
    else:
        layout.set_text(text, -1)
    cr.set_source_rgb(*color)
    cr.move_to(x, y)
    PangoCairo.show_layout(cr, layout)
    _, ext = layout.get_pixel_extents()
    return ext.height


def _rr(cr, x, y, w, h, r):
    cr.new_sub_path()
    cr.arc(x + w - r, y + r, r, -math.pi / 2, 0)
    cr.arc(x + w - r, y + h - r, r, 0, math.pi / 2)
    cr.arc(x + r, y + h - r, r, math.pi / 2, math.pi)
    cr.arc(x + r, y + r, r, math.pi, 3 * math.pi / 2)
    cr.close_path()


def _shadowed_sheet(cr, x, y, w, h, fill=(1, 1, 1)):
    for i in range(6, 0, -1):
        cr.set_source_rgba(0, 0, 0, 0.022)
        _rr(cr, x - i, y - i + 2, w + 2 * i, h + 2 * i, 3 + i)
        cr.fill()
    cr.set_source_rgb(*fill)
    cr.rectangle(x, y, w, h)
    cr.fill()


def _handles(cr, x, y, w, h):
    cr.set_source_rgb(*ACCENT)
    cr.set_line_width(1.2)
    cr.rectangle(x, y, w, h)
    cr.stroke()
    for hx, hy in ((x, y), (x + w / 2, y), (x + w, y), (x, y + h / 2), (x + w, y + h / 2),
                   (x, y + h), (x + w / 2, y + h), (x + w, y + h)):
        cr.set_source_rgb(1, 1, 1)
        cr.rectangle(hx - 3.5, hy - 3.5, 7, 7)
        cr.fill_preserve()
        cr.set_source_rgb(*ACCENT)
        cr.stroke()


def _photo(cr, x, y, w, h):
    """A placeholder picture: sky, hills and sun."""
    cr.save()
    cr.rectangle(x, y, w, h)
    cr.clip()
    cr.set_source_rgb(0.72, 0.84, 0.95)
    cr.paint()
    cr.set_source_rgb(0.98, 0.88, 0.55)
    cr.arc(x + w * 0.74, y + h * 0.3, h * 0.12, 0, 2 * math.pi)
    cr.fill()
    cr.set_source_rgb(0.45, 0.66, 0.5)
    cr.move_to(x, y + h * 0.75)
    cr.curve_to(x + w * 0.25, y + h * 0.4, x + w * 0.45, y + h * 0.9, x + w * 0.62, y + h * 0.62)
    cr.curve_to(x + w * 0.78, y + h * 0.45, x + w * 0.9, y + h * 0.7, x + w, y + h * 0.58)
    cr.line_to(x + w, y + h)
    cr.line_to(x, y + h)
    cr.close_path()
    cr.fill()
    cr.set_source_rgb(0.33, 0.55, 0.42)
    cr.move_to(x, y + h * 0.9)
    cr.curve_to(x + w * 0.3, y + h * 0.68, x + w * 0.6, y + h * 1.0, x + w, y + h * 0.8)
    cr.line_to(x + w, y + h)
    cr.line_to(x, y + h)
    cr.close_path()
    cr.fill()
    cr.restore()


# ------------------------------------------------------------------ Writer


def writer_canvas(selection='text', page_w=560, dark_bg=False, comments=False):
    """The document view: one page of a report, with a selection."""
    area = Gtk.DrawingArea(hexpand=True, vexpand=True)
    area.add_css_class('canvas')
    info = {}

    def draw(_a, cr, w, h):
        pw = min(page_w, w - 60)
        ph = max(pw * 1.414, h)        # a narrow window shows the top of a taller page
        px = (w - pw) / 2
        py = 22
        _shadowed_sheet(cr, px, py, pw, ph)
        m = pw * 0.12
        tx, tw = px + m, pw - 2 * m
        y = py + m * 0.55
        _text(cr, tx, y, 'Relatório anual 2026', SANS, 7.5, color=(0.5, 0.5, 0.5))
        _text(cr, tx, y, '6', SANS, 7.5, width=tw, color=(0.5, 0.5, 0.5), align=Pango.Alignment.RIGHT)
        y = py + m
        y += _text(cr, tx, y, 'Resultados do ano', SANS + ' Bold', 19) + 4
        y += _text(cr, tx, y, 'Vendas, pessoas e metas para 2027', SANS, 10.5, color=(0.4, 0.4, 0.4)) + 14
        # first paragraph, with a selected sentence
        para = LOREM[0]
        if selection == 'text':
            a = para.index('O custo')
            b = para.index('transportadora') + len('transportadora')
            mk = (para[:a] + '<span background="#3584e4" background_alpha="32%">' + para[a:b]
                  + '</span>' + para[b:])
            y += _text(cr, tx, y, mk, SERIF, 10.5, width=tw, markup=True, spacing=1.25) + 9
        else:
            y += _text(cr, tx, y, para, SERIF, 10.5, width=tw, spacing=1.25) + 9
        # picture floating at the right, text beside it
        iw, ih = tw * 0.42, tw * 0.3
        ix = tx + tw - iw
        if selection == 'shape':
            # a rounded rectangle with text inside, in place of the picture
            cr.set_source_rgb(0.11, 0.44, 0.85)
            _rr(cr, ix, y + 3, iw, ih, 12)
            cr.fill()
            _text(cr, ix, y + 3 + ih / 2 - 17, 'Meta de 2027', SANS + ' Bold', 12, width=iw,
                  color=(1, 1, 1), align=Pango.Alignment.CENTER)
            _text(cr, ix, y + 3 + ih / 2 + 3, '4 mil pedidos por trimestre', SANS, 8.5, width=iw,
                  color=(1, 1, 1), align=Pango.Alignment.CENTER)
            _handles(cr, ix, y + 3, iw, ih)
        else:
            _photo(cr, ix, y + 3, iw, ih)
        info['image'] = (ix, y + 3, iw, ih)
        if selection == 'image':
            _handles(cr, ix, y + 3, iw, ih)
        hy = _text(cr, tx, y, LOREM[1], SERIF, 10.5, width=tw - iw - 14, spacing=1.25)
        y += max(hy, ih + 6) + 9
        y += _text(cr, tx, y, 'Pessoas', SANS + ' Bold', 13) + 5
        y += _text(cr, tx, y, LOREM[2], SERIF, 10.5, width=tw, spacing=1.25) + 9
        # a small table
        cols, rows = 4, 4
        cw, rh = tw / cols, 19
        head = ('Região', 'Meta', 'Realizado', 'Variação')
        data = (('Sul', '1.200', '1.340', '+11,7 %'), ('Sudeste', '2.800', '2.950', '+5,4 %'),
                ('Nordeste', '950', '1.010', '+6,3 %'))
        cr.set_source_rgb(0.25, 0.33, 0.45)
        cr.rectangle(tx, y, tw, rh)
        cr.fill()
        for ci, t in enumerate(head):
            _text(cr, tx + ci * cw + 5, y + 3.5, t, SANS + ' Bold', 8.5, color=(1, 1, 1))
        for ri, row in enumerate(data):
            ry = y + (ri + 1) * rh
            if ri % 2:
                cr.set_source_rgb(0.95, 0.96, 0.98)
                cr.rectangle(tx, ry, tw, rh)
                cr.fill()
            for ci, t in enumerate(row):
                _text(cr, tx + ci * cw + 5, ry + 3.5, t, SANS, 8.5)
        cr.set_source_rgb(0.75, 0.77, 0.8)
        cr.set_line_width(0.8)
        for ri in range(rows + 1):
            cr.move_to(tx, y + ri * rh)
            cr.line_to(tx + tw, y + ri * rh)
        for ci in range(cols + 1):
            cr.move_to(tx + ci * cw, y)
            cr.line_to(tx + ci * cw, y + rows * rh)
        cr.stroke()
        y += rows * rh + 12
        if y < py + ph - m:
            _text(cr, tx, y, LOREM[0] + LOREM[1], SERIF, 10.5, width=tw, spacing=1.25)
        if comments:
            cx = px + pw + 10
            cr.set_source_rgb(1.0, 0.95, 0.67)
            _rr(cr, cx, py + m + 70, 120, 54, 6)
            cr.fill()
            _text(cr, cx + 8, py + m + 76, '<b>Ana</b>\nConferir este número com o financeiro',
                  SANS, 7.5, width=106, markup=True)
            info['comment'] = (cx, py + m + 70, 120, 54)
        info['page'] = (px, py, pw, ph)

    area.set_draw_func(draw)
    area.info = info
    return area


def ruler(orientation='h', length_cm=17, margin=0.12):
    """The ruler over the page, with margins and tab stops."""
    area = Gtk.DrawingArea()
    if orientation == 'h':
        area.set_content_height(20)
        area.set_hexpand(True)
    else:
        area.set_content_width(20)
        area.set_vexpand(True)

    def draw(_a, cr, w, h):
        cr.set_source_rgba(0.5, 0.5, 0.5, 0.10)
        cr.paint()
        if orientation != 'h':
            return
        pw = min(560, w - 60)
        px = (w - pw) / 2
        m = pw * margin
        cr.set_source_rgb(1, 1, 1)
        cr.rectangle(px + m, 0, pw - 2 * m, h)
        cr.fill()
        step = (pw - 2 * m) / length_cm
        cr.set_source_rgb(0.45, 0.45, 0.45)
        cr.set_line_width(0.8)
        for i in range(length_cm + 1):
            x = px + m + i * step
            cr.move_to(x, h)
            cr.line_to(x, h - 6)
            if i < length_cm:
                cr.move_to(x + step / 2, h)
                cr.line_to(x + step / 2, h - 3)
        cr.stroke()
        for i in range(1, length_cm, 2):
            _text(cr, px + m + i * step + 2, 1, str(i), SANS, 6.5, color=(0.4, 0.4, 0.4))
        # indent markers
        cr.set_source_rgb(*ACCENT)
        for x in (px + m, px + pw - m):
            cr.move_to(x - 4, 0)
            cr.line_to(x + 4, 0)
            cr.line_to(x, 7)
            cr.close_path()
            cr.fill()

    area.set_draw_func(draw)
    return area


def page_thumb(n, selected=False, kind='text'):
    area = Gtk.DrawingArea()
    area.set_content_width(86)
    area.set_content_height(118)

    def draw(_a, cr, w, h):
        x, y, pw, ph = 4, 3, w - 8, h - 6
        cr.set_source_rgba(0, 0, 0, 0.12)
        cr.rectangle(x + 1, y + 1.5, pw, ph)
        cr.fill()
        cr.set_source_rgb(1, 1, 1)
        cr.rectangle(x, y, pw, ph)
        cr.fill()
        if selected:
            cr.set_source_rgb(*ACCENT)
            cr.set_line_width(2.5)
            _rr(cr, x - 2.5, y - 2.5, pw + 5, ph + 5, 3)
            cr.stroke()
        cr.set_source_rgb(0.3, 0.3, 0.3)
        if kind == 'cover':
            cr.set_source_rgb(0.25, 0.33, 0.45)
            cr.rectangle(x, y, pw, ph * 0.45)
            cr.fill()
            cr.set_source_rgb(0.3, 0.3, 0.3)
            cr.rectangle(x + 9, y + ph * 0.55, pw * 0.6, 4)
            cr.fill()
            cr.set_source_rgb(0.7, 0.7, 0.7)
            cr.rectangle(x + 9, y + ph * 0.64, pw * 0.4, 2)
            cr.fill()
            return
        cr.rectangle(x + 9, y + 12, pw * 0.5, 3.5)
        cr.fill()
        cr.set_source_rgb(0.72, 0.72, 0.72)
        yy = y + 22
        i = 0
        while yy < y + ph - 10:
            if kind == 'image' and 36 < yy - y < 70:
                cr.set_source_rgb(0.72, 0.84, 0.95)
                cr.rectangle(x + pw * 0.5, y + 38, pw * 0.38, 28)
                cr.fill()
                cr.set_source_rgb(0.72, 0.72, 0.72)
                cr.rectangle(x + 9, yy, pw * 0.36, 1.6)
            elif kind == 'table' and 60 < yy - y < 90:
                cr.set_source_rgb(0.6, 0.66, 0.75)
                cr.rectangle(x + 9, yy, pw - 18, 1.6)
            else:
                cr.rectangle(x + 9, yy, (pw - 18) * (0.75 if i % 5 == 4 else 1), 1.6)
            cr.fill()
            cr.set_source_rgb(0.72, 0.72, 0.72)
            yy += 4.6
            i += 1

    area.set_draw_func(draw)
    return area


# ------------------------------------------------------------------ Tables


def tables_canvas(selection='cell', editing=False):
    """A sheet with two tables and a chart; one table is selected."""
    area = Gtk.DrawingArea(hexpand=True, vexpand=True)
    area.add_css_class('view')
    info = {}

    def draw(_a, cr, w, h):
        cr.set_source_rgb(1, 1, 1)
        cr.paint()
        ox, oy = 46, 56
        cols = ('', 'Jan', 'Fev', 'Mar', 'Abr', 'Total')
        rows = (('Aluguel', '2.400', '2.400', '2.400', '2.400', '9.600'),
                ('Energia', '310', '295', '342', '318', '1.265'),
                ('Internet', '129', '129', '129', '129', '516'),
                ('Material', '860', '412', '590', '733', '2.595'),
                ('Transporte', '540', '575', '498', '610', '2.223'),
                ('Marketing', '1.200', '900', '1.450', '1.100', '4.650'))
        cw = (104, 72, 72, 72, 72, 82)
        rh = 24
        tw = sum(cw)
        nrows = len(rows) + 2
        # reference tabs (letters and numbers) of the selected table
        cr.set_source_rgb(0.90, 0.91, 0.93)
        cr.rectangle(ox, oy - 20, tw, 18)
        cr.rectangle(ox - 26, oy, 24, nrows * rh)
        cr.fill()
        cr.set_source_rgba(*ACCENT, 0.28)
        cr.rectangle(ox + sum(cw[:5]), oy - 20, cw[5], 18)
        cr.rectangle(ox - 26, oy + rh, 24, 6 * rh)
        cr.fill()
        x = ox
        for i, c in enumerate(cw):
            _text(cr, x + c / 2 - 4, oy - 18.5, 'ABCDEF'[i], SANS, 8, color=(0.3, 0.3, 0.3))
            x += c
        for i in range(nrows):
            _text(cr, ox - 18, oy + i * rh + 5.5, str(i + 1), SANS, 8, color=(0.3, 0.3, 0.3))
        info['tabs'] = (ox - 26, oy - 20, tw + 26, 18)
        # table name
        _text(cr, ox, oy - 44, 'Despesas fixas', SANS + ' Bold', 11)
        # header row
        cr.set_source_rgb(0.25, 0.33, 0.45)
        cr.rectangle(ox, oy, tw, rh)
        cr.fill()
        x = ox
        for i, c in enumerate(cols):
            _text(cr, x + 8, oy + 5, c, SANS + ' Bold', 9, color=(1, 1, 1), width=cw[i] - 16,
                  align=Pango.Alignment.RIGHT if i else Pango.Alignment.LEFT)
            x += cw[i]
        # body
        for ri, row in enumerate(rows):
            ry = oy + (ri + 1) * rh
            cr.set_source_rgb(0.93, 0.94, 0.96)
            cr.rectangle(ox, ry, cw[0], rh)
            cr.fill()
            if ri % 2:
                cr.set_source_rgb(0.975, 0.98, 0.985)
                cr.rectangle(ox + cw[0], ry, tw - cw[0], rh)
                cr.fill()
            x = ox
            for i, t in enumerate(row):
                _text(cr, x + 8, ry + 5, t, SANS + (' Bold' if i == 0 else ''), 9, width=cw[i] - 16,
                      align=Pango.Alignment.RIGHT if i else Pango.Alignment.LEFT)
                x += cw[i]
        # footer row
        fy = oy + (len(rows) + 1) * rh
        cr.set_source_rgb(0.86, 0.88, 0.92)
        cr.rectangle(ox, fy, tw, rh)
        cr.fill()
        x = ox
        for i, t in enumerate(('Total', '5.439', '4.711', '5.409', '5.290', '20.849')):
            _text(cr, x + 8, fy + 5, t, SANS + ' Bold', 9, width=cw[i] - 16,
                  align=Pango.Alignment.RIGHT if i else Pango.Alignment.LEFT)
            x += cw[i]
        cr.set_source_rgb(0.78, 0.8, 0.83)
        cr.set_line_width(0.8)
        for i in range(nrows + 1):
            cr.move_to(ox, oy + i * rh)
            cr.line_to(ox + tw, oy + i * rh)
        x = ox
        for c in cw + (0,):
            cr.move_to(x, oy)
            cr.line_to(x, oy + nrows * rh)
            x += c
        cr.stroke()
        # the selection: F2:F7, with F2 as the active cell
        sx, sy = ox + sum(cw[:5]), oy + rh
        if selection != 'chart':
            cr.set_source_rgba(*ACCENT, 0.13)
            cr.rectangle(sx, sy + rh, cw[5], 5 * rh)
            cr.fill()
            cr.set_source_rgb(*ACCENT)
            cr.set_line_width(2)
            cr.rectangle(sx, sy, cw[5], 6 * rh)
            cr.stroke()
            cr.arc(sx + cw[5], sy + 6 * rh, 3.5, 0, 2 * math.pi)
            cr.fill()
        info['cell'] = (sx, sy, cw[5], 6 * rh)
        if editing:
            cr.set_source_rgb(1, 1, 1)
            _rr(cr, sx - 150, sy - 6, 236, rh + 12, 8)
            cr.fill_preserve()
            cr.set_source_rgba(0, 0, 0, 0.25)
            cr.set_line_width(1)
            cr.stroke()
            _text(cr, sx - 140, sy + 4, '= <span foreground="#1c71d8">SOMA</span>'
                  '(<span background="#f9f06b">B2:E2</span>) × <span background="#8ff0a4">Reajuste</span>',
                  SANS, 9.5, markup=True)
            info['editor'] = (sx - 150, sy - 6, 236, rh + 12)
        # chart
        gx, gy, gw, gh = ox + tw + 46, oy - 44, w - (ox + tw + 46) - 30, 236
        if gw > 160:
            _text(cr, gx, gy, 'Despesas por mês', SANS + ' Bold', 11)
            base = gy + gh - 22
            cr.set_source_rgb(0.8, 0.8, 0.8)
            cr.set_line_width(0.8)
            for i in range(5):
                yy = base - i * (gh - 60) / 4
                cr.move_to(gx + 26, yy)
                cr.line_to(gx + gw, yy)
                _text(cr, gx, yy - 6, str(i * 2) + ' mil', SANS, 7, color=(0.45, 0.45, 0.45))
            cr.stroke()
            vals = (5.439, 4.711, 5.409, 5.290)
            bw = (gw - 60) / len(vals)
            for i, v in enumerate(vals):
                bh = v / 8 * (gh - 60)
                cr.set_source_rgb(*(ACCENT if i != 2 else (0.9, 0.38, 0.0)))
                _rr(cr, gx + 40 + i * bw, base - bh, bw * 0.6, bh, 2)
                cr.fill()
                _text(cr, gx + 40 + i * bw + bw * 0.18, base + 4, ('Jan', 'Fev', 'Mar', 'Abr')[i], SANS, 7.5,
                      color=(0.35, 0.35, 0.35))
            info['chart'] = (gx - 6, gy - 6, gw + 12, gh + 12)
            if selection == 'chart':
                _handles(cr, gx - 6, gy - 6, gw + 12, gh + 12)
        # second table
        ty = oy + nrows * rh + 62
        _text(cr, ox, ty - 24, 'Taxas', SANS + ' Bold', 11)
        t2 = (('Reajuste', '4,5 %'), ('Reserva', '10 %'), ('Imposto', '6 %'))
        for ri, (k, v) in enumerate(t2):
            ry = ty + ri * rh
            cr.set_source_rgb(0.93, 0.94, 0.96)
            cr.rectangle(ox, ry, 104, rh)
            cr.fill()
            _text(cr, ox + 8, ry + 5, k, SANS + ' Bold', 9)
            _text(cr, ox + 112, ry + 5, v, SANS, 9, width=56, align=Pango.Alignment.RIGHT)
        cr.set_source_rgb(0.78, 0.8, 0.83)
        for i in range(4):
            cr.move_to(ox, ty + i * rh)
            cr.line_to(ox + 176, ty + i * rh)
        for xx in (ox, ox + 104, ox + 176):
            cr.move_to(xx, ty)
            cr.line_to(xx, ty + 3 * rh)
        cr.stroke()

    area.set_draw_func(draw)
    area.info = info
    return area


def sheet_thumb(kind='table'):
    area = Gtk.DrawingArea()
    area.set_content_width(16)
    area.set_content_height(16)
    return area


# ------------------------------------------------------------------ Slides

SLIDES = (
    ('Plano de lançamento', 'title'),
    ('Onde estamos', 'bullets'),
    ('Público e canais', 'image'),
    ('Calendário', 'table'),
    ('Orçamento', 'chart'),
    ('Riscos', 'bullets'),
    ('Próximos passos', 'bullets'),
)


def _slide(cr, x, y, w, h, title, kind, big=False, selection=None, info=None):
    s = w / 640
    cr.save()
    cr.rectangle(x, y, w, h)
    cr.clip()
    cr.set_source_rgb(0.99, 0.99, 0.985)
    cr.paint()
    cr.set_source_rgb(0.14, 0.22, 0.36)
    cr.rectangle(x, y, w, 9 * s)
    cr.fill()
    if kind == 'title':
        cr.set_source_rgb(0.14, 0.22, 0.36)
        cr.rectangle(x, y, w, h * 0.55)
        cr.fill()
        _text(cr, x + 44 * s, y + h * 0.25, title, SANS + ' Bold', 30 * s, color=(1, 1, 1))
        _text(cr, x + 44 * s, y + h * 0.63, 'Equipe de produto · março de 2027', SANS, 15 * s,
              color=(0.3, 0.3, 0.3))
        cr.restore()
        return
    _text(cr, x + 40 * s, y + 34 * s, title, SANS + ' Bold', 26 * s, color=(0.14, 0.22, 0.36))
    by = y + 104 * s
    bw = w - 80 * s
    if kind == 'image':
        iw, ih = w * 0.4, h * 0.52
        ix, iy = x + w - iw - 40 * s, y + 108 * s
        _photo(cr, ix, iy, iw, ih)
        if info is not None:
            info['image'] = (ix, iy, iw, ih)
        if selection == 'image':
            _handles(cr, ix, iy, iw, ih)
        bw = w - iw - 110 * s
    if kind in ('bullets', 'image'):
        items = ('Três capitais na primeira onda', 'Parceiros locais para a entrega',
                 'Campanha em rádio e redes', 'Meta: 4 mil pedidos no trimestre')
        for i, t in enumerate(items[:3] if kind == 'image' else items):
            cr.set_source_rgb(0.9, 0.38, 0.0)
            cr.arc(x + 48 * s, by + 14 * s, 4 * s, 0, 2 * math.pi)
            cr.fill()
            hh = _text(cr, x + 64 * s, by, t, SANS, 17 * s, width=bw - 24 * s, color=(0.15, 0.15, 0.15))
            if selection == 'text' and i == 1 and big:
                cr.set_source_rgba(*ACCENT, 0.28)
                cr.rectangle(x + 64 * s, by, bw - 60 * s, hh)
                cr.fill()
            by += hh + 12 * s
    elif kind == 'table':
        for r in range(4):
            cr.set_source_rgb(*((0.14, 0.22, 0.36) if r == 0 else (0.94, 0.95, 0.97) if r % 2 else (1, 1, 1)))
            cr.rectangle(x + 40 * s, by + r * 44 * s, w - 80 * s, 44 * s)
            cr.fill()
    elif kind == 'chart':
        base = y + h - 50 * s
        for i, v in enumerate((0.45, 0.7, 0.55, 0.9, 0.65)):
            cr.set_source_rgb(*(ACCENT if i != 3 else (0.9, 0.38, 0.0)))
            cr.rectangle(x + (80 + i * 100) * s, base - v * 180 * s, 60 * s, v * 180 * s)
            cr.fill()
    cr.restore()


def slide_thumb(n, selected=False, w=132):
    title, kind = SLIDES[(n - 1) % len(SLIDES)]
    area = Gtk.DrawingArea()
    h = round(w * 9 / 16)
    area.set_content_width(w + 8)
    area.set_content_height(h + 8)

    def draw(_a, cr, aw, ah):
        x, y = 4, 4
        cr.set_source_rgba(0, 0, 0, 0.15)
        cr.rectangle(x + 0.5, y + 1.5, w, h)
        cr.fill()
        _slide(cr, x, y, w, h, title, kind)
        if selected:
            cr.set_source_rgb(*ACCENT)
            cr.set_line_width(2.5)
            _rr(cr, x - 2.5, y - 2.5, w + 5, h + 5, 3)
            cr.stroke()

    area.set_draw_func(draw)
    return area


def slides_canvas(n=3, selection='image', dark=True):
    title, kind = SLIDES[n - 1]
    area = Gtk.DrawingArea(hexpand=True, vexpand=True)
    area.add_css_class('canvas')
    info = {}

    def draw(_a, cr, w, h):
        sw = min(w - 56, (h - 44) * 16 / 9)
        sh = sw * 9 / 16
        x, y = (w - sw) / 2, (h - sh) / 2
        _shadowed_sheet(cr, x, y, sw, sh)
        _slide(cr, x, y, sw, sh, title, kind, big=True, selection=selection, info=info)
        info['slide'] = (x, y, sw, sh)

    area.set_draw_func(draw)
    area.info = info
    return area


def light_table(count=12, selected=3, w=176):
    flow = Gtk.FlowBox(selection_mode=Gtk.SelectionMode.NONE, max_children_per_line=6,
                       min_children_per_line=3, column_spacing=10, row_spacing=10,
                       margin_top=18, margin_bottom=18, margin_start=18, margin_end=18,
                       valign=Gtk.Align.START, homogeneous=True)
    for i in range(1, count + 1):
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=2)
        box.append(slide_thumb(i, selected=(i == selected), w=w))
        lab = Gtk.Label(label=str(i))
        lab.add_css_class('thumb-label')
        lab.add_css_class('dimmed')
        box.append(lab)
        flow.append(box)
    return flow
