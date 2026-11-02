// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Picking a color from the screen through the Screenshot portal, the
//! eyedropper of the Colors window (INT-16). The colors that the user adds
//! to the palette of the color chooser are kept by GTK between sessions.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use gtk::{gdk, gio, glib};

use crate::formatting::labelled;
use crate::i18n::tr;

thread_local! {
    static NEXT_TOKEN: Cell<u32> = const { Cell::new(0) };
}

/// A button that picks a color from the screen for a color button.
pub fn button(target: &gtk::ColorDialogButton) -> gtk::Button {
    let button = gtk::Button::builder()
        .icon_name("color-select-symbolic")
        .tooltip_text(tr("Pick a Color from the Screen"))
        .valign(gtk::Align::Center)
        .css_classes(["flat"])
        .build();
    labelled(&button, &tr("Pick a Color from the Screen"));
    let target = target.downgrade();
    button.connect_clicked(move |_| {
        let target = target.clone();
        pick(move |color| {
            if let Some(target) = target.upgrade() {
                target.set_rgba(&color);
            }
        });
    });
    button
}

/// The path of the request object that the portal makes for a token.
fn request_path(unique_name: &str, token: &str) -> String {
    let sender = unique_name.trim_start_matches(':').replace('.', "_");
    format!("/org/freedesktop/portal/desktop/request/{sender}/{token}")
}

/// The color of a response of the portal, if the user picked one.
fn picked(parameters: &glib::Variant) -> Option<gdk::RGBA> {
    if parameters.n_children() < 2 || parameters.child_value(0).get::<u32>()? != 0 {
        return None;
    }
    let results = glib::VariantDict::new(Some(&parameters.child_value(1)));
    let (r, g, b) = results
        .lookup_value("color", Some(glib::VariantTy::new("(ddd)").ok()?))?
        .get::<(f64, f64, f64)>()?;
    Some(gdk::RGBA::new(r as f32, g as f32, b as f32, 1.0))
}

/// What to do with a picked color.
type Picked = Box<dyn FnOnce(gdk::RGBA)>;

/// Asks the portal for a color of the screen, and calls `f` with it if
/// the user picks one.
fn pick(f: impl FnOnce(gdk::RGBA) + 'static) {
    let Ok(connection) = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE) else {
        return;
    };
    let Some(unique) = connection.unique_name() else {
        return;
    };
    let token = NEXT_TOKEN.with(|n| {
        n.set(n.get() + 1);
        format!("desk_color_{}", n.get())
    });
    let path = request_path(&unique, &token);
    let callback: Rc<RefCell<Option<Picked>>> = Rc::new(RefCell::new(Some(Box::new(f))));
    let subscription: Rc<RefCell<Option<gio::SignalSubscription>>> = Rc::default();
    let kept = subscription.clone();
    *subscription.borrow_mut() = Some(connection.subscribe_to_signal(
        Some("org.freedesktop.portal.Desktop"),
        Some("org.freedesktop.portal.Request"),
        Some("Response"),
        Some(&path),
        None,
        gio::DBusSignalFlags::NONE,
        move |signal| {
            if let Some(color) = picked(signal.parameters)
                && let Some(f) = callback.borrow_mut().take()
            {
                f(color);
            }
            // One response for each request.
            kept.borrow_mut().take();
        },
    ));
    let options = glib::VariantDict::new(None);
    options.insert("handle_token", &token);
    let parameters = glib::Variant::tuple_from_iter(["".to_variant(), options.end()]);
    connection.call(
        Some("org.freedesktop.portal.Desktop"),
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.Screenshot",
        "PickColor",
        Some(&parameters),
        None,
        gio::DBusCallFlags::NONE,
        -1,
        gio::Cancellable::NONE,
        move |result| {
            if let Err(error) = result {
                glib::g_warning!("desk", "Cannot pick a color: {error}");
                subscription.borrow_mut().take();
            }
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn int_16_the_portal_answers_on_the_path_of_the_token() {
        assert_eq!(
            request_path(":1.42", "desk_color_1"),
            "/org/freedesktop/portal/desktop/request/1_42/desk_color_1"
        );
    }

    #[test]
    fn int_16_a_picked_color_comes_from_the_results() {
        let results = glib::VariantDict::new(None);
        results.insert_value("color", &(1.0f64, 0.5f64, 0.0f64).to_variant());
        let response = glib::Variant::tuple_from_iter([0u32.to_variant(), results.end()]);
        let color = picked(&response).unwrap();
        assert_eq!((color.red(), color.green(), color.blue()), (1.0, 0.5, 0.0));
        // Cancelled.
        let results = glib::VariantDict::new(None);
        let response = glib::Variant::tuple_from_iter([1u32.to_variant(), results.end()]);
        assert!(picked(&response).is_none());
    }
}
