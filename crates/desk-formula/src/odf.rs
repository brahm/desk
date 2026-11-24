// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

//! Formulas in OpenFormula, the syntax of formulas in ODF, as
//! `of:=SUM([.A1:.B2];3)`.

use crate::parse::{Expr, Formula, ParseError};

impl Formula {
    /// The formula in OpenFormula, with the `of:` namespace prefix.
    pub fn to_odf(&self) -> String {
        let text = self.to_string();
        let mut out = String::from("of:");
        let mut chars = text.chars().peekable();
        let mut in_text = false;
        let mut in_quote = false;
        let mut token = String::new();
        let flush = |token: &mut String, out: &mut String| {
            if token.is_empty() {
                return;
            }
            if let Some((a, b)) = token.split_once(':')
                && crate::CellRef::parse(a).is_some()
                && crate::CellRef::parse(b).is_some()
            {
                out.push_str(&format!("[.{a}:.{b}]"));
            } else if crate::CellRef::parse(token).is_some() {
                out.push_str(&format!("[.{token}]"));
            } else {
                out.push_str(token);
            }
            token.clear();
        };
        while let Some(c) = chars.next() {
            if in_text {
                out.push(c);
                if c == '"' {
                    if chars.peek() == Some(&'"') {
                        out.push(chars.next().unwrap_or('"'));
                    } else {
                        in_text = false;
                    }
                }
                continue;
            }
            if in_quote {
                out.push(c);
                if c == '\'' {
                    in_quote = false;
                }
                continue;
            }
            match c {
                '"' => {
                    flush(&mut token, &mut out);
                    in_text = true;
                    out.push(c);
                }
                '\'' => {
                    flush(&mut token, &mut out);
                    in_quote = true;
                    out.push(c);
                }
                c if c.is_alphanumeric() || c == '$' || c == ':' || c == '.' || c == '_' => {
                    token.push(c);
                }
                ',' => {
                    flush(&mut token, &mut out);
                    out.push(';');
                }
                // A name before a parenthesis is a function, whatever
                // it looks like: LOG10 is also a cell.
                '(' => {
                    out.push_str(&token);
                    token.clear();
                    out.push(c);
                }
                c => {
                    flush(&mut token, &mut out);
                    out.push(c);
                }
            }
        }
        flush(&mut token, &mut out);
        out
    }

    /// Reads a formula in OpenFormula, with or without a namespace prefix
    /// such as `of:` or `oooc:`; references to other sheets are kept as
    /// `#REF!`, as a table of a document has no other sheets.
    pub fn parse_odf(text: &str) -> Result<Self, ParseError> {
        let body = text
            .split_once(':')
            .filter(|(prefix, _)| {
                !prefix.is_empty() && prefix.chars().all(|c| c.is_ascii_alphabetic())
            })
            .map_or(text, |(_, rest)| rest);
        let mut out = String::new();
        let mut chars = body.chars().peekable();
        let mut in_text = false;
        let mut in_quote = false;
        while let Some(c) = chars.next() {
            if in_text {
                out.push(c);
                if c == '"' {
                    if chars.peek() == Some(&'"') {
                        out.push(chars.next().unwrap_or('"'));
                    } else {
                        in_text = false;
                    }
                }
                continue;
            }
            // A header named in quotes may hold a bracket.
            if in_quote {
                out.push(c);
                if c == '\'' {
                    in_quote = false;
                }
                continue;
            }
            match c {
                '"' => {
                    in_text = true;
                    out.push(c);
                }
                '\'' => {
                    in_quote = true;
                    out.push(c);
                }
                '[' => {
                    // A sheet named in quotes may hold a bracket too.
                    let mut reference = String::new();
                    let mut quoted = false;
                    for d in chars.by_ref() {
                        if d == ']' && !quoted {
                            break;
                        }
                        if d == '\'' {
                            quoted = !quoted;
                        }
                        reference.push(d);
                    }
                    let parts: Vec<&str> = reference.split(':').collect();
                    let mut cells = Vec::new();
                    for part in parts {
                        // A sheet before the dot names another table.
                        match part.split_once('.') {
                            Some(("", cell)) => cells.push(cell.to_owned()),
                            _ => {
                                cells.clear();
                                break;
                            }
                        }
                    }
                    if cells.is_empty() {
                        out.push_str("#REF!");
                    } else {
                        out.push_str(&cells.join(":"));
                    }
                }
                _ => out.push(c),
            }
        }
        Self::parse(&out)
    }
}

impl Expr {
    /// Whether the expression is only a number, as a cell with a constant.
    pub fn is_constant(&self) -> bool {
        matches!(self, Self::Number(_) | Self::Text(_) | Self::Bool(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_32_formulas_go_to_odf_and_back() {
        let f = Formula::parse("=SUM(A1:$B$2,3)&\"a,b\"").unwrap();
        assert_eq!(f.to_odf(), "of:=SUM([.A1:.$B$2];3)&\"a,b\"");
        assert_eq!(Formula::parse_odf(&f.to_odf()).unwrap(), f);
        assert_eq!(
            Formula::parse_odf("of:=[Sheet2.A1]+1").unwrap().to_string(),
            "=#REF!+1"
        );
    }

    #[test]
    fn a_function_named_like_a_cell_stays_a_function() {
        // LOG10 is also the cell in column LOG, row 10: it was written as
        // [.LOG10](100), which no reader takes (review RV, batch 2).
        let f = Formula::parse("=LOG10(100)+A1").unwrap();
        assert_eq!(f.to_odf(), "of:=LOG10(100)+[.A1]");
        assert_eq!(Formula::parse_odf(&f.to_odf()).unwrap(), f);
    }

    #[test]
    fn a_header_with_a_bracket_goes_to_odf_and_back() {
        let f = Formula::parse("=SUM('Preço [R$]')+A1").unwrap();
        assert_eq!(Formula::parse_odf(&f.to_odf()).unwrap(), f);
        assert_eq!(
            Formula::parse_odf("of:=['Plan [1]'.A1]+1")
                .unwrap()
                .to_string(),
            "=#REF!+1"
        );
    }
}
