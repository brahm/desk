// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Cell formats: how a value is written (TAB-28 to TAB-30), and how typed
//! text becomes a value (TAB-18).

use std::sync::Arc;

use desk_formula::{Value, date_from_days, days_from_date};

use crate::cell::{Content, Num};

/// How negative numbers are written.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum Negative {
    /// With a minus sign: -100.
    #[default]
    Minus,
    /// In red: 100.
    Red,
    /// Between parentheses: (100).
    Parentheses,
    /// In red, between parentheses.
    RedParentheses,
}

/// How a date is written.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum DateStyle {
    /// Not written.
    None,
    /// 06/01/2009.
    #[default]
    Short,
    /// 6 de jan. de 2009.
    Medium,
    /// 6 de janeiro de 2009.
    Long,
    /// terça-feira, 6 de janeiro de 2009.
    Full,
    /// 2009-01-06.
    Iso,
}

/// How a time of day is written.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum TimeStyle {
    /// Not written.
    #[default]
    None,
    /// 13:30.
    Short,
    /// 13:30:05.
    Seconds,
}

/// How a duration is written.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum DurationStyle {
    /// 1sem 2d 3h 4min 5s, with the units that are not zero.
    #[default]
    Compact,
    /// 1 semana 2 dias 3 horas.
    Full,
    /// 51:04:05, as hours, minutes and seconds.
    Clock,
}

/// How exact a fraction is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum FractionPrecision {
    /// The closest fraction with a denominator of up to this many digits,
    /// from 1 to 3.
    UpTo(u8),
    /// In halves, quarters, eighths, sixteenths, tenths or hundredths.
    Exactly(u16),
}

impl Default for FractionPrecision {
    fn default() -> Self {
        Self::UpTo(1)
    }
}

/// A format of numbers, dates or text that a person made, with a name
/// (TAB-29 to TAB-31).
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct CustomFormat {
    /// The name that the format menu shows.
    pub name: String,
    /// The pattern, in the syntax of spreadsheets: `#,##0.00 "kg"` for
    /// numbers, with sections for negatives and zero after semicolons and
    /// conditions such as `[>1000]`; `dd/MM/yyyy HH:mm` for dates; and
    /// `@` for the text of a text format.
    pub pattern: Arc<str>,
}

/// How a cell writes its value (TAB-28).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Format {
    /// As it was typed: numbers in their shortest form, and dates,
    /// percentages and currency as they were entered.
    #[default]
    Automatic,
    /// A number.
    Number {
        /// The decimal places; none for as many as the number has.
        decimals: Option<u8>,
        /// Whether thousands are separated.
        thousands: bool,
        /// How negatives are written.
        negative: Negative,
    },
    /// Money.
    Currency {
        /// The symbol, such as `R$` or `US$`.
        symbol: String,
        /// The decimal places.
        decimals: u8,
        /// Whether thousands are separated.
        thousands: bool,
        /// How negatives are written.
        negative: Negative,
        /// Whether the symbol sits at the left edge of the cell, as in
        /// accounting.
        accounting: bool,
    },
    /// A percentage: 0.03 is written 3%.
    Percent {
        /// The decimal places; none for as many as the number has.
        decimals: Option<u8>,
        /// Whether thousands are separated.
        thousands: bool,
        /// How negatives are written.
        negative: Negative,
    },
    /// A date and time.
    DateTime {
        /// How the date is written.
        date: DateStyle,
        /// How the time is written.
        time: TimeStyle,
    },
    /// A length of time.
    Duration {
        /// How it is written.
        style: DurationStyle,
    },
    /// A fraction: 0.5 is written 1/2.
    Fraction {
        /// How exact it is.
        precision: FractionPrecision,
    },
    /// A number in a base from 2 to 36 (Sistema Numérico).
    Base {
        /// The base.
        base: u8,
        /// The least digits written, filled with zeros.
        places: u8,
        /// Whether negatives are written as two's complement.
        twos_complement: bool,
    },
    /// Scientific notation: 1.23E+04.
    Scientific {
        /// The decimal places.
        decimals: u8,
    },
    /// Text: what is typed stays text, even a number.
    Text,
    /// A format that a person made (TAB-29, TAB-30).
    Custom(CustomFormat),
}

