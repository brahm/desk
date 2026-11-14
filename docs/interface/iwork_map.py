#!/usr/bin/env python3
"""Every command and panel of iWork '09, with its place in Desk (interface B).

Reads the inventory taken on the reference virtual machine (Pages 4.3,
Numbers 2.3, Keynote 5.3) and writes one row per menu command, toolbar item,
format bar mode, Inspector tab, preference pane and panel.
"""
import csv
import re
import sys

SRC, OUT = sys.argv[1], sys.argv[2]
COMPONENTS = sys.argv[3] if len(sys.argv) > 3 else ''
APPS = (('Pages', 'pages.md', 'Writer'), ('Numbers', 'numbers.md', 'Tables'), ('Keynote', 'keynote.md', 'Slides'))
COLLAPSE = {'Chart', 'Chart Type', 'Shape', 'Mask with Shape', 'Open Recent', 'Header Rows', 'Header Columns',
            'Footer Rows', 'Format X-Axis', 'Format Y-Axis', 'Merge Field', 'Sender Field', 'Bibliography Format',
            'Smart Build', 'Table'}
HEADERS = ('Send via Mail', 'Send To', 'Export')
ADAPT, SAME, SYSTEM, OUTSIDE = 'adaptado', 'igual', 'sistema', 'fora'
CTX = 'Menu de contexto e busca de comandos'

