# SPDX-FileCopyrightText: 2026 Desk contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Desk Tables in the three versions."""
import gi

gi.require_version('Gtk', '4.0')
gi.require_version('Adw', '1')
from gi.repository import Adw, Gtk  # noqa: E402

import canvas
import parts
import shell
from kit import (Scene, action_row, button_row, classes, color_box, combo_row, dropdown, expand, group,
                 hbox, icon_button, label, menu_button, menu_model, page, spacer, spin_row, split_button,
                 switch_row, tag, text_button, toggle_group, vbox)

APP = 'tables'
SHEETS = ['Resumo', 'Despesas', 'Receitas', 'Gráficos']
OBJECTS = [('Despesas fixas', 'desk-table'), ('Despesas por mês', 'desk-chart'), ('Taxas', 'desk-table')]
FUNCTIONS = menu_model(['Soma', 'Média', 'Mínimo', 'Máximo', 'Contagem', 'Produto'],
                       ['Mais funções…', 'Editor de fórmulas'])
CALCS = [('SOMA', '20.849'), ('MÉDIA', '3.474,83'), ('MÍN', '516'), ('MÁX', '9.600'), ('CONT.', '6')]
ADD_SHEET = menu_model(['Planilha em branco'], ['Duplicar planilha'])


def sheet_view(selection='range', editing=False, overlays=()):
    cv = tag(canvas.tables_canvas(selection=selection, editing=editing), 'doc.planilha')
    if overlays:
        ov = Gtk.Overlay(child=cv)
        for w in overlays:
            ov.add_overlay(w)
        return ov
    return cv


def formula_bar(editing=False):
    """The cell reference, the function button and the content of the cell."""
    ref = menu_button(text='F2', tip='Célula ativa', cid='formula.celula',
                      menu=menu_model(['Ir para a célula…'], [{'title': 'Tabelas'}, 'Despesas fixas', 'Taxas']))
    classes(ref, 'flat', 'numeric')
    fx = menu_button(icon='desk-function', tip='Função', cid='formula.funcao', menu=FUNCTIONS)
    entry = Gtk.Entry(text='=SOMA(B2:E2)', hexpand=True)
    classes(entry, 'formula-entry')
    kids = [ref, fx, tag(entry, 'formula.campo')]
    if editing:
        kids += [icon_button('process-stop', 'Cancelar', 'formula.cancelar'),
                 icon_button('object-select', 'Aceitar', 'formula.aceitar')]
    bar = hbox(*kids, spacing=4)
    classes(bar, 'toolbar', 'formatbar-slim')
    return tag(bar, 'formula.barra')


