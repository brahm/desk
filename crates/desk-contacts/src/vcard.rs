// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! vCard files (RFC 6350, and the versions 2.1 and 3.0 that Contacts of
//! GNOME and of the Mac still write): the cards of a file, and the merge
//! fields of each (MAL-03, MAL-09).

/// The most cards read from one file.
pub const MAX_CARDS: usize = 100_000;

/// The most properties read from one card.
const MAX_PROPERTIES: usize = 10_000;

/// A line of a card: its name, in upper case without its group, the
/// types it has, in lower case, and its value, escapes undone but not
/// split.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Property {
    name: String,
    types: Vec<String>,
    value: String,
}

/// A card of a contact.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Card {
    properties: Vec<Property>,
}

/// Reads the cards of a vCard file.
pub fn parse(text: &str) -> Vec<Card> {
    let mut cards = Vec::new();
    let mut card: Option<Card> = None;
    for line in unfold(text) {
        let Some(property) = property(&line) else {
            continue;
        };
        match property.name.as_str() {
            "BEGIN" if property.value.eq_ignore_ascii_case("VCARD") => {
                card = Some(Card::default());
            }
            "END" if property.value.eq_ignore_ascii_case("VCARD") => {
                if let Some(card) = card.take() {
                    cards.push(card);
                    if cards.len() >= MAX_CARDS {
                        break;
                    }
                }
            }
            _ => {
                if let Some(card) = card.as_mut()
                    && card.properties.len() < MAX_PROPERTIES
                {
                    card.properties.push(property);
                }
            }
        }
    }
    cards
}

/// The logical lines of a file: a line that starts with a space or a tab
/// goes on the one before, and so does the line after one of quoted
/// printable text that ends in `=`, as vCard 2.1 folds them.
fn unfold(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut soft = false;
    for raw in text.split('\n') {
        let raw = raw.strip_suffix('\r').unwrap_or(raw);
        if let Some(last) = lines.last_mut() {
            if soft {
                last.pop();
                last.push_str(raw);
                soft = last.ends_with('=') && quoted_printable(last);
                continue;
            }
            if let Some(rest) = raw.strip_prefix([' ', '\t']) {
                last.push_str(rest);
                continue;
            }
        }
        lines.push(raw.to_owned());
        let last = lines.last().map(String::as_str).unwrap_or_default();
        soft = last.ends_with('=') && quoted_printable(last);
    }
    lines
}

/// Whether a line is of quoted printable text.
fn quoted_printable(line: &str) -> bool {
    let head = line.split(':').next().unwrap_or_default();
    head.to_ascii_uppercase().contains("QUOTED-PRINTABLE")
}

/// A line read as a property, or none when it has no name.
fn property(line: &str) -> Option<Property> {
    let (head, value) = line.split_once(':')?;
    let mut parts = head.split(';');
    let name = parts.next()?.trim();
    // A group before the name, as `item1.EMAIL`.
    let name = name.rsplit('.').next().unwrap_or(name).to_ascii_uppercase();
    if name.is_empty() {
        return None;
    }
    let mut types = Vec::new();
    let mut quoted = false;
    let mut charset = None;
    for parameter in parts {
        let (key, values) = match parameter.split_once('=') {
            Some((key, values)) => (key.trim().to_ascii_uppercase(), values),
            // vCard 2.1 names an encoding alone, as
            // `NOTE;QUOTED-PRINTABLE`, and a type alone, as
            // `TEL;WORK;VOICE`.
            None if ["QUOTED-PRINTABLE", "8BIT", "BASE64"]
                .iter()
                .any(|e| parameter.trim().eq_ignore_ascii_case(e)) =>
            {
                ("ENCODING".to_owned(), parameter)
            }
            None => ("TYPE".to_owned(), parameter),
        };
        match key.as_str() {
            "TYPE" => types.extend(
                values
                    .trim_matches('"')
                    .split(',')
                    .map(|t| t.trim().to_ascii_lowercase())
                    .filter(|t| !t.is_empty()),
            ),
            "ENCODING" => quoted |= values.eq_ignore_ascii_case("QUOTED-PRINTABLE"),
            "CHARSET" => charset = Some(values.trim_matches('"').to_owned()),
            // vCard 4.0 ranks them from PREF=1, the preferred one.
            "PREF" if values.trim() == "1" => types.push("pref".to_owned()),
            _ => {}
        }
    }
    let value = if quoted {
        let bytes = decode_quoted_printable(value);
        let encoding = charset
            .and_then(|c| encoding_rs::Encoding::for_label(c.as_bytes()))
            .unwrap_or(encoding_rs::UTF_8);
        encoding.decode(&bytes).0.into_owned()
    } else {
        value.to_owned()
    };
    Some(Property { name, types, value })
}