# (regex over "Menu ▸ Submenu ▸ Item", place in Desk, component of the mockups, how)
MENU_RULES = [
    (r'^(Pages|Numbers|Keynote) ▸ About', 'Menu principal ▸ Sobre', 'cab.menu', SAME),
    (r'^(Pages|Numbers|Keynote) ▸ Preferences', 'Menu principal ▸ Preferências', 'dialogo.preferencias', SAME),
    (r'^(Pages|Numbers|Keynote) ▸ (Try|Provide|Register)', 'Não se aplica ao Desk', '', OUTSIDE),
    (r'^(Pages|Numbers|Keynote) ▸ Quit', 'Atalho Ctrl+Q, sem item de menu', '', ADAPT),
    (r'^(Pages|Numbers|Keynote) ▸', 'Do sistema: o GNOME cuida de janelas e de aplicativos', '', SYSTEM),
    (r'^File ▸ New from (Template|Theme) Chooser', 'Seletor de modelos dentro da janela', 'modelos.janela', ADAPT),
    (r'^File ▸ (New|Open…|Open Recent( \(.*\))?|Save|Save As…|Print…)$', 'Menu principal', 'cab.menu', SAME),
    (r'^(File|Share) ▸ Export ▸ iPod', 'Fora do escopo: aparelho da Apple; o filme sai por Exportar ▸ Filme', '', OUTSIDE),
    (r'^File ▸ Export', 'Menu principal ▸ Exportar', 'cab.menu', SAME),
    (r'^File ▸ Close All', 'Atalho Ctrl+Alt+W e busca de comandos', 'comandos.dialogo', ADAPT),
    (r'^File ▸ Close', 'Botão de fechar da janela e Ctrl+W', 'janela', ADAPT),
    (r'^File ▸ Revert', 'Busca de comandos; o rascunho de recuperação fica no lugar das versões', 'comandos.dialogo', ADAPT),
    (r'^File ▸ Reduce File Size', 'Inspetor ▸ Formato ▸ Imagem e busca de comandos', 'insp.imagem.arquivo', ADAPT),
    (r'^File ▸ Page Setup', 'Inspetor ▸ Documento (papel e margens) e diálogo Imprimir', 'insp.doc.papel', ADAPT),
    (r'^File ▸ Show Print View', 'Menu Visualização', 'cab.visualizar', ADAPT),
    (r'^File ▸ (Record Slideshow|Clear Recording)', 'Menu do botão Reproduzir e Inspetor ▸ Documento ▸ Áudio', 'insp.audio.gravacao', ADAPT),
    (r'^File ▸ Choose Theme', 'Inspetor ▸ Documento ▸ Tema', 'insp.apres.tema', ADAPT),
    (r'^File ▸ (Save as Template|Save Theme)', 'Busca de comandos (Salvar como modelo)', 'comandos.dialogo', ADAPT),
    (r'^Edit ▸ Find ▸ Search', 'Barra lateral ▸ Resultados da busca', 'lat.modo', ADAPT),
    (r'^Edit ▸ Find', 'Barra de busca, pelo botão Buscar', 'busca.barra', ADAPT),
    (r'^Edit ▸ (Spelling|Proofreading|Writing Tools)', 'Menu de contexto do texto e busca de comandos', 'comandos.dialogo', ADAPT),
    (r'^Edit ▸ Track Changes', 'Busca de comandos; a barra de alterações aparece sob a barra de formatação', 'comandos.dialogo', ADAPT),
    (r'^Edit ▸ Mail Merge', 'Busca de comandos; abre um diálogo', 'comandos.dialogo', ADAPT),
    (r'^Edit ▸ EndNote', 'Busca de comandos, na fase das citações', 'comandos.dialogo', ADAPT),
    (r'^Edit ▸ Delete Page', 'Menu de contexto das miniaturas e tecla Delete', 'lat.miniaturas', ADAPT),
    (r'^Edit ▸ (Delete Row|Delete Column|Paste Values|Mark for Move|Move)', 'Menu de contexto da tabela e busca de comandos', 'doc.planilha', ADAPT),
    (r'^Edit ▸', 'Atalho, menu de contexto e busca de comandos', 'comandos.dialogo', ADAPT),
    (r'^Insert ▸ Text Box$', 'Botão Caixa de texto da barra de cabeçalho', 'cab.inserir.texto', SAME),
    (r'^Insert ▸ Shape( \(.*\))?$', 'Botão Forma da barra de cabeçalho', 'cab.inserir.forma', SAME),
    (r'^Insert ▸ Table( \(.*\))?$', 'Botão Tabela da barra de cabeçalho', 'cab.inserir.tabela', SAME),
    (r'^Insert ▸ Chart( \(.*\))?$', 'Botão Gráfico da barra de cabeçalho', 'cab.inserir.grafico', SAME),
    (r'^Insert ▸ Comment$', 'Botão Comentário da barra de cabeçalho', 'cab.inserir.comentario', SAME),
    (r'^Insert ▸ Choose', 'Menu Inserir ▸ Escolher e botão Mídia', 'cab.inserir.midia', SAME),
    (r'^Insert ▸ Sheet$', 'Botão Adicionar da barra lateral e menu Inserir', 'lat.adicionar', ADAPT),
    (r'^Numbers\|Insert ▸ Function', 'Botão Função da barra de fórmulas', 'formula.funcao', ADAPT),
    (r'^Insert ▸ Function', 'Menu de contexto da célula e busca de comandos, com uma célula de tabela selecionada', 'comandos.dialogo', ADAPT),
    (r'^Insert ▸ (Copied Rows|Copied Columns)', 'Menu de contexto da tabela e busca de comandos', 'doc.planilha', ADAPT),
    (r'^Insert ▸ (Text )?Hyperlink', 'Menu Inserir ▸ Link, que abre um popover', 'cab.inserir', ADAPT),
    (r'^Insert ▸ (Merge Field|Sender Field)', 'Menu Inserir ▸ Campo', 'cab.inserir', ADAPT),
    (r'^Insert ▸ (EndNote|MathType)', 'Menu Inserir, na fase das citações e das equações', 'cab.inserir', ADAPT),
    (r'^Insert ▸ Fill', 'Menu de contexto da célula e busca de comandos', 'comandos.dialogo', ADAPT),
    (r'^Insert ▸', 'Menu Inserir', 'cab.inserir', SAME),
    (r'^Slide ▸ New Slide', 'Botão Adicionar da barra lateral', 'lat.adicionar', ADAPT),
    (r'^Slide ▸ Skip Slide', 'Barra de formatação do slide e menu de contexto do navegador', 'fmt.pular', ADAPT),
    (r'^Slide ▸ Show/Hide Slide Numbers', 'Inspetor ▸ Formato ▸ Slide ▸ Aparência', 'insp.slide.aparencia', ADAPT),
    (r'^Slide ▸ Go To', 'Barra inferior (posição do slide) e teclas de página', 'inf.pagina', ADAPT),
    (r'^Slide ▸', 'Menu de contexto do navegador e busca de comandos', 'lat.navegador', ADAPT),
    (r'^Table ▸ Show Reorganize Panel', 'Inspetor ▸ Dados', 'insp.nivel1', ADAPT),
    (r'^Table ▸ (Add|Delete) (Row|Column)', 'Menu Inserir ▸ Linhas e colunas, menu de contexto da tabela e Alt+setas', 'cab.inserir', ADAPT),
    (r'^Table ▸ (Header|Footer|Freeze)', 'Inspetor ▸ Formato ▸ Tabela ▸ Cabeçalhos e rodapé', 'insp.tabela.cabecalhos', ADAPT),
    (r'^Table ▸ (Resize|Distribute|Unhide)', 'Inspetor ▸ Formato ▸ Tabela ▸ Linhas e colunas e menu de contexto da tabela', 'insp.tabela.tamanho', ADAPT),
    (r'^Table ▸ Enable All Categories', 'Inspetor ▸ Dados ▸ Categorias', 'insp.reorg.categorias', ADAPT),
    (r'^Table ▸', 'Menu de contexto da tabela e busca de comandos', 'doc.planilha', ADAPT),
    (r'^Format ▸ Font ▸ Show Fonts', 'Popover Fontes, pelo botão da fonte', 'fontes.popover', ADAPT),
    (r'^Format ▸ Font ▸ (Bold|Italic|Underline)', 'Barra de formatação', 'fmt.negrito', SAME),
    (r'^Format ▸ Font ▸ (Bigger|Smaller)', 'Campo de tamanho da barra de formatação; Ctrl++ e Ctrl+− ficam com o zoom', 'fmt.tamanho', ADAPT),
    (r'^Format ▸ Font', 'Inspetor ▸ Formato ▸ Estilo ▸ Fonte, e menu Mais da barra de formatação', 'insp.texto.fonte', ADAPT),
    (r'^Format ▸ Text ▸ (Align|Center|Justify|Auto Align)', 'Barra de formatação e Inspetor ▸ Formato ▸ Estilo', 'fmt.alinhamento', SAME),
    (r'^Format ▸ Text ▸ (Decrease|Increase)', 'Atalhos e Inspetor ▸ Formato ▸ Estilo ▸ Listas', 'insp.texto.listas', ADAPT),
    (r'^Format ▸ Text ▸ (Copy|Paste) Ruler', 'Mais e busca de comandos', 'fmt.mais', ADAPT),
    (r'^Format ▸ Table', 'Inspetor ▸ Formato ▸ Tabela e menu de contexto da tabela', 'insp.tabela.cabecalhos', ADAPT),
    (r'^Format ▸ Chart ▸ Chart Type', 'Barra de formatação do gráfico e Inspetor ▸ Formato ▸ Gráfico', 'fmt.tipo-grafico', ADAPT),
    (r'^Format ▸ Chart ▸ Show Data Editor', 'Botão Editar dados da barra de formatação do gráfico', 'fmt.editar-dados', ADAPT),
    (r'^Format ▸ Chart', 'Inspetor ▸ Formato ▸ Gráfico, Eixo e Séries', 'insp.eixo.rotulos', ADAPT),
    (r'^Format ▸ (Shape|Text Box|Image)', 'Mais, menu de contexto do objeto e busca de comandos', 'fmt.mais', ADAPT),
    (r'^Format ▸ (Copy|Paste) (Character |Paragraph )?(Style|Animation)', 'Mais, atalhos e busca de comandos', 'fmt.mais', ADAPT),
    (r'^Format ▸ (Create New Paragraph Style|Import Styles)', 'Barra lateral ▸ Estilos e cabeçalho do estilo no Inspetor', 'insp.estilo-atual', ADAPT),
    (r'^Format ▸ Create New Table Style', 'Inspetor ▸ Formato ▸ Tabela ▸ Estilos de tabela', 'insp.tabela.estilos', ADAPT),
    (r'^Format ▸ Reapply Master', 'Menu Mais da barra de formatação do slide e menu de contexto do navegador', 'fmt.mais', ADAPT),
    (r'^Format ▸ (Show Conditional|Create Custom Cell|Manage Custom Cell)', 'Inspetor ▸ Formato ▸ Célula', 'insp.celula.condicional', ADAPT),
    (r'^Format ▸ (Mask|Instant Alpha|Remove Instant Alpha)', 'Barra de formatação da imagem e Inspetor ▸ Formato ▸ Imagem', 'fmt.mascara', ADAPT),
    (r'^Format ▸ (Reapply)', 'Mais e busca de comandos', 'fmt.mais', ADAPT),
    (r'^Format ▸ Advanced', 'Busca de comandos (autoria de modelos e de temas)', 'comandos.dialogo', ADAPT),
    (r'^Format ▸', 'Mais e busca de comandos', 'fmt.mais', ADAPT),
    (r'^Arrange ▸ (Bring to Front|Send to Back)', 'Botões Frente e Trás da barra de formatação do objeto', 'fmt.frente', ADAPT),
    (r'^Arrange ▸', 'Menu Organizar da barra de formatação, Inspetor ▸ Formato ▸ Organizar e menu de contexto', 'fmt.organizar', ADAPT),
    (r'^View ▸ Slide Only', 'Botão Barra lateral, que a esconde (F9)', 'cab.visualizar', ADAPT),
    (r'^View ▸ Light Table', 'Menu Visualização ▸ Mesa de luz, na área do documento', 'doc.mesa-de-luz', ADAPT),
    (r'^View ▸ (Page Thumbnails|Navigator|Outline|Search|Show Styles Drawer|Show Master Slides|Show Formula List)',
     'Barra lateral, pelo botão de conteúdo, e menu Visualização', 'lat.modo', ADAPT),
    (r'^View ▸ Zoom', 'Menu Visualização e botão de zoom da barra inferior', 'inf.zoom', ADAPT),
    (r'^View ▸ Show Presenter Notes', 'Botão Notas da barra inferior e menu Visualização', 'inf.notas', ADAPT),
    (r'^View ▸ Show Inspector', 'Botão Inspetor da barra de cabeçalho', 'cab.inspetor', SAME),
    (r'^View ▸ New Inspector', 'Não se aplica: há um Inspetor só', '', OUTSIDE),
    (r'^View ▸ Show Colors', 'Popover Cores, pelos botões de cor', 'cores.popover', ADAPT),
    (r'^View ▸ Show Adjust Image', 'Botão Ajustar imagem e Inspetor ▸ Formato ▸ Imagem', 'fmt.ajustar-imagem', ADAPT),
    (r'^View ▸ Show Media Browser', 'Botão Mídia da barra de cabeçalho, que abre um popover', 'cab.inserir.midia', ADAPT),
    (r'^View ▸ Enter Full Screen', 'Menu Visualização ▸ Tela cheia (F11)', 'cab.visualizar', ADAPT),
    (r'^View ▸ Show Document Outline', 'Barra lateral ▸ Tópicos e menu Visualização', 'lat.modo', ADAPT),
    (r'^View ▸ Show Document Warnings', 'Faixa de avisos sobre o documento', 'aviso.banner', ADAPT),
    (r'^View ▸ Show Function Browser', 'Botão Função da barra de fórmulas', 'formula.funcao', ADAPT),
    (r'^View ▸ (Show Toolbar|Hide Toolbar|Customize Toolbar)', 'Não se aplica: a barra de cabeçalho não é personalizável', '', OUTSIDE),
    (r'^View ▸', 'Menu Visualização', 'cab.visualizar', SAME),
    (r'^Play ▸ Customize Presenter Display', 'Preferências ▸ Apresentação', 'dialogo.preferencias', ADAPT),
    (r'^Play ▸', 'Botão Reproduzir e o menu dele', 'cab.reproduzir', ADAPT),
    (r'^Window ▸', 'Do sistema: o GNOME cuida das janelas', '', SYSTEM),
    (r'^Share ▸ (Share via iWork|Show Shared|No unread|Sign In)', 'Fora do escopo: o iWork.com foi encerrado', '', OUTSIDE),
    (r'^Share ▸ Send To', 'Fora do escopo: serviços e programas da Apple', '', OUTSIDE),
    (r'^Share ▸ Send via Mail', 'Menu principal ▸ Exportar ▸ Enviar por e-mail', 'cab.menu', ADAPT),
    (r'^Share ▸ Export', 'Menu principal ▸ Exportar', 'cab.menu', ADAPT),
    (r'^Share ▸', 'Fora do escopo: o iWork.com foi encerrado', '', OUTSIDE),
    (r'^Help ▸ Keyboard Shortcuts', 'Menu principal ▸ Atalhos de teclado', 'dialogo.atalhos', ADAPT),
    (r'^Help ▸ (Welcome|Video Tutorials|iWork Resources|Service and Support)', 'Fora do escopo: material da Apple', '', OUTSIDE),
    (r'^Help ▸', 'Menu principal ▸ Ajuda', 'cab.menu', SAME),
]