def format_bar_cell(prefix='fmt', slim=False):
    """The format bar when cells are selected.

    Full (version A): the table mode of the Numbers '09 bar, in its order:
    font, typeface, size, colour; alignment with automatic, vertical alignment,
    wrap; the number formats and the decimals; borders; fill. Slim (version B):
    what is used all the time, the rest under More.
    """
    bar = hbox(spacing=4)
    classes(bar, 'toolbar', 'formatbar-slim' if slim else 'formatbar')
    bar.append(menu_button(text='Adwaita Sans', tip='Fonte', cid=f'{prefix}.fonte', popover=Gtk.Popover(),
                           max_chars=12))
    if not slim:
        bar.append(dropdown(['Regular', 'Negrito'], 1, f'{prefix}.tipo', tooltip_text='Tipo'))
    bar.append(parts.size_spin(9, f'{prefix}.tamanho'))
    if slim:
        bar.append(spacer())
        for b in parts.biu(prefix):
            bar.append(b)
    bar.append(menu_button(icon='desk-text-color', tip='Cor do texto', cid=f'{prefix}.cor-texto',
                           popover=Gtk.Popover()))
    bar.append(spacer())
    aligns = [('l', None, 'format-justify-left', 'Alinhar à esquerda'),
              ('c', None, 'format-justify-center', 'Centralizar'),
              ('r', None, 'format-justify-right', 'Alinhar à direita'),
              ('j', None, 'format-justify-fill', 'Justificar')]
    if not slim:
        aligns.append(('a', None, 'desk-align-auto', 'Alinhar automaticamente'))
    bar.append(toggle_group(aligns, 2, f'{prefix}.alinhamento', 'flat'))
    if not slim:
        bar.append(menu_button(icon='desk-valign-middle', tip='Alinhamento vertical', cid=f'{prefix}.alinhamento-v',
                               menu=menu_model(['Em cima', 'No meio', 'Embaixo'])))
        bar.append(icon_button('view-wrapped', 'Ajustar texto na célula', f'{prefix}.ajustar', toggle=True,
                               active=True))
    bar.append(spacer())
    more_formats = menu_model(['Data e hora', 'Duração', 'Fração', 'Sistema numérico', 'Científico', 'Texto'],
                              ['Controle deslizante', 'Passo a passo', 'Menu pop-up'], ['Personalizado…'])
    if slim:
        bar.append(menu_button(icon='desk-number-format', text='Número', tip='Formato de dados',
                               cid=f'{prefix}.formato',
                               menu=menu_model(['Automático', 'Número', 'Moeda', 'Porcentagem', 'Caixa de seleção'],
                                               [('Mais formatos', [['Data e hora', 'Duração', 'Fração',
                                                                    'Científico', 'Texto'],
                                                                   ['Controle deslizante', 'Passo a passo',
                                                                    'Menu pop-up']])],
                                               ['Personalizado…'])))
    else:
        for t, tip, c in (('1,0', 'Número', 'numero'), ('R$', 'Moeda', 'moeda'), ('%', 'Porcentagem', 'porcento')):
            bar.append(text_button(t, tip, f'{prefix}.{c}', 'flat', 'numeric'))
        bar.append(icon_button('checkbox-checked', 'Caixa de seleção', f'{prefix}.caixa'))
        bar.append(menu_button(icon='view-more-horizontal', tip='Mais formatos', cid=f'{prefix}.mais-formatos',
                               menu=more_formats))
    bar.append(icon_button('value-decrease', 'Menos casas decimais', f'{prefix}.menos-casas'))
    bar.append(icon_button('value-increase', 'Mais casas decimais', f'{prefix}.mais-casas'))
    bar.append(spacer())
    borders = menu_button(icon='desk-borders', tip='Bordas da célula', cid=f'{prefix}.bordas',
                          menu=menu_model(['Todas', 'Externas', 'Internas', 'Nenhuma'],
                                          ['Estilo, espessura e cor…']))
    fill = menu_button(icon='desk-fill', tip='Preenchimento da célula', cid=f'{prefix}.preenchimento',
                       popover=Gtk.Popover())
    for w in ((borders, fill) if not slim else (fill, borders)):
        bar.append(w)
    bar.append(expand())
    if slim:
        bar.append(menu_button(icon='view-more-horizontal', tip='Mais opções de formatação', cid=f'{prefix}.mais',
                               menu=menu_model(['Alinhar automaticamente', ('Alinhamento vertical',
                                                                            [['Em cima', 'No meio', 'Embaixo']]),
                                                'Ajustar texto na célula', 'Mesclar células'],
                                               ['Copiar estilo', 'Colar estilo'],
                                               ['Mostrar no Inspetor'])))
    else:
        bar.append(menu_button(text='Formato', tip='Menu Formato', cid=f'{prefix}.menu-formatar',
                               menu=menu_model(['Fonte', 'Texto', 'Tabela', 'Gráfico'],
                                               ['Cabeçalhos e rodapé', 'Mostrar o nome da tabela'],
                                               ['Copiar estilo', 'Colar estilo'],
                                               ['Criar formato de célula personalizado…'])))
    return tag(bar, f'{prefix}.barra')


def calc_chips(cid='calc.resultados', dim=False):
    """Instant calculations for the selected cells; each can be dragged to a cell."""
    box = hbox(spacing=6)
    for name, value in CALCS:
        chip = hbox(label(name, 'tiny', 'dimmed'), label(value, 'numeric', 'tiny'), spacing=5)
        classes(chip, 'calc-chip')
        box.append(chip)
    return tag(box, cid)


# ------------------------------------------------------------------ panes


def sheets_sidebar():
    """Version B: the sheets, and the tables and charts of the current one."""
    sb = Adw.Sidebar()
    s1 = Adw.SidebarSection()
    for n in SHEETS:
        s1.append(Adw.SidebarItem(title=n, icon_name='desk-sheet-symbolic'))
    s2 = Adw.SidebarSection(title='Em “Despesas”')
    for n, ic in OBJECTS:
        s2.append(Adw.SidebarItem(title=n, icon_name=ic + '-symbolic'))
    sb.append(s1)
    sb.append(s2)
    sb.set_selected(1)
    return tag(sb, 'lat.planilhas')