/// How numbers and dates are written: those of the document, or where the
/// app runs (PXV-09). The language gives the words, as the names of the
/// months, and where the currency symbol goes; the region gives the
/// separators, the order of the short date and the currency.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Locale {
    /// The decimal separator.
    pub decimal: char,
    /// The thousands separator.
    pub group: char,
    /// The currency symbol of new currency cells.
    pub currency: String,
    /// Whether dates are written day first, as 06/01/2009, rather than
    /// month first.
    pub day_first: bool,
    /// What separates the day, the month and the year of a short date.
    pub date_separator: char,
    /// The months, from January.
    pub months: [&'static str; 12],
    /// The days of the week, from Sunday.
    pub weekdays: [&'static str; 7],
    /// The word between the parts of a long date, as "de"; empty for an
    /// English date.
    pub of: &'static str,
    /// Whether the words are Portuguese, rather than English.
    pub portuguese: bool,
}

/// The languages whose words Desk writes dates and numbers in, as their
/// BCP 47 tags, with the region each takes when none is given (PXV-09).
pub const LANGUAGES: [(&str, &str); 2] = [("en", "US"), ("pt", "BR")];

/// The regions whose separators, short dates and currencies Desk knows,
/// as ISO 3166 codes: decimal separator, thousands separator, whether the
/// day comes first, the separator of the short date, and the currency
/// symbol (PXV-09).
pub const REGIONS: [(&str, char, char, bool, char, &str); 13] = [
    ("AR", ',', '.', true, '/', "$"),
    ("AU", '.', ',', true, '/', "$"),
    ("BR", ',', '.', true, '/', "R$"),
    ("CA", '.', ',', false, '/', "$"),
    ("DE", ',', '.', true, '.', "€"),
    ("ES", ',', '.', true, '/', "€"),
    ("FR", ',', '\u{202f}', true, '/', "€"),
    ("GB", '.', ',', true, '/', "£"),
    ("IT", ',', '.', true, '/', "€"),
    ("MX", '.', ',', true, '/', "$"),
    ("PT", ',', '.', true, '/', "€"),
    ("US", '.', ',', false, '/', "$"),
    ("ZA", ',', '\u{a0}', true, '/', "R"),
];

impl Locale {
    /// Brazilian Portuguese.
    pub fn pt_br() -> Self {
        Self::of("pt", Some("BR"))
    }

    /// American English.
    pub fn en_us() -> Self {
        Self::of("en", Some("US"))
    }

    /// The words of a language and the numbers and dates of a region. A
    /// language Desk has no words for writes English; a region Desk does
    /// not know, or none, takes the one of the language.
    pub fn of(language: &str, region: Option<&str>) -> Self {
        let portuguese = language.eq_ignore_ascii_case("pt");
        let fallback = if portuguese { "BR" } else { "US" };
        let region = region
            .and_then(|r| {
                REGIONS
                    .iter()
                    .find(|(code, ..)| code.eq_ignore_ascii_case(r))
            })
            .or_else(|| REGIONS.iter().find(|(code, ..)| *code == fallback))
            .copied()
            .unwrap_or(("US", '.', ',', false, '/', "$"));
        let (_, decimal, group, day_first, date_separator, currency) = region;
        let (months, weekdays, of) = if portuguese {
            (
                [
                    "janeiro",
                    "fevereiro",
                    "março",
                    "abril",
                    "maio",
                    "junho",
                    "julho",
                    "agosto",
                    "setembro",
                    "outubro",
                    "novembro",
                    "dezembro",
                ],
                [
                    "domingo",
                    "segunda-feira",
                    "terça-feira",
                    "quarta-feira",
                    "quinta-feira",
                    "sexta-feira",
                    "sábado",
                ],
                "de",
            )
        } else {
            (
                [
                    "January",
                    "February",
                    "March",
                    "April",
                    "May",
                    "June",
                    "July",
                    "August",
                    "September",
                    "October",
                    "November",
                    "December",
                ],
                [
                    "Sunday",
                    "Monday",
                    "Tuesday",
                    "Wednesday",
                    "Thursday",
                    "Friday",
                    "Saturday",
                ],
                "",
            )
        };
        Self {
            decimal,
            group,
            currency: currency.to_owned(),
            day_first,
            date_separator,
            months,
            weekdays,
            of,
            portuguese,
        }
    }

    /// A date and time, in days from 30 December 1899, written in a
    /// pattern of the symbols of Unicode (LDML) such as `d MMMM y`, as the
    /// date fields of the text are (EST-27, PXV-09). Unlike the patterns of
    /// cells, a lone `y` is the whole year.
    pub fn date(&self, days: f64, pattern: &str) -> String {
        let chars: Vec<char> = pattern.chars().collect();
        let mut out = String::new();
        // The symbols since the last quoted text.
        let mut symbols = String::new();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            if c == '\'' {
                out += &custom_date(days, &std::mem::take(&mut symbols), self);
                // Quoted text, where '' is a quote, inside quotes or out.
                if chars.get(i + 1) == Some(&'\'') {
                    out.push('\'');
                    i += 2;
                    continue;
                }
                let mut j = i + 1;
                while j < chars.len() {
                    if chars[j] == '\'' {
                        if chars.get(j + 1) == Some(&'\'') {
                            out.push('\'');
                            j += 2;
                            continue;
                        }
                        break;
                    }
                    out.push(chars[j]);
                    j += 1;
                }
                i = j + 1;
                continue;
            }
            let lone =
                c == 'y' && (i == 0 || chars[i - 1] != 'y') && chars.get(i + 1) != Some(&'y');
            if lone {
                symbols.push_str("yyyy");
            } else {
                symbols.push(c);
            }
            i += 1;
        }
        out += &custom_date(days, &symbols, self);
        out
    }

    /// The locale of a tag such as `pt-BR`, `en-GB` or `pt_BR.UTF-8`: its
    /// language and its region.
    pub fn from_tag(tag: &str) -> Self {
        let tag = tag.split(['.', '@']).next().unwrap_or_default();
        let mut parts = tag.split(['-', '_']);
        let language = parts.next().unwrap_or_default();
        // The region is the part of two letters or three digits, after a
        // script of four letters if there is one.
        let region = parts.find(|p| {
            (p.len() == 2 && p.chars().all(|c| c.is_ascii_alphabetic()))
                || (p.len() == 3 && p.chars().all(|c| c.is_ascii_digit()))
        });
        Self::of(language, region)
    }
}

impl Default for Locale {
    fn default() -> Self {
        Self::en_us()
    }
}

/// A value written as a cell shows it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Shown {
    /// The text.
    pub text: String,
    /// Whether it is red, as a negative number of some formats.
    pub red: bool,
    /// Whether it is a number, which sits on the right of the cell when
    /// the alignment is automatic.
    pub numeric: bool,
    /// The currency symbol that sits at the left edge, in the accounting
    /// style.
    pub left_symbol: Option<String>,
}

fn digits_with_groups(int_part: &str, group: Option<char>) -> String {
    let Some(g) = group else {
        return int_part.to_owned();
    };
    let mut out = String::new();
    let n = int_part.len();
    for (i, c) in int_part.chars().enumerate() {
        if i > 0 && (n - i).is_multiple_of(3) {
            out.push(g);
        }
        out.push(c);
    }
    out
}

/// A number with a fixed or the shortest number of decimals, without its
/// sign.
fn plain_number(n: f64, decimals: Option<u8>, thousands: bool, locale: &Locale) -> String {
    let n = n.abs();
    let s = match decimals {
        Some(d) => format!("{:.*}", usize::from(d), n),
        None => {
            // Up to 10 decimals, as iWork rounds long decimals (TAB-18).
            let s = format!("{:.10}", n);
            let s = s.trim_end_matches('0').trim_end_matches('.').to_owned();
            if n >= 1e15 {
                return scientific(n, 5, locale);
            }
            s
        }
    };
    let (int_part, frac) = s
        .split_once('.')
        .map_or((s.as_str(), None), |(a, b)| (a, Some(b)));
    let mut out = digits_with_groups(int_part, thousands.then_some(locale.group));
    if let Some(f) = frac {
        out.push(locale.decimal);
        out.push_str(f);
    }
    out
}