TOOLBAR = [
    (r'^(View)$', 'Botão Barra lateral, com o menu Visualização', 'cab.visualizar', ADAPT),
    (r'^Full Screen$', 'Menu Visualização ▸ Tela cheia (F11)', 'cab.visualizar', ADAPT),
    (r'^Outline$', 'Barra lateral ▸ Tópicos e menu Visualização', 'lat.modo', ADAPT),
    (r'^(Pages|Sheet|New)$', 'Botão Adicionar da barra lateral', 'lat.adicionar', ADAPT),
    (r'^(Columns|Style|Character|List|Increase|Decrease|Bigger|Smaller|Subscript|Superscript)$', 'Barra de formatação e Inspetor ▸ Formato', 'fmt.barra', ADAPT),
    (r'^Guides$', 'Menu Visualização', 'cab.visualizar', ADAPT),
    (r'^Share$', 'Fora do escopo: o iWork.com foi encerrado', '', OUTSIDE),
    (r'^Send To$', 'Fora do escopo: serviços e programas da Apple', '', OUTSIDE),
    (r'^Track Changes$', 'Busca de comandos', 'comandos.dialogo', ADAPT),
    (r'^Wrap$', 'Barra de formatação do objeto', 'fmt.ajuste', ADAPT),
    (r'^(Copy Style|Paste Style)$', 'Mais, atalhos e busca de comandos', 'fmt.mais', ADAPT),
    (r'^(Lock|Unlock|Group|Ungroup|Forward|Backward|Flip Horizontal(ly)?|Flip Vertical(ly)?|Connect)$', 'Menu Organizar da barra de formatação do objeto', 'fmt.organizar', ADAPT),
    (r'^Front$', 'Botão Frente da barra de formatação do objeto', 'fmt.frente', ADAPT),
    (r'^Back$', 'Botão Trás da barra de formatação do objeto', 'fmt.fundo', ADAPT),
    (r'^Text Box$', 'Botão Caixa de texto da barra de cabeçalho', 'cab.inserir.texto', SAME),
    (r'^Shapes$', 'Botão Forma da barra de cabeçalho', 'cab.inserir.forma', SAME),
    (r'^Tables?$', 'Botão Tabela da barra de cabeçalho', 'cab.inserir.tabela', SAME),
    (r'^Charts$', 'Botão Gráfico da barra de cabeçalho', 'cab.inserir.grafico', SAME),
    (r'^Comment$', 'Botão Comentário da barra de cabeçalho', 'cab.inserir.comentario', SAME),
    (r'^Objects$', 'Os botões de inserir; em janela estreita, o menu Inserir', 'cab.inserir', ADAPT),
    (r'^Inspector$', 'Botão Inspetor da barra de cabeçalho', 'cab.inspetor', SAME),
    (r'^Media$', 'Botão Mídia da barra de cabeçalho', 'cab.inserir.midia', ADAPT),
    (r'^Adjust$', 'Botão Ajustar imagem da barra de formatação', 'fmt.ajustar-imagem', ADAPT),
    (r'^Colors$', 'Popover Cores, pelos botões de cor', 'cores.popover', ADAPT),
    (r'^Fonts$', 'Popover Fontes, pelo botão da fonte', 'fontes.popover', ADAPT),
    (r'^Format Bar$', 'Menu Visualização ▸ Barra de formatação', 'cab.visualizar', ADAPT),
    (r'^(Alpha|Instant Alpha)$', 'Botão Alfa instantâneo da barra de formatação', 'fmt.alfa', ADAPT),
    (r'^Mask$', 'Botão Editar máscara da barra de formatação', 'fmt.mascara', ADAPT),
    (r'^Data Editor$', 'Botão Editar dados da barra de formatação do gráfico', 'fmt.editar-dados', ADAPT),
    (r'^Find$', 'Botão Buscar da barra de cabeçalho', 'cab.localizar', SAME),
    (r'^Print$', 'Menu principal ▸ Imprimir', 'cab.menu', ADAPT),
    (r'^(Customize|Customize Toolbar)$', 'Não se aplica: a barra de cabeçalho não é personalizável', '', OUTSIDE),
    (r'^Function$', 'Botão Função da barra de fórmulas', 'formula.funcao', ADAPT),
    (r'^Reorganize$', 'Inspetor ▸ Dados', 'insp.nivel1', ADAPT),
    (r'^Formula List$', 'Barra lateral, pelo botão de conteúdo', 'lat.modo', ADAPT),
    (r'^Delete$', 'Tecla Delete e menu de contexto do navegador', 'lat.navegador', ADAPT),
    (r'^Skip$', 'Barra de formatação do slide e menu de contexto do navegador', 'fmt.pular', ADAPT),
    (r'^(Play|Rehearse|Record)$', 'Botão Reproduzir e o menu dele', 'cab.reproduzir', ADAPT),
    (r'^Masters$', 'Barra de formatação do slide e Inspetor ▸ Formato ▸ Slide', 'fmt.mestre', ADAPT),
    (r'^Themes$', 'Inspetor ▸ Documento ▸ Tema', 'insp.apres.tema', ADAPT),
    (r'^Edges On$', 'Menu Visualização', 'cab.visualizar', ADAPT),
    (r'^Smart Builds$', 'Menu Inserir', 'cab.inserir', ADAPT),
]