def sheets_pane_a():
    """Version A: the Sheets pane of Numbers '09: a tree, the table styles and
    the results of the instant calculations."""
    lb = Gtk.ListBox(selection_mode=Gtk.SelectionMode.SINGLE)
    classes(lb, 'navigation-sidebar')
    sel = None
    for n in SHEETS:
        open_ = (n == 'Despesas')
        row = hbox(Gtk.Image(icon_name='pan-down-symbolic' if open_ else 'pan-end-symbolic'),
                   Gtk.Image(icon_name='desk-sheet-symbolic'), label(n), spacing=6)
        r = Gtk.ListBoxRow(child=row)
        lb.append(r)
        if open_:
            for on, ic in OBJECTS:
                child = Gtk.ListBoxRow(child=hbox(Gtk.Image(icon_name=ic + '-symbolic'), label(on, 'tiny'),
                                                  spacing=6, margin_start=28))
                lb.append(child)
                if on == 'Despesas fixas':
                    sel = child
    lb.select_row(sel)
    tree = Gtk.ScrolledWindow(hscrollbar_policy=Gtk.PolicyType.NEVER, vexpand=True, child=lb)
    styles = Gtk.ListBox(selection_mode=Gtk.SelectionMode.SINGLE)
    classes(styles, 'navigation-sidebar')
    for i, n in enumerate(['Básico', 'Sem grade', 'Cinza', 'Cabeçalhos cinza', 'Livro-razão', 'Azul']):
        styles.append(Gtk.ListBoxRow(child=label(n, 'tiny')))
    styles.select_row(styles.get_row_at_index(5))
    calc = vbox(spacing=2, margin_start=12, margin_end=12, margin_top=6, margin_bottom=8)
    for name, value in CALCS:
        calc.append(hbox(label(name.lower(), 'tiny', 'dimmed', hexpand=True), label(value, 'tiny', 'numeric')))
    head = lambda t, extra=None: hbox(label(t, 'heading', hexpand=True), extra, margin_start=12, margin_end=6,  # noqa: E731
                                      margin_top=8, margin_bottom=2)
    pane = vbox(
        tag(vbox(head('Planilhas', icon_button('list-add', 'Nova planilha')), tree), 'lat.a.planilhas'),
        Gtk.Separator(),
        tag(vbox(head('Estilos'), styles), 'lat.a.estilos'),
        Gtk.Separator(),
        tag(calc, 'lat.a.calculos'))
    pane.set_size_request(168, -1)
    return pane


def table_page(prefix='insp.tabela'):
    chips = hbox(spacing=6, halign=Gtk.Align.CENTER)
    for col in ['#3d5373', '#1c71d8', '#26a269', '#e66100', '#813d9c', '#5e5c64']:
        b = Gtk.Button(child=color_box(f'linear-gradient(180deg, {col} 0 30%, white 30% 53%, '
                                       f'color-mix(in srgb, {col} 14%, white) 53% 76%, white 76%)', 'style-chip'))
        classes(b, 'flat', 'chip-button')
        chips.append(b)
    return page(
        tag(chips, prefix + '.estilos'),
        group('Cabeçalhos e rodapé', spin_row('Linhas de cabeçalho', 1, 0, 5), spin_row('Colunas de cabeçalho', 1, 0, 5),
              spin_row('Linhas de rodapé', 1, 0, 5), switch_row('Congelar os cabeçalhos', True),
              cid=prefix + '.cabecalhos'),
        group('Linhas e colunas', spin_row('Linhas', 8, 1, 9999), spin_row('Colunas', 6, 1, 999),
              spin_row('Altura da linha', 0.64, 0, 50, 0.01, digits=2, subtitle='cm'),
              spin_row('Largura da coluna', 1.9, 0, 50, 0.01, digits=2, subtitle='cm'),
              button_row('Ajustar ao conteúdo'), cid=prefix + '.tamanho'),
        group('Grade e fundo', switch_row('Linhas de grade', True), switch_row('Cor alternada nas linhas', True),
              cid=prefix + '.grade'),
        group('Nome', switch_row('Mostrar o nome da tabela', True), cid=prefix + '.nome'))