/// The bytes of quoted printable text.
fn decode_quoted_printable(text: &str) -> Vec<u8> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while let Some(&byte) = bytes.get(i) {
        if byte == b'='
            && let Some(hex) = text.get(i + 1..i + 3)
            && let Ok(value) = u8::from_str_radix(hex, 16)
        {
            out.push(value);
            i += 3;
        } else {
            out.push(byte);
            i += 1;
        }
    }
    out
}

/// The components of a structured value, split at the semicolons that
/// are not escaped, each with its escapes undone.
fn components(value: &str) -> Vec<String> {
    let mut out = vec![String::new()];
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some('n' | 'N') => push(&mut out, '\n'),
                Some(other) => push(&mut out, other),
                None => {}
            },
            ';' => out.push(String::new()),
            other => push(&mut out, other),
        }
    }
    out.into_iter().map(|c| c.trim().to_owned()).collect()
}

fn push(out: &mut [String], c: char) {
    if let Some(last) = out.last_mut() {
        last.push(c);
    }
}

/// The first of a list of values, split at the commas that are not
/// escaped, with its escapes undone.
fn first_of_list(value: &str) -> Option<String> {
    let mut first = String::new();
    let mut escaped = false;
    for c in value.chars() {
        match c {
            ',' if !escaped => break,
            '\\' if !escaped => escaped = true,
            _ => {
                if escaped {
                    first.push('\\');
                }
                escaped = false;
                first.push(c);
            }
        }
    }
    Some(text(&first))
}

/// The entries of a list of values, as those of `CATEGORIES`, an escaped
/// comma kept in its entry.
fn list(value: &str) -> Vec<String> {
    let mut entries = vec![String::new()];
    let mut escaped = false;
    for c in value.chars() {
        match c {
            ',' if !escaped => entries.push(String::new()),
            '\\' if !escaped => escaped = true,
            _ => {
                if let Some(entry) = entries.last_mut() {
                    if escaped {
                        entry.push('\\');
                    }
                    entry.push(c);
                }
                escaped = false;
            }
        }
    }
    entries
        .iter()
        .map(|e| text(e).trim().to_owned())
        .filter(|e| !e.is_empty())
        .collect()
}

/// A value with its escapes undone, its commas and semicolons kept.
fn text(value: &str) -> String {
    components(value).join(";")
}