FORMATBAR = [
    (r'^text$', 'Barra de formatação de texto', 'fmt.barra'),
    (r'^outliner$', 'Barra de formatação de texto, no modo de tópicos', 'fmt.barra'),
    (r'^(graphic|shape|image|line|movie|maskedImage|imageMask|drawable|group)', 'Barra de formatação de objeto', 'fmt.barra'),
    (r'^(table|tableCell|formula|cell)', 'Barra de formatação de célula e barra de fórmulas', 'fmt.barra'),
    (r'.', 'Barra de formatação de gráfico; os detalhes ficam no Inspetor ▸ Formato ▸ Gráfico, Eixo e Séries', 'fmt.barra'),
]

# Inspector panes and tabs, preference panes and panels: (app, kind, item, pt-BR of Apple, place, component, how)
MANUAL = '''
Pages|Inspetor|Document ▸ Document|Documento|Inspetor ▸ Documento ▸ Documento|insp.doc.papel|adaptado
Pages|Inspetor|Document ▸ TOC|Índice|Inspetor ▸ Formato, com o sumário selecionado|insp.abas|adaptado
Pages|Inspetor|Document ▸ Info|Info|Inspetor ▸ Documento ▸ Info|insp.info.campos|igual
Pages|Inspetor|Layout ▸ Layout|Leiaute|Inspetor ▸ Formato ▸ Layout (colunas) e Inspetor ▸ Documento ▸ Seção|insp.layout.colunas|adaptado
Pages|Inspetor|Layout ▸ Section|Seção|Inspetor ▸ Documento ▸ Seção|insp.secao.config|adaptado
Pages|Inspetor|Wrap|Ajuste|Inspetor ▸ Formato ▸ Organizar ▸ Ajuste do texto|insp.organizar.ajuste|adaptado
Pages|Inspetor|Text ▸ Text|Texto|Inspetor ▸ Formato ▸ Estilo|insp.texto.espacamento|adaptado
Pages|Inspetor|Text ▸ List|Lista|Inspetor ▸ Formato ▸ Estilo ▸ Listas|insp.texto.listas|adaptado
Pages|Inspetor|Text ▸ Tabs|Tabulações|Inspetor ▸ Formato ▸ Layout ▸ Tabulações|insp.layout.tabulacoes|adaptado
Pages|Inspetor|Text ▸ More|Mais|Inspetor ▸ Formato ▸ Mais|insp.mais.paginacao|igual
Pages|Inspetor|Graphic|Gráfico|Inspetor ▸ Formato ▸ Estilo, com um objeto selecionado|insp.estilo.traco|adaptado
Pages|Inspetor|Metrics|Métricas|Inspetor ▸ Formato ▸ Organizar ▸ Tamanho, Posição e giro|insp.organizar.metricas|adaptado
Pages|Inspetor|Table ▸ Table|Tabela|Inspetor ▸ Formato ▸ Tabela|insp.tabela.cabecalhos|adaptado
Pages|Inspetor|Table ▸ Format|Formato|Inspetor ▸ Formato ▸ Célula|insp.celula.formato|adaptado
Pages|Inspetor|Chart|Gráfico (dados)|Inspetor ▸ Formato ▸ Gráfico, Eixo e Séries|insp.grafico.opcoes|adaptado
Pages|Inspetor|Link ▸ Hyperlink|Hiperlink|Popover do link, pelo menu Inserir ▸ Link e pelo menu de contexto|cab.inserir|adaptado
Pages|Inspetor|Link ▸ Bookmark|Marcador|Barra lateral, pelo botão de conteúdo, e menu Inserir ▸ Marcador|lat.modo|adaptado
Pages|Inspetor|Link ▸ Merge|Mesclar|Diálogo da mala direta|comandos.dialogo|adaptado
Pages|Inspetor|QuickTime|QuickTime|Inspetor ▸ Formato, com som ou filme selecionado|insp.abas|adaptado
Numbers|Inspetor|Document|Documento|Inspetor ▸ Documento ▸ Info|insp.info.campos|adaptado
Numbers|Inspetor|Sheet|Folha|Inspetor ▸ Documento ▸ Planilha|insp.planilha.impressao|adaptado
Numbers|Inspetor|Table|Tabela|Inspetor ▸ Formato ▸ Tabela|insp.tabela.cabecalhos|adaptado
Numbers|Inspetor|Cells|Células|Inspetor ▸ Formato ▸ Célula|insp.celula.formato|adaptado
Numbers|Inspetor|Chart|Gráfico (dados)|Inspetor ▸ Formato ▸ Gráfico, Eixo e Séries|insp.grafico.opcoes|adaptado
Numbers|Inspetor|Text ▸ Text|Texto|Inspetor ▸ Formato ▸ Texto|insp.texto.espacamento|adaptado
Numbers|Inspetor|Text ▸ Columns|Colunas|Inspetor ▸ Formato ▸ Texto|insp.layout.colunas|adaptado
Numbers|Inspetor|Text ▸ Bullets|Marcadores|Inspetor ▸ Formato ▸ Texto ▸ Listas|insp.texto.listas|adaptado
Numbers|Inspetor|Graphic|Gráfico|Inspetor ▸ Formato ▸ Estilo, com um objeto selecionado|insp.estilo.traco|adaptado
Numbers|Inspetor|Metrics|Métricas|Inspetor ▸ Formato ▸ Organizar|insp.organizar.metricas|adaptado
Numbers|Inspetor|Hyperlink|Hiperlink|Popover do link, pelo menu Inserir ▸ Link e pelo menu de contexto|cab.inserir|adaptado
Numbers|Inspetor|QuickTime|QuickTime|Inspetor ▸ Formato, com som ou filme selecionado|insp.abas|adaptado
Keynote|Inspetor|Document ▸ Document|Documento|Inspetor ▸ Documento ▸ Documento|insp.apres.opcoes|adaptado
Keynote|Inspetor|Document ▸ Audio|Áudio|Inspetor ▸ Documento ▸ Áudio|insp.audio.trilha|igual
Keynote|Inspetor|Document ▸ Spotlight|Spotlight|Inspetor ▸ Documento ▸ Info|insp.info.campos|adaptado
Keynote|Inspetor|Slide ▸ Transition|Transição|Inspetor ▸ Animação, sem nada selecionado|insp.transicao.efeito|adaptado
Keynote|Inspetor|Slide ▸ Appearance|Aparência|Inspetor ▸ Formato ▸ Slide, sem nada selecionado|insp.slide.aparencia|adaptado
Keynote|Inspetor|Build ▸ Build In|Entrada|Inspetor ▸ Animação ▸ Entrada|insp.entrada.efeito|adaptado
Keynote|Inspetor|Build ▸ Build Out|Saída|Inspetor ▸ Animação ▸ Saída|insp.saida.efeito|adaptado
Keynote|Inspetor|Build ▸ Action|Ação|Inspetor ▸ Animação ▸ Ação|insp.acao.efeito|adaptado
Keynote|Inspetor|Build ▸ Build Order (gaveta)|Ordem das composições|Inspetor ▸ Animação: a ordem fica no pé de cada aba|insp.entrada.ordem|adaptado
Keynote|Inspetor|Text ▸ Text|Texto|Inspetor ▸ Formato ▸ Texto|insp.texto.espacamento|adaptado
Keynote|Inspetor|Text ▸ Columns|Colunas|Inspetor ▸ Formato ▸ Texto|insp.layout.colunas|adaptado
Keynote|Inspetor|Text ▸ Bullets|Marcadores|Inspetor ▸ Formato ▸ Texto ▸ Listas|insp.texto.listas|adaptado
Keynote|Inspetor|Graphic|Gráfico|Inspetor ▸ Formato ▸ Estilo, com um objeto selecionado|insp.estilo.traco|adaptado
Keynote|Inspetor|Metrics|Métricas|Inspetor ▸ Formato ▸ Organizar|insp.organizar.metricas|adaptado
Keynote|Inspetor|Table ▸ Table|Tabela|Inspetor ▸ Formato ▸ Tabela|insp.tabela.cabecalhos|adaptado
Keynote|Inspetor|Table ▸ Format|Formato|Inspetor ▸ Formato ▸ Célula|insp.celula.formato|adaptado
Keynote|Inspetor|Chart|Gráfico (dados)|Inspetor ▸ Formato ▸ Gráfico, Eixo e Séries|insp.grafico.opcoes|adaptado
Keynote|Inspetor|Hyperlink|Hiperlink|Popover do link, pelo menu Inserir ▸ Link e pelo menu de contexto|cab.inserir|adaptado
Keynote|Inspetor|QuickTime|QuickTime|Inspetor ▸ Formato, com som ou filme selecionado|insp.abas|adaptado
Pages|Preferências|General|Geral|Preferências ▸ Geral|dialogo.preferencias|igual
Pages|Preferências|Rulers|Réguas|Preferências ▸ Réguas|dialogo.preferencias|igual
Pages|Preferências|Auto-Correction|Correção Automática|Preferências ▸ Correção automática|dialogo.preferencias|igual
Numbers|Preferências|General|Geral|Preferências ▸ Geral|dialogo.preferencias|igual
Numbers|Preferências|Rulers|Réguas|Preferências ▸ Réguas|dialogo.preferencias|igual
Numbers|Preferências|Auto-Correction|Correção Automática|Preferências ▸ Correção automática|dialogo.preferencias|igual
Keynote|Preferências|General|Geral|Preferências ▸ Geral|dialogo.preferencias|igual
Keynote|Preferências|Rulers|Réguas|Preferências ▸ Réguas|dialogo.preferencias|igual
Keynote|Preferências|Slideshow|Apresentação de Slides|Preferências ▸ Apresentação|dialogo.preferencias|igual
Keynote|Preferências|Presenter Display|Monitor do Apresentador|Preferências ▸ Apresentação ▸ Tela do apresentador|dialogo.preferencias|adaptado
Keynote|Preferências|Remote|Remoto|Fora do escopo: o Keynote Remote era um aplicativo do iPhone|-|fora
Keynote|Preferências|Auto-Correction|Correção Automática|Preferências ▸ Correção automática|dialogo.preferencias|igual
Pages|Janelas e folhas|Template Chooser|Seletor de Modelos|Seletor de modelos dentro da janela|modelos.janela|adaptado
Pages|Janelas e folhas|Find & Replace|Buscar e Substituir|Barra de busca sob a barra de cabeçalho|busca.barra|adaptado
Pages|Janelas e folhas|Search (busca lateral)|Buscar|Barra lateral ▸ Resultados da busca|lat.modo|adaptado
Pages|Janelas e folhas|Spelling e Proofreader|Ortografia e Revisor|Popover no texto e menu de contexto|comandos.dialogo|adaptado
Pages|Janelas e folhas|Export|Exportar|Diálogo de exportação, pelo menu principal ▸ Exportar|cab.menu|adaptado
Pages|Janelas e folhas|Opções de Save|Opções de salvar|Seletor de arquivos do GNOME; as opções ficam em Inspetor ▸ Documento|insp.doc.senha|adaptado
Pages|Janelas e folhas|Mail Merge|Mala direta|Diálogo, pela busca de comandos|comandos.dialogo|adaptado
Pages|Janelas e folhas|Numeração de página, seções e exclusão de páginas|-|Inspetor ▸ Documento ▸ Seção e menu de contexto das miniaturas|insp.secao.numeracao|adaptado
Pages|Janelas e folhas|Estilos: criar, excluir, importar|-|Diálogos pequenos, pela barra lateral ▸ Estilos|lat.modo|adaptado
Pages|Janelas e folhas|EndNote|-|Fase das citações|comandos.dialogo|adaptado
Pages|Janelas e folhas|Avisos de abertura, colagem e salvamento|-|Faixa de avisos, toasts e diálogos de alerta|aviso.banner|adaptado
Pages|Janelas e folhas|Styles Drawer|Gaveta de Estilos|Barra lateral ▸ Estilos|lat.modo|adaptado
Pages|Janelas e folhas|Fonts (painel do sistema)|Fontes|Popover Fontes|fontes.popover|adaptado
Pages|Janelas e folhas|Colors (painel do sistema)|Cores|Popover Cores|cores.popover|adaptado
Pages|Janelas e folhas|Media Browser|Navegador de Mídia|Popover do botão Mídia|cab.inserir.midia|adaptado
Pages|Janelas e folhas|Adjust Image|Ajustar Imagem|Inspetor ▸ Formato ▸ Imagem ▸ Ajustes|insp.imagem.ajustar|adaptado
Pages|Janelas e folhas|Document Warnings|Avisos do Documento|Faixa de avisos, que abre a lista|aviso.banner|adaptado
Pages|Janelas e folhas|Customize Toolbar|Personalizar Barra de Ferramentas|Não se aplica: a barra de cabeçalho não é personalizável|-|fora
Pages|Janelas e folhas|Tela cheia|Tela Cheia|Tela cheia do GNOME (F11), com as barras recolhidas|cab.visualizar|adaptado
Numbers|Janelas e folhas|Template Chooser|Seletor de Modelos|Seletor de modelos dentro da janela|modelos.janela|adaptado
Numbers|Janelas e folhas|Reorganize (ordenar, filtrar, categorias)|Reorganizar|Inspetor ▸ Dados ▸ Ordem, Filtro e Categorias|insp.reorg.ordenar|adaptado
Numbers|Janelas e folhas|Formula List e busca|Lista de Fórmulas|Barra lateral, pelo botão de conteúdo|lat.modo|adaptado
Numbers|Janelas e folhas|Function Browser e editor de fórmulas|Navegador de Funções|Popover do botão Função e edição na barra de fórmulas|formula.funcao|adaptado
Numbers|Janelas e folhas|Find & Replace|Buscar e Substituir|Barra de busca sob a barra de cabeçalho|busca.barra|adaptado
Numbers|Janelas e folhas|Export|Exportar|Diálogo de exportação, pelo menu principal ▸ Exportar|cab.menu|adaptado
Numbers|Janelas e folhas|Print|Imprimir|Diálogo Imprimir do GNOME, com a visualização de impressão|cab.menu|adaptado
Numbers|Janelas e folhas|Estilos de tabela e tabelas predefinidas|-|Barra lateral ▸ Estilos de tabela e popover do botão Tabela|cab.inserir.tabela|adaptado
Numbers|Janelas e folhas|Painel de planilhas, estilos e cálculo instantâneo|-|Barra lateral ▸ Planilhas e resultados na barra inferior|calc.resultados|adaptado
Keynote|Janelas e folhas|Theme Chooser|Seletor de Temas|Seletor de modelos dentro da janela|modelos.janela|adaptado
Keynote|Janelas e folhas|Export|Exportar|Diálogo de exportação, pelo menu principal ▸ Exportar|cab.menu|adaptado
Keynote|Janelas e folhas|Send To|Enviar a|Fora do escopo: serviços e programas da Apple|-|fora
Keynote|Janelas e folhas|Print|Imprimir|Diálogo Imprimir do GNOME, com as opções de slides, notas e folhetos|cab.menu|adaptado
Keynote|Janelas e folhas|Find & Replace|Buscar e Substituir|Barra de busca sob a barra de cabeçalho|busca.barra|adaptado
Keynote|Janelas e folhas|Choose Theme e tamanho do slide|Escolher Tema|Inspetor ▸ Documento ▸ Tema e Tamanho do slide|insp.apres.tamanho|adaptado
Keynote|Janelas e folhas|Save e senhas|-|Seletor de arquivos do GNOME; as senhas ficam em Inspetor ▸ Documento|insp.apres.opcoes|adaptado
Keynote|Janelas e folhas|Customize Presenter Display|Personalizar Monitor do Apresentador|Preferências ▸ Apresentação ▸ Tela do apresentador|dialogo.preferencias|adaptado
Keynote|Janelas e folhas|Apresentação em execução (Play, Rehearse, Record)|-|Janela de apresentação em tela cheia e tela do apresentador|cab.reproduzir|adaptado
Keynote|Janelas e folhas|Smart Builds e Magic Move|Composições Inteligentes e Movimento Mágico|Menu Inserir e Inspetor ▸ Animação|insp.transicao.efeito|adaptado
Keynote|Janelas e folhas|Light Table|Mesa de Luz|Menu Visualização ▸ Mesa de luz, na área do documento|doc.mesa-de-luz|adaptado
Keynote|Janelas e folhas|Master Slides (lista)|Slides Mestre|Barra lateral ▸ Slides mestres|lat.modo|adaptado
Keynote|Janelas e folhas|Presenter Notes|Notas do Apresentador|Área de notas sob o slide, pelo botão da barra inferior|doc.notas|adaptado
Keynote|Janelas e folhas|Ações do Automator|-|Fora do escopo: Automator|-|fora
'''