def cell_page(prefix='insp.celula'):
    return page(
        group('Formato de dados',
              combo_row('Formato', ['Automático', 'Número', 'Moeda', 'Porcentagem', 'Data e hora', 'Duração',
                                    'Fração', 'Científico', 'Texto', 'Caixa de seleção', 'Controle deslizante',
                                    'Personalizado…'], 1),
              spin_row('Casas decimais', 0, 0, 30), switch_row('Separador de milhar', True),
              combo_row('Negativos', ['-100', '(100)', '-100 em vermelho']), cid=prefix + '.formato'),
        group('Preenchimento e borda', action_row('Preenchimento', suffix=parts.swatch('#ffffff')),
              action_row('Borda', suffix=[dropdown(['———', '- - -', '· · ·'], 0), parts.swatch('#9a9996')]),
              cid=prefix + '.borda'),
        group('Texto na célula', switch_row('Ajustar texto na célula', True), cid=prefix + '.ajuste'),
        group('Formato condicional', button_row('Mostrar regras…'), cid=prefix + '.condicional'))


def format_bar_chart(prefix='fmt'):
    """Version B, a chart selected: its type, the data and the look."""
    bar = hbox(spacing=4)
    classes(bar, 'toolbar', 'formatbar-slim')
    bar.append(menu_button(icon='desk-chart', text='Colunas', tip='Tipo de gráfico', cid=f'{prefix}.tipo-grafico',
                           popover=parts.gallery_popover('chart')))
    bar.append(tag(classes(Gtk.Button(label='Editar dados', tooltip_text='Editar as referências de dados'), 'flat'),
                   f'{prefix}.editar-dados'))
    bar.append(spacer())
    bar.append(menu_button(icon='desk-fill', tip='Cores do gráfico', cid=f'{prefix}.preenchimento',
                           popover=Gtk.Popover()))
    bar.append(menu_button(icon='desk-stroke', tip='Traço', cid=f'{prefix}.traco', popover=parts.stroke_popover()))
    bar.append(icon_button('desk-shadow', 'Sombra', f'{prefix}.sombra', toggle=True))
    bar.append(spacer())
    bar.append(icon_button('desk-front', 'Trazer para a frente', f'{prefix}.frente'))
    bar.append(icon_button('desk-back', 'Enviar para trás', f'{prefix}.fundo'))
    bar.append(menu_button(icon='desk-arrange', tip='Organizar', cid=f'{prefix}.organizar',
                           menu=menu_model(*parts.ARRANGE_MENU)))
    bar.append(expand())
    bar.append(menu_button(icon='view-more-horizontal', tip='Mais opções de formatação', cid=f'{prefix}.mais',
                           menu=menu_model(['Legenda', 'Título', 'Barras de erro', 'Linha de tendência'],
                                           ['Copiar estilo', 'Colar estilo'], ['Mostrar no Inspetor'])))
    return tag(bar, f'{prefix}.barra')