fn scientific(n: f64, decimals: u8, locale: &Locale) -> String {
    let s = format!("{:.*E}", usize::from(decimals), n);
    let (mantissa, exp) = s.split_once('E').unwrap_or((&s, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let mantissa = mantissa.replace('.', &locale.decimal.to_string());
    format!(
        "{mantissa}E{}{:02}",
        if exp < 0 { '-' } else { '+' },
        exp.abs()
    )
}

fn signed(body: String, negative: bool, style: Negative) -> (String, bool) {
    if !negative {
        return (body, false);
    }
    match style {
        Negative::Minus => (format!("-{body}"), false),
        Negative::Red => (body, true),
        Negative::Parentheses => (format!("({body})"), false),
        Negative::RedParentheses => (format!("({body})"), true),
    }
}

fn fraction(n: f64, precision: FractionPrecision) -> String {
    let negative = n < 0.0;
    let n = n.abs();
    let whole = n.trunc();
    let frac = n - whole;
    let (num, den) = match precision {
        FractionPrecision::Exactly(den) => ((frac * f64::from(den)).round() as u64, u64::from(den)),
        FractionPrecision::UpTo(digits) => {
            let max = 10u64.pow(u32::from(digits.clamp(1, 3))) - 1;
            let mut best = (0, 1, f64::INFINITY);
            for den in 1..=max {
                let num = (frac * den as f64).round() as u64;
                let err = (frac - num as f64 / den as f64).abs();
                if err < best.2 - 1e-12 {
                    best = (num, den, err);
                }
            }
            (best.0, best.1)
        }
    };
    let (mut whole, mut num) = (whole as u64, num);
    if num == den {
        whole += 1;
        num = 0;
    }
    let g = gcd(num, den).max(1);
    let sign = if negative { "-" } else { "" };
    match (whole, num) {
        (w, 0) => format!("{sign}{w}"),
        (0, n) => format!("{sign}{}/{}", n / g, den / g),
        (w, n) => format!("{sign}{w} {}/{}", n / g, den / g),
    }
}

fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 { a } else { gcd(b, a % b) }
}

fn in_base(n: f64, base: u8, places: u8, twos: bool) -> String {
    let base = u32::from(base.clamp(2, 36));
    let mut v = n.trunc() as i64;
    let negative = v < 0;
    if negative && twos {
        // Two's complement in 32 bits, as iWork writes it.
        v = i64::from(v as i32 as u32);
    }
    let mut u = v.unsigned_abs();
    let mut digits = Vec::new();
    while u > 0 {
        digits.push(std::char::from_digit((u % u64::from(base)) as u32, base).unwrap_or('0'));
        u /= u64::from(base);
    }
    while digits.len() < usize::from(places.max(1)) {
        digits.push('0');
    }
    let s: String = digits.iter().rev().collect::<String>().to_uppercase();
    if negative && !twos {
        format!("-{s}")
    } else {
        s
    }
}

fn date_text(days: f64, date: DateStyle, time: TimeStyle, locale: &Locale) -> String {
    let day = days.floor();
    let (y, m, d) = date_from_days(day as i64);
    let month = locale.months[(m as usize).saturating_sub(1).min(11)];
    let weekday = locale.weekdays[((day as i64 + 6).rem_euclid(7)) as usize];
    let short_month: String = month.chars().take(3).collect();
    let date_part = match date {
        DateStyle::None => String::new(),
        DateStyle::Short if locale.day_first => {
            format!("{d:02}{sep}{m:02}{sep}{y:04}", sep = locale.date_separator)
        }
        DateStyle::Short => format!("{m}{sep}{d}{sep}{y:04}", sep = locale.date_separator),
        DateStyle::Medium if locale.of.is_empty() => format!("{short_month} {d}, {y}"),
        DateStyle::Medium => format!("{d} {of} {short_month}. {of} {y}", of = locale.of),
        DateStyle::Long if locale.of.is_empty() => format!("{month} {d}, {y}"),
        DateStyle::Long => format!("{d} {of} {month} {of} {y}", of = locale.of),
        DateStyle::Full if locale.of.is_empty() => format!("{weekday}, {month} {d}, {y}"),
        DateStyle::Full => format!("{weekday}, {d} {of} {month} {of} {y}", of = locale.of),
        DateStyle::Iso => format!("{y:04}-{m:02}-{d:02}"),
    };
    let seconds = ((days - day) * 86_400.0).round() as i64;
    let (h, mi, s) = (seconds / 3600 % 24, seconds / 60 % 60, seconds % 60);
    let time_part = match time {
        TimeStyle::None => String::new(),
        TimeStyle::Short => format!("{h:02}:{mi:02}"),
        TimeStyle::Seconds => format!("{h:02}:{mi:02}:{s:02}"),
    };
    match (date_part.is_empty(), time_part.is_empty()) {
        (false, false) => format!("{date_part} {time_part}"),
        (true, _) => time_part,
        (_, true) => date_part,
    }
}

fn duration_text(days: f64, style: DurationStyle, pt: bool) -> String {
    let negative = days < 0.0;
    let total = (days.abs() * 86_400.0).round() as i64;
    let sign = if negative { "-" } else { "" };
    if style == DurationStyle::Clock {
        return format!(
            "{sign}{}:{:02}:{:02}",
            total / 3600,
            total / 60 % 60,
            total % 60
        );
    }
    let parts = [
        (
            total / 604_800,
            "sem",
            "semana",
            "semanas",
            "w",
            "week",
            "weeks",
        ),
        (total / 86_400 % 7, "d", "dia", "dias", "d", "day", "days"),
        (
            total / 3600 % 24,
            "h",
            "hora",
            "horas",
            "h",
            "hour",
            "hours",
        ),
        (
            total / 60 % 60,
            "min",
            "minuto",
            "minutos",
            "m",
            "minute",
            "minutes",
        ),
        (
            total % 60,
            "s",
            "segundo",
            "segundos",
            "s",
            "second",
            "seconds",
        ),
    ];
    let mut out = Vec::new();
    for (n, pt_short, pt_one, pt_many, en_short, en_one, en_many) in parts {
        if n == 0 {
            continue;
        }
        out.push(match (style, pt) {
            (DurationStyle::Compact, true) => format!("{n}{pt_short}"),
            (DurationStyle::Compact, false) => format!("{n}{en_short}"),
            (_, true) => format!("{n} {}", if n == 1 { pt_one } else { pt_many }),
            (_, false) => format!("{n} {}", if n == 1 { en_one } else { en_many }),
        });
    }
    if out.is_empty() {
        return "0s".into();
    }
    format!("{sign}{}", out.join(" "))
}

/// A number written by a custom pattern (TAB-29).
fn custom_number(n: f64, pattern: &str, locale: &Locale) -> (String, bool) {
    let sections: Vec<&str> = split_sections(pattern);
    // Conditions such as [>1000] pick a section; otherwise the second
    // section is for negatives and the third for zero.
    let mut chosen = None;
    for s in &sections {
        if let Some(rest) = s.strip_prefix('[')
            && let Some((cond, body)) = rest.split_once(']')
            && let Some(ok) = condition(cond, n)
        {
            if ok {
                chosen = Some((body, false));
                break;
            }
            continue;
        }
    }
    let (section, abs) = chosen.unwrap_or_else(|| {
        let plain: Vec<&str> = sections
            .iter()
            .copied()
            .filter(|s| !has_condition(s))
            .collect();
        match (plain.len(), n) {
            (0, _) => ("General", false),
            (3.., 0.0) => (plain[2], true),
            (2.., x) if x < 0.0 => (plain[1], true),
            _ => (plain[0], false),
        }
    });
    let red = section.contains("[Red]") || section.contains("[Vermelho]");
    let section = section.replace("[Red]", "").replace("[Vermelho]", "");
    let value = if abs { n.abs() } else { n };
    (number_by_pattern(value, &section, locale), red)
}

fn split_sections(pattern: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut quoted = false;
    for (i, c) in pattern.char_indices() {
        match c {
            '"' => quoted = !quoted,
            ';' if !quoted => {
                out.push(&pattern[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(&pattern[start..]);
    out
}

fn has_condition(section: &str) -> bool {
    section
        .strip_prefix('[')
        .and_then(|rest| rest.split_once(']'))
        .is_some_and(|(cond, _)| condition(cond, 0.0).is_some())
}

fn condition(cond: &str, n: f64) -> Option<bool> {
    let ops = [">=", "<=", "<>", ">", "<", "="];
    let op = ops.iter().find(|op| cond.starts_with(**op))?;
    let v: f64 = cond[op.len()..].trim().replace(',', ".").parse().ok()?;
    Some(match *op {
        ">=" => n >= v,
        "<=" => n <= v,
        "<>" => n != v,
        ">" => n > v,
        "<" => n < v,
        _ => n == v,
    })
}

fn number_by_pattern(n: f64, pattern: &str, locale: &Locale) -> String {
    if pattern.trim().eq_ignore_ascii_case("general") {
        return plain_number(n, None, false, locale);
    }
    // Literal text before and after the digits.
    let mut prefix = String::new();
    let mut suffix = String::new();
    let mut digits = String::new();
    let mut chars = pattern.chars().peekable();
    let mut seen_digits = false;
    while let Some(c) = chars.next() {
        let lit = |s: &mut String, prefix: &mut String, suffix: &mut String, seen: bool| {
            if seen {
                suffix.push_str(s);
            } else {
                prefix.push_str(s);
            }
        };
        match c {
            '"' => {
                let mut s = String::new();
                for d in chars.by_ref() {
                    if d == '"' {
                        break;
                    }
                    s.push(d);
                }
                lit(&mut s, &mut prefix, &mut suffix, seen_digits);
            }
            '\\' => {
                let mut s = chars.next().map(String::from).unwrap_or_default();
                lit(&mut s, &mut prefix, &mut suffix, seen_digits);
            }
            '#' | '0' | '?' | ',' | '.' if !seen_digits || suffix.is_empty() => {
                seen_digits = true;
                digits.push(c);
            }
            c => {
                let mut s = c.to_string();
                lit(&mut s, &mut prefix, &mut suffix, seen_digits);
            }
        }
    }
    if digits.is_empty() {
        return format!("{prefix}{suffix}");
    }
    let mut value = n;
    if prefix.contains('%') || suffix.contains('%') {
        value *= 100.0;
    }
    // Commas after the last digit scale by thousands (TAB-29).
    let trailing = digits.len() - digits.trim_end_matches(',').len();
    let digits = digits.trim_end_matches(',');
    value /= 1000f64.powi(trailing as i32);
    let (int_pat, frac_pat) = digits.split_once('.').unwrap_or((digits, ""));
    let decimals = frac_pat
        .chars()
        .filter(|c| matches!(c, '0' | '#' | '?'))
        .count();
    let min_decimals = frac_pat.chars().filter(|c| *c == '0').count();
    let grouped = int_pat.contains(',');
    let min_int = int_pat.chars().filter(|c| *c == '0').count();
    let s = format!("{:.*}", decimals, value.abs());
    let (ip, fp) = s.split_once('.').unwrap_or((&s, ""));
    let mut fp = fp.to_owned();
    while fp.len() > min_decimals && fp.ends_with('0') {
        fp.pop();
    }
    let mut ip = ip.trim_start_matches('0').to_owned();
    while ip.len() < min_int {
        ip.insert(0, '0');
    }
    let mut out = digits_with_groups(&ip, grouped.then_some(locale.group));
    if !fp.is_empty() {
        out.push(locale.decimal);
        out.push_str(&fp);
    }
    let sign = if value < 0.0 && !out.chars().all(|c| c == '0' || c == locale.decimal) {
        "-"
    } else {
        ""
    };
    format!("{prefix}{sign}{out}{suffix}")
}

/// A date written by a custom pattern such as `dd/MM/yyyy HH:mm`
/// (TAB-30).
fn custom_date(days: f64, pattern: &str, locale: &Locale) -> String {
    let day = days.floor();
    let (y, m, d) = date_from_days(day as i64);
    let seconds = ((days - day) * 86_400.0).round() as i64;
    let (h, mi, s) = (seconds / 3600 % 24, seconds / 60 % 60, seconds % 60);
    let weekday = locale.weekdays[((day as i64 + 6).rem_euclid(7)) as usize];
    let month = locale.months[(m as usize).saturating_sub(1).min(11)];
    let mut out = String::new();
    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let mut n = 1;
        while i + n < chars.len() && chars[i + n] == c {
            n += 1;
        }
        match c {
            'y' => {
                out += &if n <= 2 {
                    format!("{:02}", y % 100)
                } else {
                    format!("{y:04}")
                }
            }
            'M' => {
                out += &match n {
                    1 => m.to_string(),
                    2 => format!("{m:02}"),
                    3 => month.chars().take(3).collect(),
                    _ => month.to_owned(),
                }
            }
            'd' => {
                out += &if n == 1 {
                    d.to_string()
                } else {
                    format!("{d:02}")
                }
            }
            'E' => {
                out += &if n <= 3 {
                    weekday.chars().take(3).collect()
                } else {
                    weekday.to_owned()
                }
            }
            'H' => {
                out += &if n == 1 {
                    h.to_string()
                } else {
                    format!("{h:02}")
                }
            }
            'h' => {
                let h12 = if h % 12 == 0 { 12 } else { h % 12 };
                out += &if n == 1 {
                    h12.to_string()
                } else {
                    format!("{h12:02}")
                }
            }
            'm' => {
                out += &if n == 1 {
                    mi.to_string()
                } else {
                    format!("{mi:02}")
                }
            }
            's' => {
                out += &if n == 1 {
                    s.to_string()
                } else {
                    format!("{s:02}")
                }
            }
            'a' => out += if h < 12 { "AM" } else { "PM" },
            '\'' => {
                let mut j = i + 1;
                while j < chars.len() && chars[j] != '\'' {
                    out.push(chars[j]);
                    j += 1;
                }
                i = j + 1;
                continue;
            }
            _ => {
                for _ in 0..n {
                    out.push(c);
                }
            }
        }
        i += n;
    }
    out
}

fn is_date_pattern(pattern: &str) -> bool {
    let mut quoted = false;
    let mut bracket = false;
    for c in pattern.chars() {
        match c {
            '\'' | '"' => quoted = !quoted,
            '[' if !quoted => bracket = true,
            ']' if !quoted => bracket = false,
            'y' | 'd' | 'M' | 'H' | 'E' if !quoted && !bracket => return true,
            _ => {}
        }
    }
    false
}

impl Format {
    /// Whether the format writes numbers, so that typed numbers stay
    /// numbers.
    pub fn is_text(&self) -> bool {
        matches!(self, Self::Text)
    }

    /// A value written in this format.
    pub fn show(&self, value: &Value, locale: &Locale) -> Shown {
        let pt = locale.portuguese;
        let numeric = |text: String, red: bool| Shown {
            text,
            red,
            numeric: true,
            left_symbol: None,
        };
        let n = match value {
            Value::Empty => return Shown::default(),
            Value::Error(e) => {
                return Shown {
                    text: e.code().to_owned(),
                    ..Shown::default()
                };
            }
            Value::Text(t) => {
                let text = match self {
                    Self::Custom(c) if c.pattern.contains('@') => c.pattern.replace('@', t),
                    _ => t.clone(),
                };
                return Shown {
                    text,
                    ..Shown::default()
                };
            }
            Value::Bool(b) => {
                return Shown {
                    text: match (b, pt) {
                        (true, true) => "VERDADEIRO",
                        (false, true) => "FALSO",
                        (true, false) => "TRUE",
                        (false, false) => "FALSE",
                    }
                    .to_owned(),
                    ..Shown::default()
                };
            }
            Value::Number(n) => *n,
            Value::Date(d) => {
                return numeric(
                    match self {
                        Self::DateTime { date, time } => date_text(*d, *date, *time, locale),
                        Self::Custom(c) if is_date_pattern(&c.pattern) => {
                            custom_date(*d, &c.pattern, locale)
                        }
                        _ => {
                            let time = if d.fract() == 0.0 {
                                TimeStyle::None
                            } else {
                                TimeStyle::Short
                            };
                            let date = if *d < 1.0 {
                                DateStyle::None
                            } else {
                                DateStyle::Short
                            };
                            date_text(*d, date, time, locale)
                        }
                    },
                    false,
                );
            }
            Value::Duration(d) => {
                return numeric(
                    match self {
                        Self::Duration { style } => duration_text(*d, *style, pt),
                        _ => duration_text(*d, DurationStyle::Compact, pt),
                    },
                    false,
                );
            }
        };
        match self {
            Self::Automatic | Self::Text | Self::DateTime { .. } | Self::Duration { .. } => {
                let (text, red) = signed(
                    plain_number(n, None, false, locale),
                    n < 0.0,
                    Negative::Minus,
                );
                numeric(text, red)
            }
            Self::Number {
                decimals,
                thousands,
                negative,
            } => {
                let (text, red) = signed(
                    plain_number(n, *decimals, *thousands, locale),
                    n < 0.0 && round_nonzero(n, *decimals),
                    *negative,
                );
                numeric(text, red)
            }
            Self::Currency {
                symbol,
                decimals,
                thousands,
                negative,
                accounting,
            } => {
                let body = plain_number(n, Some(*decimals), *thousands, locale);
                let neg = n < 0.0 && round_nonzero(n, Some(*decimals));
                if *accounting {
                    let (text, red) = signed(body, neg, *negative);
                    Shown {
                        text,
                        red,
                        numeric: true,
                        left_symbol: Some(symbol.clone()),
                    }
                } else {
                    let space = if pt { "\u{a0}" } else { "" };
                    let (text, red) = signed(format!("{symbol}{space}{body}"), neg, *negative);
                    numeric(text, red)
                }
            }
            Self::Percent {
                decimals,
                thousands,
                negative,
            } => {
                let p = n * 100.0;
                let (text, red) = signed(
                    plain_number(p, *decimals, *thousands, locale),
                    p < 0.0 && round_nonzero(p, *decimals),
                    *negative,
                );
                numeric(format!("{text}%"), red)
            }
            Self::Fraction { precision } => numeric(fraction(n, *precision), false),
            Self::Base {
                base,
                places,
                twos_complement,
            } => numeric(in_base(n, *base, *places, *twos_complement), false),
            Self::Scientific { decimals } => {
                let s = scientific(n.abs(), *decimals, locale);
                numeric(if n < 0.0 { format!("-{s}") } else { s }, false)
            }
            Self::Custom(c) => {
                if is_date_pattern(&c.pattern) {
                    return numeric(custom_date(n, &c.pattern, locale), false);
                }
                let (text, red) = custom_number(n, &c.pattern, locale);
                numeric(text, red)
            }
        }
    }
}

fn round_nonzero(n: f64, decimals: Option<u8>) -> bool {
    match decimals {
        Some(d) => (n.abs() * 10f64.powi(i32::from(d))).round() != 0.0,
        None => n != 0.0,
    }
}

/// What typed text becomes: the content of the cell and, for Automatic
/// cells, the format that it was typed in, as a percentage or an amount
/// of money keeps its look (TAB-18).
pub fn parse_input<T: crate::CellText>(
    text: &str,
    format: &Format,
    locale: &Locale,
) -> (Content<T>, Option<Format>) {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return (Content::Empty, None);
    }
    if trimmed.starts_with('=') && trimmed.len() > 1 {
        return (Content::Formula(trimmed.to_owned()), None);
    }
    if format.is_text() {
        return (Content::Text(T::from_plain(text)), None);
    }
    match trimmed.to_uppercase().as_str() {
        "TRUE" | "VERDADEIRO" => return (Content::Bool(true), None),
        "FALSE" | "FALSO" => return (Content::Bool(false), None),
        _ => {}
    }
    if let Some((n, typed)) = parse_number(trimmed, locale) {
        let new_format = match (format, typed) {
            (Format::Automatic, Some(f)) => Some(f),
            _ => None,
        };
        return (Content::Number(Num(n)), new_format);
    }
    if let Some(d) = parse_date(trimmed, locale) {
        let new_format = match format {
            Format::Automatic => Some(Format::DateTime {
                date: if d >= 1.0 {
                    DateStyle::Short
                } else {
                    DateStyle::None
                },
                time: if d.fract() != 0.0 || trimmed.contains(':') {
                    TimeStyle::Short
                } else {
                    TimeStyle::None
                },
            }),
            _ => None,
        };
        return (Content::Date(Num(d)), new_format);
    }
    (Content::Text(T::from_plain(text)), None)
}

/// A number typed with digits, + − ( ) / currency % . , E and e (TAB-18),
/// with the format it implies.
pub fn parse_number(text: &str, locale: &Locale) -> Option<(f64, Option<Format>)> {
    let mut s = text.trim().replace('\u{a0}', " ");
    let mut negative = false;
    if s.starts_with('(') && s.ends_with(')') {
        negative = true;
        s = s[1..s.len() - 1].trim().to_owned();
    }
    for minus in ['-', '−'] {
        if let Some(rest) = s.strip_prefix(minus) {
            negative = !negative;
            s = rest.trim().to_owned();
        }
    }
    if let Some(rest) = s.strip_prefix('+') {
        s = rest.trim().to_owned();
    }
    let mut format = None;
    if let Some(rest) = s.strip_suffix('%') {
        s = rest.trim().to_owned();
        format = Some(Format::Percent {
            decimals: None,
            thousands: false,
            negative: Negative::Minus,
        });
    }
    for symbol in [locale.currency.as_str(), "R$", "US$", "$", "€", "£", "¥"] {
        if let Some(rest) = s.strip_prefix(symbol) {
            let rest = rest.trim().to_owned();
            match rest.strip_prefix(['-', '−']) {
                Some(r) => {
                    negative = !negative;
                    s = r.trim().to_owned();
                }
                None => s = rest,
            }
            format = Some(Format::Currency {
                symbol: symbol.to_owned(),
                decimals: 2,
                thousands: true,
                negative: Negative::Minus,
                accounting: false,
            });
            break;
        }
    }
    if s.is_empty() {
        return None;
    }
    // Fractions such as 1/2 or 1 1/2.
    if let Some((whole, frac)) = s.rsplit_once(' ').map_or(Some(("", s.as_str())), Some)
        && let Some((a, b)) = frac.split_once('/')
        && a.chars().all(|c| c.is_ascii_digit())
        && b.chars().all(|c| c.is_ascii_digit())
        && !a.is_empty()
        && !b.is_empty()
        && whole.chars().all(|c| c.is_ascii_digit())
        && format.is_none()
    {
        let (a, b): (f64, f64) = (a.parse().ok()?, b.parse().ok()?);
        if b == 0.0 {
            return None;
        }
        // Without a whole part, 6/1 could be a date: dates win.
        if whole.is_empty() && parse_date(frac, locale).is_some() {
            return None;
        }
        let w: f64 = if whole.is_empty() {
            0.0
        } else {
            whole.parse().ok()?
        };
        let n = w + a / b;
        return Some((
            if negative { -n } else { n },
            Some(Format::Fraction {
                precision: FractionPrecision::UpTo(1),
            }),
        ));
    }
    // Digits, with the separators of the locale or of plain numbers.
    let has_exp = s.contains(['e', 'E']);
    let normalized: String = if locale.decimal == ',' {
        if s.contains(',') || !s.contains('.') || s.matches('.').count() > 1 || grouped_by(&s, '.')
        {
            s.replace('.', "").replace(',', ".")
        } else {
            s.clone()
        }
    } else {
        s.replace(',', "")
    };
    if !normalized
        .chars()
        .all(|c| c.is_ascii_digit() || matches!(c, '.' | 'e' | 'E' | '+' | '-'))
        || !normalized
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit() || c == '.')
    {
        return None;
    }
    let n: f64 = normalized.parse().ok()?;
    if has_exp && format.is_none() {
        format = Some(Format::Scientific { decimals: 2 });
    }
    let n = if negative { -n } else { n };
    let n = match format {
        Some(Format::Percent { .. }) => n / 100.0,
        _ => n,
    };
    Some((n, format))
}

/// Whether text is digits in groups of three after its first group, as
/// 1.234.567 with dots.
fn grouped_by(s: &str, sep: char) -> bool {
    let parts: Vec<&str> = s.split(sep).collect();
    parts.len() > 1
        && parts[0].len() <= 3
        && parts[1..]
            .iter()
            .all(|p| p.len() == 3 && p.chars().all(|c| c.is_ascii_digit()))
}

/// A date or a time typed with slashes, hyphens or colons, in days from
/// 30 December 1899.
pub fn parse_date(text: &str, locale: &Locale) -> Option<f64> {
    let (date, time) = match text.split_once(' ') {
        Some((d, t)) if t.contains(':') => (Some(d), Some(t)),
        _ if text.contains(':') && !text.contains(['/', '-']) => (None, Some(text)),
        _ => (Some(text), None),
    };
    let mut days = 0.0;
    if let Some(d) = date {
        let parts: Vec<&str> = d.split(['/', '-', '.']).collect();
        if parts.len() < 2
            || parts.len() > 3
            || !parts
                .iter()
                .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
        {
            return None;
        }
        let nums: Vec<i32> = parts.iter().map(|p| p.parse().unwrap_or(0)).collect();
        let (y, m, day) = if parts[0].len() == 4 {
            (nums[0], nums[1], *nums.get(2)?)
        } else {
            let (m, day) = if locale.day_first {
                (nums[1], nums[0])
            } else {
                (nums[0], nums[1])
            };
            let y = match nums.get(2) {
                Some(y) if parts[2].len() <= 2 => 2000 + y - if *y > 50 { 100 } else { 0 },
                Some(y) => *y,
                None => 2000,
            };
            (y, m, day)
        };
        if !(1..=12).contains(&m) || day < 1 {
            return None;
        }
        let last = date_from_days(days_from_date(y, m + 1, 1) - 1).2 as i32;
        if day > last {
            return None;
        }
        days = days_from_date(y, m, day) as f64;
    }
    if let Some(t) = time {
        let (t, pm) = if let Some(r) = t.strip_suffix("PM").or_else(|| t.strip_suffix("pm")) {
            (r.trim(), Some(true))
        } else if let Some(r) = t.strip_suffix("AM").or_else(|| t.strip_suffix("am")) {
            (r.trim(), Some(false))
        } else {
            (t, None)
        };
        let parts: Vec<&str> = t.split(':').collect();
        if parts.len() < 2 || parts.len() > 3 {
            return None;
        }
        let mut h: i64 = parts[0].parse().ok()?;
        let m: i64 = parts[1].parse().ok()?;
        let s: i64 = parts.get(2).map_or(Some(0), |s| s.parse().ok())?;
        if let Some(pm) = pm {
            h = h % 12 + if pm { 12 } else { 0 };
        }
        if !(0..24).contains(&h) || !(0..60).contains(&m) || !(0..60).contains(&s) {
            return None;
        }
        days += (h * 3600 + m * 60 + s) as f64 / 86_400.0;
    }
    Some(days)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn show(f: &Format, v: Value) -> String {
        f.show(&v, &Locale::pt_br()).text
    }

    #[test]
    fn pxv_09_the_language_gives_the_words_and_the_region_the_numbers() {
        let money = Format::Currency {
            symbol: "R$".into(),
            decimals: 2,
            thousands: true,
            negative: Negative::Minus,
            accounting: false,
        };
        let long = Format::DateTime {
            date: DateStyle::Long,
            time: TimeStyle::None,
        };
        let short = Format::DateTime {
            date: DateStyle::Short,
            time: TimeStyle::None,
        };
        // 18 November 2026.
        let day = Value::Date(46_344.0);
        // English words with the numbers of Brazil.
        let en_br = Locale::from_tag("en-BR");
        assert_eq!(en_br.currency, "R$");
        assert_eq!(
            money.show(&Value::Number(1234.5), &en_br).text,
            "R$1.234,50"
        );
        assert_eq!(long.show(&day, &en_br).text, "November 18, 2026");
        assert_eq!(short.show(&day, &en_br).text, "18/11/2026");
        // Portuguese words with the numbers of the United States.
        let pt_us = Locale::from_tag("pt-US");
        assert_eq!(
            money.show(&Value::Number(1234.5), &pt_us).text,
            "R$\u{a0}1,234.50"
        );
        assert_eq!(long.show(&day, &pt_us).text, "18 de novembro de 2026");
        assert_eq!(short.show(&day, &pt_us).text, "11/18/2026");
        // A region of its own separator of dates, and one from the system.
        assert_eq!(
            short.show(&day, &Locale::from_tag("en-DE")).text,
            "18.11.2026"
        );
        assert_eq!(Locale::from_tag("pt_PT.UTF-8").currency, "€");
        // No region takes the one of the language.
        assert_eq!(Locale::from_tag("pt"), Locale::pt_br());
        assert_eq!(Locale::from_tag("und"), Locale::en_us());
        // The date fields of the text, in patterns of Unicode.
        assert_eq!(
            Locale::pt_br().date(46_344.5, "EEEE, d MMMM y"),
            "quarta-feira, 18 novembro 2026"
        );
        assert_eq!(
            Locale::en_us().date(46_344.5, "d/M/yy HH:mm"),
            "18/11/26 12:00"
        );
    }

    #[test]
    fn tab_28_formats_write_values() {
        let pt = Locale::pt_br();
        let n = Format::Number {
            decimals: Some(2),
            thousands: true,
            negative: Negative::Parentheses,
        };
        assert_eq!(show(&n, Value::Number(-1234.5)), "(1.234,50)");
        let c = Format::Currency {
            symbol: "R$".into(),
            decimals: 2,
            thousands: true,
            negative: Negative::Minus,
            accounting: false,
        };
        assert_eq!(show(&c, Value::Number(1234.5)), "R$\u{a0}1.234,50");
        let p = Format::Percent {
            decimals: Some(0),
            thousands: false,
            negative: Negative::Minus,
        };
        assert_eq!(show(&p, Value::Number(3.0)), "300%");
        let d = Format::DateTime {
            date: DateStyle::Long,
            time: TimeStyle::None,
        };
        assert_eq!(show(&d, Value::Date(39_819.0)), "6 de janeiro de 2009");
        let full = Format::DateTime {
            date: DateStyle::Full,
            time: TimeStyle::Short,
        };
        assert_eq!(
            Format::show(&full, &Value::Date(39_819.562_5), &Locale::en_us()).text,
            "Tuesday, January 6, 2009 13:30"
        );
        assert_eq!(
            show(
                &Format::Duration {
                    style: DurationStyle::Compact
                },
                Value::Duration(1.5)
            ),
            "1d 12h"
        );
        assert_eq!(
            show(
                &Format::Duration {
                    style: DurationStyle::Clock
                },
                Value::Duration(1.5)
            ),
            "36:00:00"
        );
        assert_eq!(
            show(
                &Format::Fraction {
                    precision: FractionPrecision::UpTo(1)
                },
                Value::Number(1.5)
            ),
            "1 1/2"
        );
        assert_eq!(
            show(
                &Format::Fraction {
                    precision: FractionPrecision::Exactly(8)
                },
                Value::Number(0.3)
            ),
            "1/4"
        );
        assert_eq!(
            show(
                &Format::Base {
                    base: 2,
                    places: 8,
                    twos_complement: false
                },
                Value::Number(5.0)
            ),
            "00000101"
        );
        assert_eq!(
            show(
                &Format::Base {
                    base: 16,
                    places: 1,
                    twos_complement: true
                },
                Value::Number(-1.0)
            ),
            "FFFFFFFF"
        );
        assert_eq!(
            show(&Format::Scientific { decimals: 2 }, Value::Number(12345.0)),
            "1,23E+04"
        );
        assert_eq!(show(&Format::Automatic, Value::Number(0.1 + 0.2)), "0,3");
        assert!(n.show(&Value::Number(-1.0), &pt).numeric);
    }

    #[test]
    fn tab_29_tab_30_custom_formats() {
        let kg = Format::Custom(CustomFormat {
            name: "Quilos".into(),
            pattern: "#,##0.0 \"kg\";[Red]-#,##0.0 \"kg\";\"zero\"".into(),
        });
        assert_eq!(show(&kg, Value::Number(1234.56)), "1.234,6 kg");
        let neg = kg.show(&Value::Number(-2.0), &Locale::pt_br());
        assert_eq!((neg.text.as_str(), neg.red), ("-2,0 kg", true));
        assert_eq!(show(&kg, Value::Number(0.0)), "zero");
        let thousands = Format::Custom(CustomFormat {
            name: "Milhares".into(),
            pattern: "#,##0, \"mil\"".into(),
        });
        assert_eq!(show(&thousands, Value::Number(1_500_000.0)), "1.500 mil");
        let cond = Format::Custom(CustomFormat {
            name: "Grande".into(),
            pattern: "[>1000]\"muito\";0".into(),
        });
        assert_eq!(show(&cond, Value::Number(2000.0)), "muito");
        assert_eq!(show(&cond, Value::Number(20.0)), "20");
        let date = Format::Custom(CustomFormat {
            name: "Dia".into(),
            pattern: "EEEE, d 'de' MMMM".into(),
        });
        assert_eq!(
            show(&date, Value::Date(39_819.0)),
            "terça-feira, 6 de janeiro"
        );
    }

    #[test]
    fn tab_18_typed_text_becomes_values() {
        let pt = Locale::pt_br();
        let en = Locale::en_us();
        let read = |t: &str, l: &Locale| parse_input::<String>(t, &Format::Automatic, l);
        assert_eq!(read("1.234,5", &pt).0, Content::Number(Num(1234.5)));
        assert_eq!(read("1,234.5", &en).0, Content::Number(Num(1234.5)));
        assert_eq!(read("(12)", &en).0, Content::Number(Num(-12.0)));
        assert_eq!(read("3%", &pt).0, Content::Number(Num(0.03)));
        assert!(matches!(read("3%", &pt).1, Some(Format::Percent { .. })));
        assert_eq!(read("R$ 10,50", &pt).0, Content::Number(Num(10.5)));
        assert!(matches!(
            read("R$ 10,50", &pt).1,
            Some(Format::Currency { .. })
        ));
        assert_eq!(read("1E3", &en).0, Content::Number(Num(1000.0)));
        assert_eq!(read("06/01/2009", &pt).0, Content::Date(Num(39_819.0)));
        assert_eq!(read("1/6/2009", &en).0, Content::Date(Num(39_819.0)));
        assert_eq!(read("2009-01-06", &en).0, Content::Date(Num(39_819.0)));
        assert_eq!(read("1 1/2", &en).0, Content::Number(Num(1.5)));
        assert_eq!(read("=A1+1", &en).0, Content::Formula("=A1+1".into()));
        assert_eq!(read("Verdadeiro", &pt).0, Content::Bool(true));
        assert_eq!(read("abc", &en).0, Content::Text("abc".into()));
        let as_text = parse_input::<String>("12", &Format::Text, &en).0;
        assert_eq!(as_text, Content::Text("12".into()));
    }
}
