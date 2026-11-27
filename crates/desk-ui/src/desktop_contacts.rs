// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The contacts of the desktop, from Evolution Data Server, which GNOME
//! Contacts and Evolution keep, as the source of a mail merge (W105,
//! MAL-01, MAL-04): every contact, or those of a group, and the
//! person's own card, which fills the sender fields.
//!
//! The server is reached on the session bus: its sources say which
//! address books there are, and each book gives its contacts as vCard.
//! A Flatpak needs to talk to `org.gnome.evolution.dataserver.*`.

use desk_contacts::vcard::{self, Card};
use gtk::gio;
use gtk::glib;
use gtk::prelude::*;

/// The service of the sources of Evolution Data Server.
const SOURCES: &str = "org.gnome.evolution.dataserver.Sources5";
const SOURCES_PATH: &str = "/org/gnome/evolution/dataserver/SourceManager";
const SOURCE_INTERFACE: &str = "org.gnome.evolution.dataserver.Source";
/// The service that opens address books.
const BOOKS: &str = "org.gnome.evolution.dataserver.AddressBook10";
const BOOKS_PATH: &str = "/org/gnome/evolution/dataserver/AddressBookFactory";
const FACTORY_INTERFACE: &str = "org.gnome.evolution.dataserver.AddressBookFactory";
const BOOK_INTERFACE: &str = "org.gnome.evolution.dataserver.AddressBook";
/// Every contact, in the query language of the server.
const EVERY_CONTACT: &str = "(contains \"x-evolution-any-field\" \"\")";
/// How long the server has for each call, in milliseconds.
const TIMEOUT: i32 = 10_000;

/// Whether the data of a source of the server, a key file, is that of an
/// address book that is on.
pub fn is_address_book(data: &str) -> bool {
    let file = glib::KeyFile::new();
    if file.load_from_data(data, glib::KeyFileFlags::NONE).is_err() {
        return false;
    }
    file.has_group("Address Book") && file.boolean("Data Source", "Enabled").unwrap_or(true)
}

fn call(
    bus: &gio::DBusConnection,
    service: &str,
    path: &str,
    interface: &str,
    method: &str,
    arguments: Option<&glib::Variant>,
) -> Result<glib::Variant, glib::Error> {
    bus.call_sync(
        Some(service),
        path,
        interface,
        method,
        arguments,
        None,
        gio::DBusCallFlags::NONE,
        TIMEOUT,
        gio::Cancellable::NONE,
    )
}

/// The identifiers of the address books of the server.
fn address_books(bus: &gio::DBusConnection) -> Result<Vec<String>, glib::Error> {
    let objects = call(
        bus,
        SOURCES,
        SOURCES_PATH,
        "org.freedesktop.DBus.ObjectManager",
        "GetManagedObjects",
        None,
    )?;
    let mut books = Vec::new();
    // a{oa{sa{sv}}}
    let objects = objects.child_value(0);
    for object in objects.iter() {
        let interfaces = object.child_value(1);
        for interface in interfaces.iter() {
            if interface.child_value(0).str() != Some(SOURCE_INTERFACE) {
                continue;
            }
            let properties = glib::VariantDict::new(Some(&interface.child_value(1)));
            let uid = properties.lookup::<String>("UID").ok().flatten();
            let data = properties.lookup::<String>("Data").ok().flatten();
            if let (Some(uid), Some(data)) = (uid, data)
                && is_address_book(&data)
            {
                books.push(uid);
            }
        }
    }
    Ok(books)
}

/// The cards of an address book.
fn cards_of(bus: &gio::DBusConnection, uid: &str) -> Result<Vec<Card>, glib::Error> {
    let opened = call(
        bus,
        BOOKS,
        BOOKS_PATH,
        FACTORY_INTERFACE,
        "OpenAddressBook",
        Some(&(uid,).to_variant()),
    )?;
    let (path, service) = opened.get::<(String, String)>().ok_or_else(|| {
        glib::Error::new(
            gio::IOErrorEnum::InvalidData,
            "an address book without a path",
        )
    })?;
    call(bus, &service, &path, BOOK_INTERFACE, "Open", None)?;
    let list = call(
        bus,
        &service,
        &path,
        BOOK_INTERFACE,
        "GetContactList",
        Some(&(EVERY_CONTACT,).to_variant()),
    );
    // The book is closed whatever it gave.
    let _ = call(bus, &service, &path, BOOK_INTERFACE, "Close", None);
    let (vcards,) = list?.get::<(Vec<String>,)>().ok_or_else(|| {
        glib::Error::new(gio::IOErrorEnum::InvalidData, "contacts that are not vCard")
    })?;
    let mut cards = Vec::new();
    for text in vcards {
        cards.extend(vcard::parse(&text));
        if cards.len() >= vcard::MAX_CARDS {
            cards.truncate(vcard::MAX_CARDS);
            break;
        }
    }
    Ok(cards)
}