def chart_pages():
    chart = page(
        parts.style_chips('insp.grafico.estilos'),
        group('Gráfico', combo_row('Tipo', ['Colunas', 'Colunas empilhadas', 'Barras', 'Linhas', 'Área', 'Pizza',
                                            'Dispersão', 'Misto', 'Dois eixos'], 0),
              switch_row('Título', True), switch_row('Legenda'), switch_row('Texto oculto de tabelas'),
              cid='insp.grafico.opcoes'),
        group('Colunas', spin_row('Espaço entre colunas', 40, 0, 500, 5, subtitle='%'),
              spin_row('Espaço entre grupos', 100, 0, 500, 5, subtitle='%'), cid='insp.grafico.espacos'),
        group('Cena 3D', switch_row('Gráfico em 3D'), cid='insp.grafico.3d'))
    axis = page(
        tag(toggle_group([('y', 'Valor (Y)', None, None), ('x', 'Categoria (X)', None, None)], 0, None,
                         homogeneous=True), 'insp.eixo.qual'),
        group('Escala', combo_row('Escala', ['Linear', 'Logarítmica']),
              Adw.EntryRow(title='Mínimo', text='Automático'), Adw.EntryRow(title='Máximo', text='Automático'),
              spin_row('Passos', 4, 1, 50), cid='insp.eixo.escala'),
        group('Rótulos e linhas', switch_row('Rótulos de valor', True), switch_row('Linhas de grade', True),
              switch_row('Marcas de escala'), combo_row('Formato', ['Igual aos dados', 'Número', 'Moeda',
                                                                     'Porcentagem']), cid='insp.eixo.rotulos'))
    series = page(
        group('Dados', Adw.EntryRow(title='Valores', text='Despesas fixas :: B8:E8'),
              Adw.EntryRow(title='Rótulos', text='Despesas fixas :: B1:E1'),
              combo_row('Rótulos de valor', ['Nenhum', 'Em cima', 'No meio', 'Embaixo']), cid='insp.series.dados'),
        group('Linha de tendência', combo_row('Tipo', ['Nenhuma', 'Linear', 'Logarítmica', 'Polinomial',
                                                        'Potência', 'Exponencial', 'Média móvel']),
              cid='insp.series.tendencia'),
        group('Barras de erro', combo_row('Tipo', ['Nenhuma', 'Positiva e negativa', 'Só positiva',
                                                    'Só negativa']), cid='insp.series.erro'))
    return [('grafico', 'Gráfico', None, chart), ('eixo', 'Eixo', None, axis), ('series', 'Séries', None, series),
            ('organizar', 'Organizar', None, parts.arrange_page(writer=False))], 'grafico'


def format_pages():
    return [('tabela', 'Tabela', None, table_page()), ('celula', 'Célula', None, cell_page()),
            ('texto', 'Texto', None, parts.text_style_page('insp.texto')),
            ('organizar', 'Organizar', None, parts.arrange_page(writer=False))], 'tabela'


def reorganize_pages():
    sort = page(
        group('Ordenar', combo_row('Ordenar', ['A tabela inteira', 'As linhas selecionadas']),
              action_row('Total', suffix=toggle_group([('a', 'Crescente', None, None),
                                                       ('d', 'Decrescente', None, None)], 1)),
              button_row('Adicionar coluna', 'list-add'), cid='insp.reorg.ordenar'),
        classes(Gtk.Button(label='Ordenar agora', halign=Gtk.Align.CENTER), 'pill'))
    filt = page(group('Filtrar', switch_row('Mostrar as linhas que correspondem', True),
                      combo_row('Corresponder', ['A todas as regras', 'A qualquer regra']),
                      action_row('Total', subtitle='é maior que 1.000'),
                      button_row('Adicionar regra', 'list-add'), cid='insp.reorg.filtrar'))
    cat = page(group('Categorias', combo_row('Categorizar por', ['Nenhuma', 'Coluna A', 'Total']),
                     cid='insp.reorg.categorias'))
    return [('ordenar', 'Ordem', None, sort), ('filtrar', 'Filtro', None, filt),
            ('categorias', 'Categorias', None, cat)], 'ordenar'


def document_pages():
    sheet = page(
        group('Planilha', Adw.EntryRow(title='Nome', text='Despesas'), cid='insp.planilha.nome'),
        group('Impressão', action_row('Orientação', suffix=toggle_group([('r', 'Retrato', None, None),
                                                                         ('p', 'Paisagem', None, None)], 1)),
              combo_row('Papel', ['A4', 'Carta', 'Ofício']),
              spin_row('Escala do conteúdo', 100, 10, 400, 5, subtitle='%'),
              spin_row('Numeração começa em', 1, 1, 9999), cid='insp.planilha.impressao'),
        group('Margens', spin_row('Superior', 1.5, 0, 20, 0.1, digits=2, subtitle='cm'),
              spin_row('Inferior', 1.5, 0, 20, 0.1, digits=2, subtitle='cm'), cid='insp.planilha.margens'))
    return [('planilha', 'Planilha', None, sheet), ('info', 'Info', None, parts.info_page())], 'planilha'


LEVELS = [shell.FORMAT, shell.DATA, shell.DOCUMENT]


def inspector_b(level=shell.FORMAT, context='table'):
    if level == shell.DATA:
        pages, vis = reorganize_pages()
        return shell.inspector_b(LEVELS, level, 'Despesas fixas', None, pages, vis)
    if level == 'Documento':
        pages, vis = document_pages()
        return shell.inspector_b(LEVELS, level, None, None, pages, vis)
    if context == 'chart':
        pages, vis = chart_pages()
        return shell.inspector_b(LEVELS, level, 'Gráfico', None, pages, vis)
    pages, vis = format_pages()
    return shell.inspector_b(LEVELS, level, 'Tabela', None, pages, vis)


