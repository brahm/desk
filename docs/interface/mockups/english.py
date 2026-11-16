# SPDX-FileCopyrightText: 2026 Desk contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Draws the mockups in English.

The scenes were written with their text in Brazilian Portuguese. Unless
DESK_MOCKUP_LANG=pt is in the environment, render.py calls install() before
it imports the scene modules, and every string constant of those modules that
is a key of EN is replaced by its translation as the module is compiled.
Strings that are not keys (identifiers, CSS, icon names) are left alone, so
a missing translation shows as Portuguese text in the picture.
"""
import ast
import importlib.abc
import importlib.machinery
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
MODULES = ('writer', 'tables', 'slides', 'parts', 'shell', 'canvas', 'extras', 'proposals')

_RAW = """
 · Editado| · Edited
26.171 palavras · 41 páginas|26,171 words · 41 pages
26.171 palavras|26,171 words
6 células|6 cells
=SOMA(B2:E2)|=SUM(B2:E2)
A qualquer regra|Any rule
A receita cresceu doze por cento em relação ao ano anterior, puxada pelas vendas diretas e pela abertura de duas lojas. O custo de logística caiu depois da troca de transportadora, e a margem voltou ao patamar de 2023. |Revenue grew twelve percent over the previous year, driven by direct sales and the opening of two stores. Logistics costs fell after the change of carrier, and the margin returned to the level of 2023. 
A seção começa em|Section starts on
A tabela inteira|The whole table
A todas as regras|All rules
Abr|Apr
Abrir|Open
Abrir recente|Open Recent
Abrir um documento|Open a document
Abrir…|Open…
Aceitar|Accept
Acessibilidade|Accessibility
Acima e abaixo|Above and below
Adicionar coluna|Add column
Adicionar planilha|Add sheet
Adicionar página|Add page
Adicionar regra|Add rule
Adicionar slide|Add slide
Adicionar tabela ou gráfico|Add table or chart
Adicionar tabulação|Add tab stop
Agrupar|Group
Ajuda|Help
Ajustar ao conteúdo|Fit to content
Ajustar imagem|Adjust image
Ajustar texto na célula|Wrap text in cell
Ajustar à janela|Fit in window
Ajustar à largura|Fit width
Ajustar à página|Fit page
Ajuste|Wrap
Ajuste do texto|Text wrap
Alfa instantâneo|Instant Alpha
Alinhamento|Alignment
Alinhamento vertical|Vertical alignment
Alinhar|Align
Alinhar automaticamente|Align automatically
Alinhar objetos|Align objects
Alinhar à direita|Align right
Alinhar à esquerda|Align left
Altura|Height
Altura da linha|Row height
Aluguel|Rent
Ampliar|Zoom in
Animar|Animate
Animação|Animation
Anterior|Previous
Antes do parágrafo|Before paragraph
Ao clicar|On click
Aparecer|Appear
Aparência|Appearance
Apresentação|Presentation
Após a composição anterior|After previous build
Arquivo|File
As linhas selecionadas|The selected rows
Ata de março|March minutes
Atalhos de teclado|Keyboard Shortcuts
Atraso|Delay
Atualizar|Update
Aumentar recuo|Increase indent
Automaticamente|Automatically
Automático|Automatic
Autor|Author
Avançado|Advanced
Azul|Blue
Ação|Action
Barra de formatação|Format bar
Barra de fórmulas|Formula bar
Barra lateral|Sidebar
Barras|Bars
Barras de erro|Error bars
Borda|Border
Bordas da célula|Cell borders
Bordas e fundo do parágrafo|Paragraph borders and background
Branco|White
Buscar|Find
Buscar comandos…|Search Commands…
Buscar fontes…|Search fonts…
Buscar formas…|Search shapes…
Buscar no documento…|Find in document…
Básicas|Basic
Básico|Basic
CONT.|COUNT
Cabeçalho|Header
Cabeçalho 1|Heading 1
Cabeçalho 2|Heading 2
Cabeçalho e rodapé|Header and footer
Cabeçalhos cinza|Gray headers
Cabeçalhos e rodapé|Headers and footer
Cabeçalhos e rodapés|Headers and footers
Caixa de seleção|Checkbox
Caixa de texto|Text box
Calendário|Schedule
Campanha em rádio e redes|Radio and social campaign
Campo|Field
Campo de mesclagem|Merge field
Cancelar|Cancel
Capa|Cover
Capítulo|Chapter
Caractere|Character
Caracteres|Characters
Caracteres especiais…|Special characters…
Caracteres sem espaços|Characters without spaces
Carta|Letter
Casas decimais|Decimals
Categoria|Category
Categoria (X)|Category (X)
Categorias|Categories
Categorizar por|Categorize by
Cena 3D|3D scene
Centralizar|Center
Centro|Center
Científico|Scientific
Cinza|Gray
Citar a pesquisa de fevereiro. Perguntar quem já usa o serviço nas três capitais e mostrar o mapa antes de falar dos parceiros.|Mention the February survey. Ask who already uses the service in the three capitals and show the map before talking about the partners.
Citação|Quote
Colar estilo|Paste style
Colar estilo gráfico|Paste graphic style
Coluna A|Column A
Coluna antes|Column before
Coluna depois|Column after
Colunas|Columns
Colunas de cabeçalho|Header columns
Colunas empilhadas|Stacked columns
Com a composição anterior|With previous build
Comentário|Comment
Comentários|Comments
Composição|Build
Composição inteligente|Smart build
Congelar os cabeçalhos|Freeze headers
Conta-gotas|Eyedropper
Contagem|Count
Contagem de palavras|Word count
Conteúdo da barra lateral|Sidebar content
Continuar|Continue
Contorno|Outline
Contraste|Contrast
Controle de alterações|Track changes
Controle deslizante|Slider
Copiar estilo|Copy style
Copiar estilo de caractere|Copy character style
Copiar estilo de parágrafo|Copy paragraph style
Copiar estilo gráfico|Copy graphic style
Copiar resultado|Copy result
Cor|Color
Cor alternada nas linhas|Alternating row color
Cor de fundo|Background color
Cor de fundo do texto|Text background color
Cor do texto|Text color
Cor do traço|Stroke color
Cor e alinhamento|Color and alignment
Cores do gráfico|Chart colors
Corpo|Body
Corresponder|Match
Crescente|Ascending
Criar formato de célula personalizado…|Create custom cell format…
Cubo|Cube
Célula|Cell
Célula ativa|Active cell
Da direita para a esquerda|Right to left
Da esquerda para a direita|Left to right
Dados|Data
Data e hora|Date and time
De baixo|From bottom
De cima|From top
De coluna|Column
De layout|Layout
De página|Page
De seção|Section
Decrescente|Descending
Definir como objeto mestre|Set as master object
Depois do parágrafo|After paragraph
Desagrupar|Ungroup
Descrição da imagem|Image description
Desenhar com a caneta|Draw with the pen
Desfoque|Blur
Desk Slides · M Mix com a HIG|Desk Slides · M
Desk Slides · N iWork atual|Desk Slides · N
Desk Tables · M Mix com a HIG|Desk Tables · M
Desk Tables · N iWork atual|Desk Tables · N
Desk Writer · M Mix com a HIG|Desk Writer · M
Desk Writer · N iWork atual|Desk Writer · N
Desligada|Off
Deslocamento|Offset
Despesas|Expenses
Despesas fixas|Fixed expenses
Despesas fixas :: B1:E1|Fixed expenses :: B1:E1
Despesas fixas :: B8:E8|Fixed expenses :: B8:E8
Despesas por mês|Expenses by month
Destravar|Unlock
Diferenciar maiúsculas|Match case
Diminuir recuo|Decrease indent
Direita|Right
Direção|Direction
Dispersão|Scatter
Dissolver|Dissolve
Distribuir|Distribute
Distribuir objetos|Distribute objects
Do documento|From the document
Do slide mestre|From the slide layout
Documento|Document
Documentos|Documents
Documentos abertos|Open documents
Dois eixos|Two axes
Duas páginas|Two pages
Duplicar planilha|Duplicate sheet
Duração|Duration
Editar as referências de dados|Edit data references
Editar dados|Edit data
Editar máscara|Edit mask
Editar o slide mestre|Edit slide layout
Editor de fórmulas|Formula editor
Efeito|Effect
Eixo|Axis
Em branco|Blank
Em cima|On top
Em linha|Inline
Em repetição|Loop
Em volta|Around
Em “Despesas”|In “Expenses”
Embaixo|Below
Energia|Power
Ensaiar a apresentação|Rehearse Slideshow
Entrada|Build In
Entre linhas|Between lines
Entrega|Delivery
Enviar para trás|Send to back
Enviar por e-mail…|Send by email…
Enviar uma camada para trás|Send backward
Equipe de produto · março de 2027|Product team · March 2027
Escala|Scale
Escala do conteúdo|Content scale
Escolher arquivo…|Choose file…
Escolher tema…|Choose theme…
Escolher áudio…|Choose audio…
Escolher…|Choose…
Espanhol|Spanish
Espaçamento|Spacing
Espaçamento entre caracteres|Character spacing
Espaçamento entre linhas|Line spacing
Espaço entre colunas|Gap between columns
Espaço entre grupos|Gap between groups
Espaço extra|Extra space
Espelhar|Flip
Espelhar horizontalmente|Flip horizontally
Espelhar verticalmente|Flip vertically
Espessura|Width
Espessura do traço|Stroke width
Esquerda|Left
Estatísticas|Statistics
Este documento foi aberto com 3 avisos|This document was opened with 3 warnings
Estilo|Style
Estilo de caractere|Character style
Estilo de lista|List style
Estilo de parágrafo|Paragraph style
Estilo de parágrafo…|Paragraph style…
Estilo do traço|Stroke style
Estilo e cor|Style and color
Estilo seguinte|Following style
Estilo, espessura e cor…|Style, width and color…
Estilos|Styles
Estilos de objeto|Object styles
Estilos de tabela|Table styles
Evitar linhas viúvas e órfãs|Prevent widow and orphan lines
Exigir senha para abrir|Require password to open
Exigir senha para sair|Require password to exit
Exponencial|Exponential
Exportar|Export
Exposição|Exposure
Externas|Outside
Família|Family
Fechar|Close
Fev|Feb
Filmes|Movies
Filme…|Movie…
Filtrar|Filter
Filtro|Filter
Flutuante|Floating
Fluxo de caixa|Cash flow
Fonte|Font
Forma|Shape
Formatar|Format
Formato|Format
Formato condicional|Conditional highlighting
Formato de dados|Data format
Forte|Strong
Foto na horizontal|Photo, horizontal
Fotos|Photos
Fração|Fraction
Fundo|Background
Fundo da célula|Cell background
Fundo do slide|Slide background
Função|Function
Gaveta de estilos|Styles drawer
Geometria|Geometry
Girar|Rotate
Grade e fundo|Grid and background
Gradiente|Gradient
Gradiente avançado|Advanced gradient
Gravada|Recorded
Gravar a apresentação…|Record Slideshow…
Gravar…|Record…
Gravação|Recording
Grupo|Group
Grupo e trava|Group and lock
Gráfico|Chart
Gráfico (dados)|Chart (data)
Gráfico em 3D|3D chart
Gráficos|Charts
Hifenizar|Hyphenate
Hifenizar palavras|Hyphenate words
Hifenização|Hyphenation
Hifenização e idioma|Hyphenation and language
Horizontalmente|Horizontally
Idioma|Language
Igual aos dados|Same as data
Imagem|Image
Imagem tingida|Tinted image
Imagem, filme ou som…|Image, movie or sound…
Imagens…|Images…
Imposto|Tax
Impressão|Printing
Imprimir…|Print…
Inferior|Bottom
Informações|Information
Inglês (EUA)|English (US)
Iniciar|Start
Iniciar em|Start at
Iniciar transição|Start transition
Inserir|Insert
Inserir função|Insert function
Inserir quebra de parágrafo|Insert paragraph break
Inserir tabulação|Insert tab
Inspetor|Inspector
Interativo|Interactive
Internas|Inside
Intervalo|Range
Invisíveis|Invisibles
Início da seção|Section start
Ir para a célula…|Go to cell…
Ir para a página…|Go to page…
Ir para e navegar por|Go to and browse by
Ir para o slide|Go to slide
Ir para o slide…|Go to slide…
Itálico|Italic
Jornal|Newsprint
Justificar|Justify
Largura|Width
Largura da coluna|Column width
Largura igual|Equal width
Legenda|Legend
Ligaduras|Ligatures
Limpar a lista|Clear list
Linha|Line
Linha abaixo|Row below
Linha acima|Row above
Linha de base|Baseline
Linha de tendência|Trendline
Linhas|Rows
Linhas de cabeçalho|Header rows
Linhas de grade|Gridlines
Linhas de rodapé|Footer rows
Linhas e colunas|Rows and columns
Link…|Link…
Lista|List
Lista de fórmulas|Formula list
Livro-razão|Ledger
Logarítmica|Logarithmic
Lousa|Chalkboard
Maior|Larger
Mais|More
Mais ajustes|More adjustments
Mais casas decimais|More decimals
Mais formatos|More formats
Mais funções…|More functions…
Mais opções de formatação|More format options
Maiúsculas|Capitalization
Manter com o parágrafo seguinte|Keep with next paragraph
Manter linhas juntas|Keep lines together
Manter proporções|Constrain proportions
Marcador|Bookmark
Marcadores e listas|Bullets and lists
Marcas de escala|Tick marks
Margem|Margin
Margem interna|Inset margin
Margens|Margins
Mascarar com forma|Mask with shape
Material|Supplies
Meio|Middle
Melhorar|Enhance
Menor|Smaller
Menos casas decimais|Fewer decimals
Menu Formato|Format menu
Menu Inserir|Insert menu
Menu Visualização|View menu
Menu pop-up|Pop-up menu
Menu principal|Main menu
Mesa de luz|Light table
Mesclar células|Merge cells
Mesmo|Same
Mestres|Layouts
Meta|Target
Meta de 2027|2027 target
Meta: 4 mil pedidos no trimestre|Target: 4,000 orders in the quarter
Miniaturas das páginas|Page thumbnails
Misto|Mixed
Modo|Mode
Modo de tópicos|Outline mode
Moeda|Currency
Moldura|Frame
Moldura de imagem|Picture frame
Mostrar as linhas que correspondem|Show rows that match
Mostrar estatísticas|Show statistics
Mostrar mais…|Show more…
Mostrar no Inspetor|Show in Inspector
Mostrar o nome da tabela|Show table name
Mostrar regras…|Show rules…
Mostrar todos os resultados na barra lateral|Show all results in the sidebar
Mover|Move
Mover para dentro|Move in
Movimento mágico|Magic Move
MÁX|MAX
MÉDIA|AVERAGE
MÍN|MIN
Máscara|Mask
Máscara e transparência|Mask and transparency
Máximo|Maximum
Média|Average
Média móvel|Moving average
Métricas|Metrics
Mídia|Media
Mínimo|Minimum
Música|Music
Narração|Narration
Navegador|Navigator
Navegar por|Browse by
Negativos|Negatives
Negrito|Bold
Negrito itálico|Bold italic
Nenhum|None
Nenhuma|None
Nenhuma gravação|No recording
Nitidez|Sharpness
No documento e recentes|In the document and recent
No meio|In the middle
Nome|Name
Nome do arquivo|File name
Nordeste|Northeast
Nota de rodapé|Footnote
Notas|Notes
Notas de fim de documento|Document endnotes
Notas de fim de seção|Section endnotes
Notas de rodapé|Footnotes
Notas do apresentador|Presenter notes
Nova planilha|New sheet
Novo documento|New Document
Novo estilo a partir da seleção…|New style from selection…
Novo slide|New slide
Numerada|Numbered
Numeração|Numbering
Numeração começa em|Numbering starts at
Numeração de páginas|Page numbering
Número|Number
Número da página|Page number
Número do slide|Slide number
O custo|Logistics costs
O investimento em treinamento dobrou, e a rotatividade da equipe foi a menor desde a fundação. Os indicadores de satisfação, medidos a cada trimestre, ficaram acima de nove pontos em todas as unidades. |Investment in training doubled, and staff turnover was the lowest since the company was founded. Satisfaction scores, measured every quarter, stayed above nine points in every unit. 
Objeto|Object
Objetos|Objects
Ofício|Legal
Onde estamos|Where we are
Opacidade|Opacity
Opções|Options
Opções avançadas|Advanced options
Opções da apresentação|Slideshow options
Opções da busca|Find options
Ordem|Order
Ordem das composições|Build order
Ordem e alinhamento|Order and alignment
Ordenar|Sort
Ordenar agora|Sort now
Organizar|Arrange
Orientação|Orientation
Orçamento|Budget
Orçamento 2027|Budget 2027
Padrão|Default
Paginação e quebras|Pagination and breaks
Paisagem|Landscape
Palavras|Words
Palavras inteiras|Whole words
Palavras-chave|Keywords
Papel|Paper
Para a direita|To the right
Para a esquerda|To the left
Para o próximo ciclo, a prioridade é consolidar os canais digitais sem perder o atendimento de balcão, que continua sendo a origem de metade dos pedidos recorrentes. As metas por região estão na tabela da página seguinte. |For the next cycle, the priority is to consolidate the digital channels without losing counter service, which is still the source of half of the recurring orders. The targets by region are in the table on the next page. 
Parceiros locais para a entrega|Local partners for delivery
Parágrafo começa em nova página|Paragraph starts on a new page
Parágrafo seguinte|Next paragraph
Parágrafos|Paragraphs
Passo a passo|Step by step
Passos|Steps
Personalizada…|Custom…
Personalizado…|Custom…
Pessoas|People
Pizza|Pie
Planilha|Sheet
Planilha 2 de 4|Sheet 2 of 4
Planilha em branco|Blank sheet
Planilhas|Sheets
Plano de lançamento|Launch plan
Polinomial|Polynomial
Pontilhada|Dotted
Pontilhado|Dotted
Por grupo de marcadores|By bullet group
Por marcador|By bullet
Porcentagem|Percentage
Português (Brasil)|Portuguese (Brazil)
Posicionamento|Placement
Positiva e negativa|Positive and negative
Posição|Position
Posição e giro|Position and rotation
Potência|Power
Preenchimento|Fill
Preenchimento da célula|Cell fill
Preenchimento e borda|Fill and border
Preferências|Preferences
Preto|Black
Primeira linha|First line
Primeira página diferente|Different first page
Primeiro slide|First slide
Produto|Product
Proposta|Proposal
Proposta comercial|Sales proposal
Pré-visualizar|Preview
Próximos passos|Next steps
Pular este slide|Skip this slide
Página|Page
Página 6 de 41|Page 6 of 41
Página anterior|Previous page
Página direita|Right page
Página esquerda|Left page
Página seguinte|Next page
Páginas|Pages
Páginas do modelo|Template pages
Páginas esquerda e direita diferentes|Different left and right pages
Páginas opostas|Facing pages
Público e canais|Audience and channels
Qualquer página|Any page
Quebra|Break
Quebra de página|Page break
Quebra de seção|Section break
Quebra, campo ou nota…|Break, field or note…
Reajuste|Adjustment
Realizado|Actual
Reaplicar o mestre ao slide|Reapply layout to slide
Receitas|Income
Recuo|Indent
Recuos|Indents
Redefinir|Reset
Redefinir estilo a partir da seleção|Redefine style from selection
Redefinir geometria do texto|Reset text geometry
Redefinir máscara|Reset mask
Reduzir|Zoom out
Reflexo|Reflection
Região|Region
Reiniciar se ficar parada|Restart if idle
Relatório anual 2026|Annual report 2026
Remover hifenização do parágrafo|Remove paragraph hyphenation
Reorganizar|Organize
Repetir a apresentação|Loop slideshow
Reproduzir|Play
Reproduzir a partir deste slide|Play from This Slide
Reproduzir ao abrir|Play on open
Reproduzir do início|Play from Start
Reproduzir em janela|Play in Window
Reprodução|Playback
Reprodução automática|Self-playing
Reserva|Reserve
Resultados da busca|Search results
Resultados do ano|Results of the year
Resumo|Summary
Retrato|Portrait
Retângulo|Rectangle
Reunião de vendas|Sales meeting
Riscos|Risks
Rodapé|Footer
Régua|Ruler
Réguas|Rulers
Rótulos|Labels
Rótulos de valor|Value labels
Rótulos e linhas|Labels and lines
SOMA|SUM
Salvar|Save
Salvar como…|Save As…
Saturação|Saturation
Saída|Build Out
Seguinte|Next
Segurança|Security
Seleção|Selection
Sem grade|No grid
Separador de milhar|Thousands separator
Setas|Arrows
Seção|Section
Sistema numérico|Numeral system
Slide 3 de 12|Slide 3 of 12
Slide anterior|Previous slide
Slide mestre|Slide layout
Slide seguinte|Next slide
Slides M · Animação|Slides M · Animation
Slides M+ · objetos no cabeçalho|Slides M+
Slides N · Animar|Slides N · Animate
Slides mestres|Slide layouts
Sobre o |About 
Sobrescrito|Superscript
Soma|Sum
Sombra|Shadow
Somente links|Links only
Sublinhado|Underline
Subscrito|Subscript
Substituir|Replace
Substituir imagem…|Replace image…
Substituir por…|Replace with…
Substituir tudo|Replace All
Substituir…|Replace…
Subtítulo|Subtitle
Sudeste|Southeast
Sul|South
Sumário|Table of contents
Superior|Top
Séries|Series
Só na seleção|In selection only
Só negativa|Negative only
Só o slide|Slide only
Só o título|Title only
Só positiva|Positive only
Tabela|Table
Tabela dinâmica|Pivot table
Tabelas|Tables
Tabelas e gráficos|Tables and charts
Tables M+ · objetos no cabeçalho|Tables M+
Tabulações|Tabs
Tachado|Strikethrough
Tamanho|Size
Tamanho da fonte|Font size
Tamanho do slide|Slide size
Tamanho original|Original size
Tamanho real|Actual size
Taxas|Rates
Tela cheia|Fullscreen
Tela do apresentador|Presenter display
Tela menor que 1280 × 720 ‒ partes da janela podem não caber|Screen smaller than 1280 × 720 ‒ parts of the window may not fit
Tema|Theme
Tema da apresentação|Presentation theme
Temas|Themes
Temperatura|Temperature
Texto|Text
Texto do documento|Document text
Texto na célula|Text in cell
Texto oculto de tabelas|Hidden table text
Texto simples…|Plain text…
Tipo|Type
Tipo de gráfico|Chart type
Tipo, espaçamento e ligaduras no Inspetor|Typeface, spacing and ligatures in the Inspector
Todas|All
Todas maiúsculas|All caps
Tornar editável|Make editable
Tornar objetos mestres selecionáveis|Make master objects selectable
Total de páginas|Page count
Tracejada|Dashed
Tracejado|Dashed
Transição|Transition
Transporte|Transport
Travar|Lock
Travar o objeto|Lock object
Trazer para a frente|Bring to front
Trazer uma camada para a frente|Bring forward
Traço|Stroke
Trilha sonora|Soundtrack
Trocar de tema…|Change theme…
Três capitais na primeira onda|Three capitals in the first wave
Tudo de uma vez|All at once
Título|Title
Título e marcadores|Title and bullets
Título e subtítulo|Title and subtitle
Título, marcadores e foto|Title, bullets and photo
Tópicos|Outline
Uma página|One page
Uma vez|Once
Usar ligaduras|Use ligatures
Usar os da seção anterior|Use those of the previous section
Valor (Y)|Value (Y)
Valores|Values
Variação|Change
Vendas, pessoas e metas para 2027|Sales, people and targets for 2027
Ver avisos|Show warnings
Versalete|Small caps
Verticalmente|Vertically
Virar|Flip
Visualizar|View
Visualização|View
Visualização de impressão|Print preview
Visualização web|Web view
Writer M · estilo escuro|Writer M · dark style
Writer M · imagem selecionada|Writer M · image selected
Writer M+ · objetos no cabeçalho|Writer M+
Writer N · estilo escuro|Writer N · dark style
Writer N · imagem selecionada|Writer N · image selected
À direita|On the right
À esquerda|On the left
Área|Area
Áudio|Audio
Ângulo|Angle
Ênfase|Emphasis
Último slide|Last slide
é maior que 1.000|is greater than 1,000
Mar|Mar
Jan|Jan
transportadora|carrier
2 de 4|2 of 4
2 de 7|2 of 7
 mil|K
 %|%
