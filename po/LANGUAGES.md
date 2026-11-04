<!--
SPDX-FileCopyrightText: 2026 Desk contributors
SPDX-License-Identifier: GPL-3.0-or-later
-->

# Languages

Desk will be translated into every language that GNOME supports and
that has a spelling dictionary Desk can use. Until then the interface
stays in English and Brazilian Portuguese: each app is translated into
the other languages only when it has all of its features, first Writer,
then Tables and Slides.

## Which languages GNOME supports

GNOME keeps no formal list. A language counts as supported when at
least 80% of the interface strings of a release are translated, the
rule of its release notes and of [Damned Lies](https://l10n.gnome.org/).
The GNOME translation team counted 46 such languages in GNOME 51, and 16
of them at 100%.

The percentages below come from the translations in the release tarballs
of 53 GNOME 51 modules (the shell, the core libraries and the core apps),
as packaged by Ubuntu, because Damned Lies does not list its numbers by
language in one place. They put 49 languages above 80%: the last three
(Finnish, Punjabi and Norwegian Nynorsk) are near the line and may be
under it on Damned Lies. Check its GNOME release page again before each
translation round.

## Languages that Desk will support

Each of these has a Hunspell dictionary that Desk reads, bundled with
the Flatpak of Writer from the Ubuntu 26.04 packages (26.10 for Persian
and Punjabi). Hyphenation says whether there are hyphenation patterns
too.

| Code | Language | GNOME 51 | Dictionary | Hyphenation |
|---|---|---:|---|---|
| `sl` | Slovenian | 100% | `sl_SI` | yes |
| `sv` | Swedish | 100% | `sv_SE`, `sv_FI` | yes |
| `cs` | Czech | 100% | `cs_CZ` | yes |
| `uk` | Ukrainian | 100% | `uk_UA` | yes |
| `hu` | Hungarian | 99.9% | `hu_HU` | yes |
| `fr` | French | 99.9% | `fr` | yes |
| `bg` | Bulgarian | 99.9% | `bg_BG` | yes |
| `ru` | Russian | 99.8% | `ru_RU` | yes |
| `lt` | Lithuanian | 99.5% | `lt_LT` | yes |
| `eu` | Basque | 99.5% | `eu` | no |
| `sr` | Serbian (Cyrillic) | 99.5% | `sr_RS`, `sr_ME` | yes |
| `en_GB` | English (UK) | 99.3% | `en_GB`, `en_US` | yes |
| `pl` | Polish | 99.1% | `pl_PL` | yes |
| `kk` | Kazakh | 98.9% | `kk_KZ` | no |
| `es` | Spanish | 98.7% | `es_ES` and 19 regions | yes |
| `ca` | Catalan | 98.7% | `ca`, `ca_ES-valencia` | yes |
| `it` | Italian | 98.0% | `it_IT`, `it_CH` | yes |
| `pt_BR` | Portuguese (Brazil) | 97.9% | `pt_BR` | yes |
| `de` | German | 97.8% | `de_DE` | yes |
| `eo` | Esperanto | 97.7% | `eo` | yes |
| `gl` | Galician | 97.6% | `gl_ES` | yes |
| `pt` | Portuguese | 97.5% | `pt_PT` | yes |
| `id` | Indonesian | 97.3% | `id_ID` | yes |
| `el` | Greek | 96.8% | `el_GR` | yes |
| `sr@latin` | Serbian (Latin) | 96.4% | `sr_Latn_RS` | yes |
| `ro` | Romanian | 96.2% | `ro_RO` | yes |
| `nl` | Dutch | 95.5% | `nl` | yes |
| `sk` | Slovak | 95.4% | `sk_SK` | yes |
| `fa` | Persian | 94.5% | `fa-IR` | no |
| `oc` | Occitan | 94.5% | `oc_FR` | no |
| `lv` | Latvian | 93.6% | `lv_LV` | yes |
| `be` | Belarusian | 92.0% | `be_BY` | yes |
| `he` | Hebrew | 91.6% | `he_IL` | no |
| `nb` | Norwegian Bokmål | 91.0% | `nb_NO` | yes |
| `hi` | Hindi | 89.0% | `hi_IN` | no |
| `ko` | Korean | 88.9% | `ko` | no |
| `hr` | Croatian | 87.6% | `hr_HR` | yes |
| `pa` | Punjabi | 83.6% | `pa_IN` | no |
| `nn` | Norwegian Nynorsk | 81.6% | `nn_NO` | yes |

## Languages left out for now

- **Danish** (`da`, 100%) and **Turkish** (`tr`, 100%): their
  dictionaries exist, but Spellbook, the Hunspell reader of Desk, rejects
  them (Danish has a word with a bare `/`, Turkish uses flag 0). They
  join once Spellbook or Desk reads them.
- **Thai** (`th`, 90.5%): the dictionary exists, but Thai writes no
  spaces between words, and Desk splits words at spaces and punctuation.
  It needs a word breaker for Thai first.
- **Finnish** (`fi`, 85.5%): it is spelled with Voikko, not Hunspell.
- **Chinese** (`zh_CN`, `zh_TW`), **Japanese** (`ja`), **Uyghur**
  (`ug`), **Georgian** (`ka`) and **Friulian** (`fur`): there is no
  Hunspell dictionary for them in Ubuntu, and spelling dictionaries do not
  apply to Chinese and Japanese.