A_ICONS = [('doc', 'x-office-document', 'Documento'), ('planilha', 'desk-sheet', 'Planilha'),
           ('tabela', 'desk-table', 'Tabela'), ('formato', 'desk-number-format', 'Formato'),
           ('grafico', 'desk-chart', 'Gráfico (dados)'), ('texto', 'desk-character-style', 'Texto'),
           ('grafica', 'desk-fill', 'Gráfico'), ('metricas', 'desk-ruler', 'Métricas'),
           ('link', 'desk-link', 'Link')]


def inspector_a():
    cg, cr = shell.compact_group, shell.compact_row
    groups = [
        cg('Nome', shell.plain_row(Gtk.Entry(text='Despesas fixas', hexpand=True)), cid='insp.a.nome'),
        cg('Cabeçalhos e rodapé', cr('Linhas de cabeçalho', 1, '', 0), cr('Colunas de cabeçalho', 1, '', 0),
           cr('Linhas de rodapé', 1, '', 0), cid='insp.a.cabecalhos'),
        cg('Linhas e colunas', cr('Altura da linha', 0.64, 'cm', 2), cr('Largura da coluna', 1.9, 'cm', 2),
           shell.switch_line('Ajustar ao conteúdo', False), cid='insp.a.tamanho'),
        cg('Bordas da célula', shell.plain_row(hbox(dropdown(['———', '- - -'], 0), parts.swatch('#9a9996'),
                                                   parts._mini_spin(0.5, 2), spacing=8)), cid='insp.a.bordas'),
        cg('Fundo', shell.plain_row(hbox(label('Fundo da célula', hexpand=True), parts.swatch('#ffffff'))),
           shell.switch_line('Cor alternada nas linhas', True), cid='insp.a.fundo'),
    ]
    return shell.inspector_a(A_ICONS, 2, 'Tabela', None, groups)


# ---------------------------------------------------------------- versions


def version_a(win):
    start = [text_button('Abrir', 'Abrir um documento', 'cab.abrir'),
             menu_button(text='Visualização', tip='Menu Visualização', cid='cab.visualizar-menu',
                         menu=parts.view_menu(APP)),
             menu_button(text='Inserir', tip='Menu Inserir', cid='cab.inserir-menu',
                         menu=parts.insert_menu(APP, objects=True)),
             menu_button(icon='desk-function', text='Função', tip='Inserir função', cid='cab.funcao',
                         menu=FUNCTIONS)]
    end = [menu_button(icon='open-menu', tip='Menu principal', cid='cab.menu', menu=parts.main_menu(APP)),
           icon_button('sidebar-show-right', 'Inspetor', 'cab.inspetor', toggle=True, active=True),
           icon_button('image-x-generic', 'Mídia', 'cab.midia', toggle=True),
           icon_button('desk-sort', 'Reorganizar', 'cab.reorganizar', toggle=True)]
    hb = shell.header_bar(APP, start, end)
    inner = shell.split(inspector_a(), sheet_view(), shell.INSPECTOR_A_W, Gtk.PackType.END)
    body = shell.split(sheets_pane_a(), inner, 168)
    bottom = shell.bottom_bar(parts.flat_menu('100 %', 'Zoom', 'inf.zoom', menu=parts.zoom_menu()), expand(),
                              tag(label('Planilha 2 de 4', 'dimmed', 'tiny', 'numeric', margin_end=8),
                                  'inf.contagem'))
    win.set_content(shell.toolbar_view(body, top=[hb, format_bar_cell('fmt'), formula_bar()], bottom=[bottom]))