R$|$
"""
EN = dict(line.split('|', 1) for line in _RAW.strip('\n').split('\n'))
# The formula being edited in a cell, in Pango markup.
_FORMULA = ('= <span foreground="#1c71d8">SOMA</span>(<span background="#f9f06b">B2:E2</span>)'
            ' × <span background="#8ff0a4">Reajuste</span>')
EN[_FORMULA] = _FORMULA.replace('SOMA', 'SUM').replace('Reajuste', 'Adjustment')


# A number written the Brazilian way: 2.400, 3.474,83, +11,7 %, 125 %.
_NUMBER = re.compile(r'^[+-]?(\d{1,3}(\.\d{3})+|\d+)(,\d+)?( %)?$')


def _number(text):
    """The number in the English notation, or None when text is not one."""
    if not _NUMBER.match(text) or text.isdigit():
        return None
    swapped = text.translate(str.maketrans('.,', ',.'))
    return swapped.replace(' %', '%')


class _Translate(ast.NodeTransformer):
    """Replaces each string constant that EN knows by its translation."""

    def visit_Call(self, node):
        # "Tipo" is "Type" everywhere but beside the list of font faces.
        strings = {n.value for n in ast.walk(node)
                   if isinstance(n, ast.Constant) and isinstance(n.value, str)}
        if {'Tipo', 'Regular'} <= strings and len(strings) < 12:
            for n in ast.walk(node):
                if isinstance(n, ast.Constant) and n.value == 'Tipo':
                    n.value = 'Typeface'
        return self.generic_visit(node)

    def visit_Constant(self, node):
        if isinstance(node.value, str):
            text = EN.get(node.value) or _number(node.value)
            if text:
                return ast.copy_location(ast.Constant(text), node)
        return node


class _Loader(importlib.machinery.SourceFileLoader):
    def get_code(self, fullname):
        # Always from the source: a cached .pyc would hold the Portuguese text.
        path = self.get_filename(fullname)
        return self.source_to_code(self.get_data(path), path)

    def source_to_code(self, data, path, *, _optimize=-1):
        tree = _Translate().visit(ast.parse(data, path))
        return compile(ast.fix_missing_locations(tree), path, 'exec', dont_inherit=True,
                       optimize=_optimize)


class _Finder(importlib.abc.MetaPathFinder):
    def find_spec(self, name, path=None, target=None):
        if name not in MODULES:
            return None
        file = os.path.join(HERE, name + '.py')
        return importlib.machinery.ModuleSpec(name, _Loader(name, file), origin=file)


def install():
    """Makes the scene modules load translated. Call before importing them."""
    sys.dont_write_bytecode = True      # the cache would hold the other language
    sys.meta_path.insert(0, _Finder())