def parse_menus(lines):
    """Yield (path list, pt-BR name, shortcut, number of children collapsed)."""
    stack, rows, header = [], [], None
    for raw in lines:
        m = re.match(r'^(\s*)- "([^"]+)"(?: / pt-BR "([^"]*)")?(.*)$', raw)
        if not m:
            if '———' in raw:
                header = None           # a separator ends the group of a header
            continue
        ind, name, pt, rest = len(m.group(1)), m.group(2).strip(), (m.group(3) or '').strip(), m.group(4)
        depth = 0 if ind <= 2 else (ind - 2) // 2
        while stack and stack[-1][0] >= depth:
            stack.pop()
        if depth == 0:
            header = None
        # Share lists its groups flat, under headers that the nib keeps disabled
        if stack and stack[-1][1] == 'Share' and 'desativado no nib' in rest and name in HEADERS:
            header = name
            continue
        if header and stack and stack[-1][1] == 'Share':
            name = f'{header} ▸ {name}'
        sc = re.search(r'— `([^`]*(?:Cmd|Ctrl|Opt|Shift)\+[^`]*|F\d+)`', rest)
        parent_collapsed = any(s[2] for s in stack)
        collapsed = name in COLLAPSE and depth >= 1 and '▸' in rest
        if parent_collapsed:
            for s in stack:
                if s[2]:
                    s[3]['n'] += 1
        else:
            row = {'path': [s[1] for s in stack] + [name], 'pt': pt, 'sc': sc.group(1) if sc else '', 'n': 0,
                   'menu': '▸' in rest}
            if depth >= 1:
                rows.append(row)
            stack.append((depth, name, collapsed, row))
            continue
        stack.append((depth, name, False, {'n': 0}))
    return rows