def version_b(win, level=shell.FORMAT, selection='range', editing=False):
    side = shell.nav_sidebar(APP, sheets_sidebar(), ['Planilhas', 'Estilos de tabela', 'Comentários',
                                                     'Resultados da busca'], 'Adicionar planilha', ADD_SHEET)
    start = [split_button(icon='sidebar-show', tip='Barra lateral', drop_tip='Visualização',
                          cid='cab.visualizar', menu=parts.view_menu(APP)),
             spacer(),
             parts.insert_menu_button(APP)]
    start += parts.insert_buttons()
    end = [menu_button(icon='open-menu', tip='Menu principal', cid='cab.menu', menu=parts.main_menu(APP)),
           icon_button('sidebar-show-right', 'Inspetor', 'cab.inspetor', toggle=True, active=True),
           icon_button('edit-find', 'Buscar', 'cab.localizar', toggle=True)]
    hb = shell.header_bar(APP, start, end)
    context = 'chart' if selection == 'chart' else 'table'
    body = shell.split(inspector_b(level, context), sheet_view(selection, editing), shell.INSPECTOR_W,
                       Gtk.PackType.END)
    bottom = shell.bottom_bar(
        parts.flat_menu('100 %', 'Zoom', 'inf.zoom', menu=parts.zoom_menu()),
        expand(), calc_chips(), expand(),
        parts.flat_menu('6 células', 'Seleção', 'inf.selecao', arrow=False,
                        menu=menu_model(['Soma', 'Média', 'Mínimo', 'Máximo', 'Contagem', 'Produto'],
                                        ['Copiar resultado'])))
    fbar = format_bar_chart('fmt') if context == 'chart' else format_bar_cell('fmt', slim=True)
    content = shell.toolbar_view(body, top=[hb, fbar, formula_bar(editing)], bottom=[bottom])
    win.set_content(shell.split(side, content, shell.SIDEBAR_W))


def version_c(win, level=shell.FORMAT):
    start = [menu_button(icon='desk-insert', tip='Inserir', cid='cab.inserir-tudo', popover=Gtk.Popover())]
    end = [menu_button(icon='open-menu', tip='Menu principal', cid='cab.menu',
                       menu=parts.main_menu(APP, lean=True)),
           spacer(),
           icon_button('x-office-document', 'Documento', 'cab.documento', toggle=True),
           icon_button('desk-sort', 'Reorganizar', 'cab.reorganizar', toggle=True),
           icon_button('desk-format', 'Formatar', 'cab.formatar', toggle=True, active=True),
           spacer(),
           icon_button('edit-find', 'Buscar', 'cab.localizar', toggle=True)]
    hb = shell.header_bar(APP, start, end)
    view = Adw.TabView()
    zoom = shell.osd_pill(icon_button('zoom-out', 'Reduzir'), label('100 %', 'numeric', margin_start=4, margin_end=4),
                          icon_button('zoom-in', 'Ampliar'), cid='osd.zoom',
                          halign=Gtk.Align.END, valign=Gtk.Align.END, margin_end=14, margin_bottom=12)
    chosen = None
    for n in SHEETS:
        child = sheet_view(overlays=(zoom,)) if n == 'Despesas' else Gtk.Box()
        pg = view.append(child)
        pg.set_title(n)
        if n == 'Despesas':
            chosen = pg
    view.set_selected_page(chosen)
    tabs = Adw.TabBar(view=view, autohide=False, expand_tabs=False)
    tabs.set_end_action_widget(icon_button('list-add', 'Adicionar planilha', 'abas.adicionar'))
    tag(tabs, 'abas.planilhas')
    pages, vis = format_pages()
    body = shell.split(shell.inspector_c(None, pages, vis), view, shell.INSPECTOR_W, Gtk.PackType.END)
    cellbar = hbox(menu_button(icon='desk-function', tip='Função', cid='formula.funcao', menu=FUNCTIONS),
                   tag(label('F2', 'dimmed', 'numeric'), 'formula.celula'),
                   tag(classes(label('=SOMA(B2:E2)', 'formula-entry', hexpand=True), 'numeric'), 'formula.campo'),
                   calc_chips(), spacing=8)
    classes(cellbar, 'toolbar', 'bottombar')
    tag(cellbar, 'inf.celula')
    win.set_content(shell.toolbar_view(body, top=[hb, tabs], bottom=[cellbar]))


SCENES = [
    Scene('tables-a', 'Desk Tables · A Clássica', lambda w: version_a(w), APP, 'A'),
    Scene('tables-b', 'Desk Tables · B Equilibrada', lambda w: version_b(w), APP, 'B'),
    Scene('tables-c', 'Desk Tables · C Enxuta', lambda w: version_c(w), APP, 'C'),
]