/// The lines of an address, from the components of `ADR`: the post
/// office box, the extended address, the street, the city, the region,
/// the postal code and the country.
fn address(value: &str) -> String {
    let parts = components(value);
    let part = |i: usize| parts.get(i).map(String::as_str).unwrap_or_default();
    let mut lines: Vec<String> = [part(0), part(1), part(2)]
        .iter()
        .flat_map(|p| p.split('\n'))
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(str::to_owned)
        .collect();
    let city = match (part(3), part(4)) {
        ("", region) => region.to_owned(),
        (city, "") => city.to_owned(),
        (city, region) => format!("{city}, {region}"),
    };
    let town = [city.as_str(), part(5)]
        .iter()
        .filter(|p| !p.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(" ");
    lines.extend(
        [town, part(6).to_owned()]
            .into_iter()
            .filter(|l| !l.is_empty()),
    );
    lines.join("\n")
}

impl Card {
    /// The properties of a name, the preferred ones first.
    fn all(&self, name: &str) -> impl Iterator<Item = &Property> {
        let (pref, rest): (Vec<_>, Vec<_>) = self
            .properties
            .iter()
            .filter(|p| p.name == name)
            .partition(|p| p.types.iter().any(|t| t == "pref"));
        pref.into_iter().chain(rest)
    }

    /// The first property of a name and of a type; of any type when
    /// `kind` is none, and of none of work and home for `other`.
    fn of(&self, name: &str, kind: Option<&str>) -> Option<&Property> {
        let mut all = self.all(name);
        match kind {
            None => all.next(),
            Some("other") => all.find(|p| {
                !p.types
                    .iter()
                    .any(|t| matches!(t.as_str(), "work" | "home"))
            }),
            Some(kind) => all.find(|p| p.types.iter().any(|t| t == kind)),
        }
    }

    /// The first phone of a kind, as `cell`, but not a fax unless asked.
    fn phone(&self, kind: Option<&str>, fax: bool) -> Option<String> {
        self.all("TEL")
            .filter(|p| p.types.iter().any(|t| t == "fax") == fax)
            .find(|p| match kind {
                None => true,
                Some("other") => !p
                    .types
                    .iter()
                    .any(|t| matches!(t.as_str(), "work" | "home" | "cell")),
                Some(kind) => p.types.iter().any(|t| t == kind),
            })
            .map(|p| text(&p.value))
    }

    /// The groups of the contact, as the categories of its card, which
    /// GNOME Contacts and Evolution make its groups.
    pub fn categories(&self) -> Vec<String> {
        self.all("CATEGORIES")
            .flat_map(|p| list(&p.value))
            .collect()
    }

    /// The datum of a merge field of the card, by the name of the field
    /// or one of its other names (MAL-09); none when the card has none.
    pub fn get(&self, field: &str) -> Option<String> {
        let field = crate::canonical(field)?;
        let name = || self.of("N", None).map(|p| components(&p.value));
        let part = |i: usize| name().and_then(|n| n.get(i).cloned());
        let first_address = || self.of("ADR", None).map(|p| components(&p.value));
        let address_part = |i: usize| first_address().and_then(|a| a.get(i).cloned());
        let org = || self.of("ORG", None).map(|p| components(&p.value));
        let value = match field {
            "Name" => self.of("FN", None).map(|p| text(&p.value)).or_else(|| {
                let n = name()?;
                let get = |i: usize| n.get(i).map(String::as_str).unwrap_or_default();
                Some(
                    [get(3), get(1), get(2), get(0), get(4)]
                        .iter()
                        .filter(|p| !p.is_empty())
                        .copied()
                        .collect::<Vec<_>>()
                        .join(" "),
                )
            }),
            "Prefix" => part(3),
            "First Name" => part(1),
            "Middle Name" => part(2),
            "Last Name" => part(0),
            "Suffix" => part(4),
            "Nickname" => self
                .of("NICKNAME", None)
                .and_then(|p| first_of_list(&p.value)),
            "Job Title" => self.of("TITLE", None).map(|p| text(&p.value)),
            "Company" => org().and_then(|o| o.into_iter().next()),
            "Department" => org().and_then(|o| o.into_iter().nth(1)),
            "Phone" => self.phone(None, false),
            "Work Phone" => self.phone(Some("work"), false),
            "Home Phone" => self.phone(Some("home"), false),
            "Mobile Phone" => self.phone(Some("cell"), false),
            "Other Phone" => self.phone(Some("other"), false),
            "Work Fax" => self.phone(Some("work"), true),
            "Home Fax" => self.phone(Some("home"), true),
            "Email" => self.of("EMAIL", None).map(|p| text(&p.value)),
            "Work Email" => self.of("EMAIL", Some("work")).map(|p| text(&p.value)),
            "Home Email" => self.of("EMAIL", Some("home")).map(|p| text(&p.value)),
            "Other Email" => self.of("EMAIL", Some("other")).map(|p| text(&p.value)),
            "URL" => self.of("URL", None).map(|p| text(&p.value)),
            "Birthday" => self.of("BDAY", None).map(|p| text(&p.value)),
            "Address" => self.of("ADR", None).map(|p| address(&p.value)),
            "Work Address" => self.of("ADR", Some("work")).map(|p| address(&p.value)),
            "Home Address" => self.of("ADR", Some("home")).map(|p| address(&p.value)),
            "Other Address" => self.of("ADR", Some("other")).map(|p| address(&p.value)),
            "Street" => address_part(2),
            "City" => address_part(3),
            "State" => address_part(4),
            "ZIP" => address_part(5),
            "Country" => address_part(6),
            "Note" => self.of("NOTE", None).map(|p| text(&p.value)),
            _ => None,
        };
        value.filter(|v| !v.trim().is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ANA: &str = "BEGIN:VCARD\r\nVERSION:3.0\r\nN:Souza;Ana;Maria;Dra.;\r\nFN:Dra. Ana Maria Souza\r\nORG:Desk Ltda.;Engenharia\r\nTITLE:Diretora\r\nTEL;TYPE=WORK,VOICE:+55 11 3000-0000\r\nTEL;TYPE=CELL:+55 11 90000-0000\r\nTEL;TYPE=WORK,FAX:+55 11 3000-0001\r\nEMAIL;TYPE=HOME:ana@example.org\r\nitem1.EMAIL;TYPE=WORK;TYPE=pref:ana@desk.example\r\nADR;TYPE=WORK:;Sala 4;Av. Paulista\\, 1000;São Paulo;SP;01310-100;Brasil\r\nNOTE:Prefere ser chamada\r\n  de Ana.\r\nBDAY:1980-04-02\r\nEND:VCARD\r\n";

    #[test]
    fn mal_04_the_groups_of_a_card_are_its_categories() {
        let cards = parse(
            "BEGIN:VCARD\r\nVERSION:3.0\r\nFN:Ana\r\nCATEGORIES:Clients,Família\\, perto\r\nCATEGORIES:VIP\r\nEND:VCARD\r\n",
        );
        assert_eq!(cards[0].categories(), ["Clients", "Família, perto", "VIP"]);
        assert!(parse(ANA)[0].categories().is_empty());
    }

    #[test]
    fn mal_09_the_fields_of_a_card_of_vcard_3() {
        let cards = parse(ANA);
        assert_eq!(cards.len(), 1);
        let ana = &cards[0];
        let get = |f: &str| ana.get(f);
        assert_eq!(get("First Name").as_deref(), Some("Ana"));
        assert_eq!(get("Last Name").as_deref(), Some("Souza"));
        assert_eq!(get("Middle Name").as_deref(), Some("Maria"));
        assert_eq!(get("Prefix").as_deref(), Some("Dra."));
        assert_eq!(get("Suffix"), None);
        assert_eq!(get("Name").as_deref(), Some("Dra. Ana Maria Souza"));
        assert_eq!(get("Company").as_deref(), Some("Desk Ltda."));
        assert_eq!(get("Empresa").as_deref(), Some("Desk Ltda."));
        assert_eq!(get("Department").as_deref(), Some("Engenharia"));
        assert_eq!(get("Job Title").as_deref(), Some("Diretora"));
        assert_eq!(get("Phone").as_deref(), Some("+55 11 3000-0000"));
        assert_eq!(get("Mobile Phone").as_deref(), Some("+55 11 90000-0000"));
        assert_eq!(get("Work Fax").as_deref(), Some("+55 11 3000-0001"));
        assert_eq!(get("Home Phone"), None);
        // The preferred email comes first.
        assert_eq!(get("Email").as_deref(), Some("ana@desk.example"));
        assert_eq!(get("Home Email").as_deref(), Some("ana@example.org"));
        assert_eq!(
            get("Work Address").as_deref(),
            Some("Sala 4\nAv. Paulista, 1000\nSão Paulo, SP 01310-100\nBrasil")
        );
        assert_eq!(get("Address"), get("Work Address"));
        assert_eq!(get("Home Address"), None);
        assert_eq!(get("City").as_deref(), Some("São Paulo"));
        assert_eq!(get("CEP").as_deref(), Some("01310-100"));
        assert_eq!(get("Note").as_deref(), Some("Prefere ser chamada de Ana."));
        assert_eq!(get("Birthday").as_deref(), Some("1980-04-02"));
        assert_eq!(get("Shoe Size"), None);
    }

    #[test]
    fn mal_09_a_card_of_vcard_2_1_in_quoted_printable() {
        let text = "BEGIN:VCARD\nVERSION:2.1\nN;CHARSET=UTF-8;ENCODING=QUOTED-PRINTABLE:Jo=C3=A3o;Jos=\n=C3=A9\nTEL;HOME;VOICE:555-0100\nADR;HOME:;;1 Main St;Springfield;;;\nEND:VCARD\nBEGIN:VCARD\nN:Two;Card\nEND:VCARD\n";
        let cards = parse(text);
        assert_eq!(cards.len(), 2);
        assert_eq!(cards[0].get("Last Name").as_deref(), Some("João"));
        assert_eq!(cards[0].get("First Name").as_deref(), Some("José"));
        assert_eq!(cards[0].get("Home Phone").as_deref(), Some("555-0100"));
        assert_eq!(
            cards[0].get("Home Address").as_deref(),
            Some("1 Main St\nSpringfield")
        );
        assert_eq!(cards[1].get("Name").as_deref(), Some("Card Two"));
        // An encoding named alone, and a charset of its own.
        let text = "BEGIN:VCARD\nN;CHARSET=ISO-8859-1;QUOTED-PRINTABLE:Jo=E3o;Ana\nNICKNAME:Bo\\, Jr.,Bobby\nEMAIL;PREF=100:old@x\nEMAIL:new@x\nEMAIL;PREF=1:best@x\nEND:VCARD\n";
        let card = &parse(text)[0];
        assert_eq!(card.get("Last Name").as_deref(), Some("João"));
        assert_eq!(card.get("Nickname").as_deref(), Some("Bo, Jr."));
        assert_eq!(card.get("Email").as_deref(), Some("best@x"));
    }

    #[test]
    fn mal_09_text_that_is_not_a_card_reads_as_none() {
        assert!(parse("").is_empty());
        assert!(parse("N:Lost;Line\nEND:VCARD\n:no name\n").is_empty());
        // A card that never ends is left out.
        assert!(parse("BEGIN:VCARD\nN:A;B").is_empty());
    }
}