/// Every card of the desktop, of every address book.
pub fn cards() -> Result<Vec<Card>, glib::Error> {
    let bus = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE)?;
    let mut cards = Vec::new();
    for uid in address_books(&bus)? {
        // A book that cannot be opened, as one of an account offline,
        // leaves the others.
        match cards_of(&bus, &uid) {
            Ok(more) => cards.extend(more),
            Err(error) => glib::g_warning!("desk", "cannot read the address book {uid}: {error}"),
        }
    }
    Ok(cards)
}

/// The cards of a group, or every card.
pub fn in_group(cards: Vec<Card>, group: Option<&str>) -> Vec<Card> {
    match group {
        None => cards,
        Some(group) => cards
            .into_iter()
            .filter(|c| c.categories().iter().any(|g| g == group))
            .collect(),
    }
}

/// The groups of the cards, in the order of their names.
pub fn groups(cards: &[Card]) -> Vec<String> {
    let mut groups: Vec<String> = cards.iter().flat_map(Card::categories).collect();
    groups.sort_by_key(|g| g.to_lowercase());
    groups.dedup();
    groups
}

/// The person's own card among the cards: the one of their full name, as
/// the system knows it, since GNOME keeps no card marked as one's own.
pub fn own_card<'a>(cards: &'a [Card], real_name: &str) -> Option<&'a Card> {
    let real_name = real_name.trim();
    if real_name.is_empty() || real_name == "Unknown" {
        return None;
    }
    cards.iter().find(|c| {
        c.get("Name")
            .is_some_and(|n| n.trim().eq_ignore_ascii_case(real_name))
    })
}

thread_local! {
    /// The person's own card, once the desktop was asked for it.
    static OWN: std::cell::RefCell<Option<Option<std::rc::Rc<Card>>>> =
        const { std::cell::RefCell::new(None) };
}

/// Calls `f` with the person's own card, or none, asking the desktop for
/// it the first time, away from the window so that it does not wait.
pub fn with_own_card(f: impl FnOnce(Option<std::rc::Rc<Card>>) + 'static) {
    if let Some(card) = OWN.with_borrow(Clone::clone) {
        f(card);
        return;
    }
    glib::MainContext::default().spawn_local(async move {
        let name = glib::real_name().to_string_lossy().into_owned();
        let card = gio::spawn_blocking(move || {
            cards()
                .ok()
                .and_then(|cards| own_card(&cards, &name).cloned())
        })
        .await
        .ok()
        .flatten()
        .map(std::rc::Rc::new);
        OWN.with_borrow_mut(|own| *own = Some(card.clone()));
        f(card);
    });
}

/// The person's own card, if the desktop gave it already.
pub fn own_card_now() -> Option<std::rc::Rc<Card>> {
    OWN.with_borrow(Clone::clone).flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mal_04_the_address_books_are_the_sources_that_have_one() {
        assert!(is_address_book(
            "[Data Source]\nDisplayName=Personal\nEnabled=true\n\n[Address Book]\nBackendName=local\n"
        ));
        assert!(!is_address_book(
            "[Data Source]\nEnabled=false\n\n[Address Book]\nBackendName=local\n"
        ));
        assert!(!is_address_book(
            "[Data Source]\nEnabled=true\n\n[Calendar]\n"
        ));
        assert!(!is_address_book("not a key file ["));
    }

    #[test]
    fn mal_01_mal_04_a_group_and_the_own_card() {
        let cards = vcard::parse(
            "BEGIN:VCARD\r\nVERSION:3.0\r\nFN:Ana Souza\r\nCATEGORIES:Clients\r\nEND:VCARD\r\n\
             BEGIN:VCARD\r\nVERSION:3.0\r\nFN:Bruno Lima\r\nCATEGORIES:Family,Clients\r\nEND:VCARD\r\n\
             BEGIN:VCARD\r\nVERSION:3.0\r\nFN:Carla\r\nCATEGORIES:Family\r\nEND:VCARD\r\n",
        );
        assert_eq!(groups(&cards), ["Clients", "Family"]);
        let family = in_group(cards.clone(), Some("Family"));
        assert_eq!(family.len(), 2);
        assert_eq!(in_group(cards.clone(), None).len(), 3);
        assert_eq!(
            own_card(&cards, "bruno lima").and_then(|c| c.get("Name")),
            Some("Bruno Lima".into())
        );
        assert!(own_card(&cards, "Unknown").is_none());
    }
}