def first(rules, text, app=''):
    for rule in rules:
        if re.search(rule[0], text) or re.search(rule[0], f'{app}|{text}'):
            return rule[1:]
    raise SystemExit('sem regra: ' + text)


def section(lines, start, stop):
    a = next(i for i, l in enumerate(lines) if re.match(start, l))
    b = next((i for i, l in enumerate(lines) if i > a and re.match(stop, l)), len(lines))
    return lines[a + 1:b]


def main():
    out = []
    known = {l.split('\t')[0] for l in open(COMPONENTS, encoding='utf-8')} if COMPONENTS else None
    for app, fname, desk in APPS:
        lines = open(f'{SRC}/{fname}', encoding='utf-8').read().split('\n')
        for row in parse_menus(section(lines, r'^### 2\.1 ', r'^### 2\.2 ')):
            if row['menu'] and not row['n'] and row['path'][-1] not in COLLAPSE:
                continue                    # a submenu title; its items follow
            path = ' ▸ '.join(row['path'])
            place, comp, how = first(MENU_RULES, path, app)
            name = path + (f' ({row["n"]} itens)' if row['n'] else '')
            out.append([app, desk, 'Menu', name, row['pt'] or '-', row['sc'] or '-', place, comp or '-', how])
        seen = set()
        tb = section(lines, r'^\*\*Todos os itens que a barra aceita', r'^### 3\.3')
        for raw in tb:
            m = re.match(r'^- "([^"]+)"(?: / pt-BR "([^"]*)")?', raw)
            if not m or m.group(1).strip() in seen:
                continue
            label = m.group(1).strip()
            seen.add(label)
            place, comp, how = first(TOOLBAR, label)
            out.append([app, desk, 'Barra de ferramentas', label, (m.group(2) or '-').strip() or '-', '-', place,
                        comp or '-', how])
        for raw in section(lines, r'^### 3\.3', r'^### 3\.4'):
            m = re.match(r'^\| `([A-Za-z]+)` \| (.*) \|$', raw)
            if not m:
                continue
            controls = len(re.findall(r'`[A-Za-z]+`', m.group(2)))
            place, comp = first(FORMATBAR, m.group(1))
            out.append([app, desk, 'Barra de formatação', f'modo {m.group(1)} ({controls} controles)', '-', '-', place,
                        comp, ADAPT])
    for raw in MANUAL.strip().split('\n'):
        app, kind, item, pt, place, comp, how = raw.split('|')
        desk = dict((a, d) for a, _f, d in APPS)[app]
        out.append([app, desk, kind, item, pt, '-', place, comp, how])
    if known:
        bad = sorted({r[7] for r in out if r[7] != '-' and r[7] not in known})
        if bad:
            raise SystemExit('componente fora do inventário: ' + ', '.join(bad))
    order = {'Menu': 0, 'Barra de ferramentas': 1, 'Barra de formatação': 2, 'Inspetor': 3, 'Preferências': 4,
             'Janelas e folhas': 5}
    apps = [a for a, _f, _d in APPS]
    out.sort(key=lambda r: (apps.index(r[0]), order[r[2]]))
    with open(OUT, 'w', encoding='utf-8', newline='') as f:
        f.write('# SPDX-FileCopyrightText: 2026 Desk contributors\n# SPDX-License-Identifier: GPL-3.0-or-later\n#\n'
                '# Every menu command, toolbar item, format bar mode, Inspector tab, preference\n'
                '# pane and panel of Pages 4.3, Numbers 2.3 and Keynote 5.3 (iWork \'09 9.3), read\n'
                '# on the reference virtual machine, and its place in the interface of Desk.\n'
                '# como: igual (same place and form), adaptado (another place or form, the GNOME\n'
                '# way), sistema (GNOME does it), fora (out of scope). componente names a row of\n'
                '# componentes.tsv. Tab-separated.\n')
        w = csv.writer(f, delimiter='\t', lineterminator='\n', quoting=csv.QUOTE_NONE, quotechar=None)
        w.writerow(['iwork', 'desk', 'onde_no_iwork', 'item', 'nome_da_apple', 'atalho_do_iwork', 'lugar_no_desk',
                    'componente', 'como'])
        w.writerows(out)
    from collections import Counter
    print(len(out), Counter((r[0], r[2]) for r in out))
    print(Counter(r[8] for r in out))
    return out


if __name__ == '__main__':
    main()
