// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The names of the merge fields of a contact, as Pages names them, and
//! the other names they go by (MAL-09).

/// The fields of a contact, in the order of the menu of Pages: the name,
/// the work, the phones, emails and web, the addresses and their parts,
/// and the rest. A field of a kind alone, as `Phone`, takes the first
/// value of that kind.
pub const CONTACT_FIELDS: &[&str] = &[
    "Name",
    "Prefix",
    "First Name",
    "Middle Name",
    "Last Name",
    "Suffix",
    "Nickname",
    "Job Title",
    "Department",
    "Company",
    "Phone",
    "Work Phone",
    "Home Phone",
    "Mobile Phone",
    "Work Fax",
    "Home Fax",
    "Other Phone",
    "Email",
    "Work Email",
    "Home Email",
    "Other Email",
    "URL",
    "Birthday",
    "Address",
    "Work Address",
    "Home Address",
    "Other Address",
    "Street",
    "City",
    "State",
    "ZIP",
    "Country",
    "Note",
];

/// Other names of the fields, in lower case, in English and in
/// Portuguese, as the column headers of spreadsheets and the fields of
/// templates name them.
const SYNONYMS: &[(&str, &str)] = &[
    ("full name", "Name"),
    ("display name", "Name"),
    ("nome completo", "Name"),
    ("title", "Prefix"),
    ("name prefix", "Prefix"),
    ("name suffix", "Suffix"),
    ("additional name", "Middle Name"),
    ("tratamento", "Prefix"),
    ("first", "First Name"),
    ("given name", "First Name"),
    ("forename", "First Name"),
    ("nome", "First Name"),
    ("primeiro nome", "First Name"),
    ("middle", "Middle Name"),
    ("nome do meio", "Middle Name"),
    ("last", "Last Name"),
    ("surname", "Last Name"),
    ("family name", "Last Name"),
    ("sobrenome", "Last Name"),
    ("apelido", "Nickname"),
    ("job", "Job Title"),
    ("position", "Job Title"),
    ("role", "Job Title"),
    ("cargo", "Job Title"),
    ("departamento", "Department"),
    ("organization", "Company"),
    ("organisation", "Company"),
    ("org", "Company"),
    ("empresa", "Company"),
    ("organização", "Company"),
    ("telephone", "Phone"),
    ("tel", "Phone"),
    ("phone number", "Phone"),
    ("telefone", "Phone"),
    ("telefone comercial", "Work Phone"),
    ("telefone residencial", "Home Phone"),
    ("mobile", "Mobile Phone"),
    ("cell", "Mobile Phone"),
    ("cell phone", "Mobile Phone"),
    ("celular", "Mobile Phone"),
    ("fax", "Work Fax"),
    ("e-mail", "Email"),
    ("mail", "Email"),
    ("email address", "Email"),
    ("e-mail comercial", "Work Email"),
    ("e-mail pessoal", "Home Email"),
    ("web", "URL"),
    ("website", "URL"),
    ("web site", "URL"),
    ("home page", "URL"),
    ("homepage", "URL"),
    ("site", "URL"),
    ("birth date", "Birthday"),
    ("date of birth", "Birthday"),
    ("aniversário", "Birthday"),
    ("data de nascimento", "Birthday"),
    ("endereço", "Address"),
    ("endereço comercial", "Work Address"),
    ("endereço residencial", "Home Address"),
    ("street address", "Street"),
    ("rua", "Street"),
    ("logradouro", "Street"),
    ("town", "City"),
    ("cidade", "City"),
    ("province", "State"),
    ("region", "State"),
    ("estado", "State"),
    ("uf", "State"),
    ("zip code", "ZIP"),
    ("postal code", "ZIP"),
    ("postcode", "ZIP"),
    ("cep", "ZIP"),
    ("país", "Country"),
    ("pais", "Country"),
    ("notes", "Note"),
    ("nota", "Note"),
    ("notas", "Note"),
    ("observações", "Note"),
];

/// The field of a contact that a name means, as Pages names it: the name
/// itself in any case, or one of its other names; none for a name that
/// is not a field of a contact.
pub fn canonical(name: &str) -> Option<&'static str> {
    let name = name.trim();
    if let Some(field) = CONTACT_FIELDS.iter().find(|f| f.eq_ignore_ascii_case(name)) {
        return Some(field);
    }
    let lower = name.to_lowercase();
    SYNONYMS
        .iter()
        .find(|(other, _)| *other == lower)
        .map(|(_, field)| *field)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mal_09_a_field_goes_by_its_name_and_its_other_names() {
        assert_eq!(canonical("first name"), Some("First Name"));
        assert_eq!(canonical(" Surname "), Some("Last Name"));
        assert_eq!(canonical("Empresa"), Some("Company"));
        assert_eq!(canonical("CEP"), Some("ZIP"));
        assert_eq!(canonical("Endereço"), Some("Address"));
        assert_eq!(canonical("Shoe size"), None);
        // Every other name leads to a field.
        for (_, field) in SYNONYMS {
            assert!(CONTACT_FIELDS.contains(field), "{field}");
        }
    }
}
