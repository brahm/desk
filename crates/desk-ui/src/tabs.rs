// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The window of an app, with its open documents in tabs (window M+,
//! D-001 and D-008 in docs/decisions.md).
//!
//! A [`Frame`] is one `AdwApplicationWindow` whose content is an
//! `AdwTabOverview` over an `AdwTabView`. Each tab holds a whole document:
//! its header bar, its sidebars, the document and the Inspector, so that
//! the code of a document does not need to know that other documents share
//! the window. Each document puts its own `AdwTabBar` under its header bar;
//! all of them show the same tab view, and only the one of the document in
//! sight is on screen.
//!
//! The actions of a document live in a group of its own, which the widgets
//! of its tab find as `win`. The window itself answers the keyboard
//! shortcuts of the app, and puts its actions on the session bus, from its
//! own action map, not from the widget that has the focus. So the window
//! holds a stand-in for each action of the document in sight: activating
//! one runs the action of that document, and each takes its state and
//! whether it is enabled from that document, again whenever the selected
//! tab changes.
//!
//! The shortcuts of the tabs are those of GNOME (D-022, D-023):
//! Ctrl+PageUp and Ctrl+PageDown, Ctrl+Tab, Alt+1 to Alt+9, and with Shift
//! to move a tab. Ctrl+Home and Ctrl+End stay with the text and the cells.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use adw::prelude::*;
use gtk::{gio, glib};

/// What a document gives the frame that holds it.
pub(crate) trait FramePage {
    /// The actions of the document, which the window runs while its tab is
    /// the one in sight.
    fn actions(&self) -> gio::ActionGroup;
    /// Asked when the tab is about to close. `done` is called with true to
    /// close it, at once or after a dialog, or with false to keep it.
    fn ask_close(self: Rc<Self>, done: Box<dyn FnOnce(bool)>);
    /// Called once the tab has closed.
    fn closed(&self) {}
    /// Called when the tab was dragged to another window of the app, which
    /// holds it from now on.
    fn moved_to(&self, _frame: &Rc<Frame>) {}
}

/// What a document puts on the window that holds it: controllers, and
/// handlers of notifications of its properties, which move with the tab
/// to another window, the handlers told of the new one (third review of
/// RV batch 5; one place for both apps since batch 6).
#[derive(Default)]
pub(crate) struct WindowBindings {
    controllers: RefCell<Vec<gtk::EventController>>,
    handlers: RefCell<Vec<WindowNotify>>,
}

/// A property of the window, what follows it, and its handler there.
type WindowNotify = (
    &'static str,
    Rc<dyn Fn(&adw::ApplicationWindow)>,
    glib::SignalHandlerId,
);

impl WindowBindings {
    /// Adds a controller to `window`.
    pub(crate) fn add_controller(
        &self,
        window: &adw::ApplicationWindow,
        controller: impl IsA<gtk::EventController>,
    ) {
        let controller = controller.upcast();
        window.add_controller(controller.clone());
        self.controllers.borrow_mut().push(controller);
    }

    /// Calls `f` whenever `property` of `window` changes.
    pub(crate) fn connect(
        &self,
        window: &adw::ApplicationWindow,
        property: &'static str,
        f: impl Fn(&adw::ApplicationWindow) + 'static,
    ) {
        let f: Rc<dyn Fn(&adw::ApplicationWindow)> = Rc::new(f);
        let id = Self::attach(window, property, &f);
        self.handlers.borrow_mut().push((property, f, id));
    }

    fn attach(
        window: &adw::ApplicationWindow,
        property: &str,
        f: &Rc<dyn Fn(&adw::ApplicationWindow)>,
    ) -> glib::SignalHandlerId {
        let f = f.clone();
        window.connect_notify_local(Some(property), move |window, _| f(window))
    }

    /// Moves to `new` from `old`, and tells each handler of what `new`
    /// shows, which may differ.
    pub(crate) fn move_to(&self, old: &adw::ApplicationWindow, new: &adw::ApplicationWindow) {
        for controller in self.controllers.borrow().iter() {
            old.remove_controller(controller);
            new.add_controller(controller.clone());
        }
        let handlers: Vec<_> = {
            let mut handlers = self.handlers.borrow_mut();
            for (property, f, id) in handlers.iter_mut() {
                let moved = Self::attach(new, property, f);
                old.disconnect(std::mem::replace(id, moved));
            }
            handlers.iter().map(|(_, f, _)| f.clone()).collect()
        };
        for f in handlers {
            f(new);
        }
    }
}

/// A tab, the widget it holds and its document.
type Tab = (adw::TabPage, gtk::Widget, Rc<dyn FramePage>);
/// What opens a new document in a tab and gives its tab back.
type NewTab = Box<dyn Fn() -> Option<adw::TabPage>>;

/// One window of an app and its tabs.
pub(crate) struct Frame {
    window: adw::ApplicationWindow,
    view: adw::TabView,
    overview: adw::TabOverview,
    /// Each tab, the widget it holds and its document.
    pages: RefCell<Vec<Tab>>,
    /// What the New Tab button of the overview opens.
    new_tab: RefCell<Option<NewTab>>,
    /// The actions of the document in sight, and what the window listens
    /// to on them.
    current: RefCell<Option<(gio::ActionGroup, Vec<glib::SignalHandlerId>)>>,
}

thread_local! {
    /// The windows of the app, oldest first.
    static FRAMES: RefCell<Vec<Rc<Frame>>> = const { RefCell::new(Vec::new()) };
}

impl Frame {
    /// A new window of `app`, with no tab yet.
    pub(crate) fn new(app: &adw::Application, title: &str) -> Rc<Self> {
        let view = adw::TabView::new();
        // Ctrl+Home and Ctrl+End go to the start and the end of the text
        // or of the table, not to the first and the last tab (D-023).
        view.remove_shortcuts(
            adw::TabViewShortcuts::CONTROL_HOME
                | adw::TabViewShortcuts::CONTROL_END
                | adw::TabViewShortcuts::CONTROL_SHIFT_HOME
                | adw::TabViewShortcuts::CONTROL_SHIFT_END,
        );
        let overview = adw::TabOverview::builder()
            .view(&view)
            .child(&view)
            .enable_new_tab(true)
            .build();
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title(title)
            .default_width(1280)
            .default_height(720)
            .width_request(360)
            .height_request(294)
            .content(&overview)
            .build();
        let this = Rc::new(Self {
            window,
            view,
            overview,
            pages: RefCell::new(Vec::new()),
            new_tab: RefCell::new(None),
            current: RefCell::new(None),
        });
        this.connect();
        FRAMES.with(|f| f.borrow_mut().push(this.clone()));
        this
    }

    /// The window of the app that is in front, if any.
    pub(crate) fn active(app: &adw::Application) -> Option<Rc<Self>> {
        let active = app.active_window();
        FRAMES.with(|f| {
            let frames = f.borrow();
            active
                .and_then(|w| {
                    frames
                        .iter()
                        .find(|f| f.window.upcast_ref::<gtk::Window>() == &w)
                })
                .or_else(|| frames.last())
                .cloned()
        })
    }

    /// The GTK window.
    pub(crate) fn window(&self) -> &adw::ApplicationWindow {
        &self.window
    }

    /// The tab view, which a document gives to its own tab bar.
    pub(crate) fn view(&self) -> &adw::TabView {
        &self.view
    }

    /// Shows the open documents as a grid (Open documents in the header
    /// bar).
    pub(crate) fn show_overview(&self) {
        self.overview.set_open(true);
    }

    /// Adds a document as a new tab after the one in sight, and shows it.
    pub(crate) fn add(
        self: &Rc<Self>,
        child: &impl IsA<gtk::Widget>,
        page: Rc<dyn FramePage>,
    ) -> adw::TabPage {
        let tab = match self.view.selected_page() {
            Some(selected) => self.view.add_page(child, Some(&selected)),
            None => self.view.append(child),
        };
        // The actions answer inside the tab even before it is selected.
        child
            .as_ref()
            .insert_action_group("win", Some(&page.actions()));
        self.pages
            .borrow_mut()
            .push((tab.clone(), child.as_ref().clone(), page));
        Self::follow_title(&tab);
        self.view.set_selected_page(&tab);
        self.route_actions();
        tab
    }

    /// Closes the tab of `child`, which asks its document first.
    pub(crate) fn close(&self, child: &impl IsA<gtk::Widget>) {
        let child = child.as_ref();
        let tab = self
            .pages
            .borrow()
            .iter()
            .find(|(_, w, _)| w == child)
            .map(|(t, _, _)| t.clone());
        if let Some(tab) = tab {
            self.view.close_page(&tab);
        }
    }

    /// Sets what the New Tab button of the overview opens: a new document
    /// in this window, whose tab the function gives back.
    pub(crate) fn set_new_tab(&self, open: impl Fn() -> Option<adw::TabPage> + 'static) {
        *self.new_tab.borrow_mut() = Some(Box::new(open));
    }

    /// Gives the window the title of a tab while it is the one in sight.
    /// Connected once: the window is the one that holds the tab when the
    /// title changes, as the tab may move (third review of RV batch 5).
    fn follow_title(tab: &adw::TabPage) {
        tab.connect_title_notify(|tab| {
            if tab.is_selected()
                && let Some(window) = tab.child().root().and_downcast::<gtk::Window>()
            {
                window.set_title(Some(&tab.title()));
            }
        });
    }

    fn page_of(&self, tab: &adw::TabPage) -> Option<Rc<dyn FramePage>> {
        self.pages
            .borrow()
            .iter()
            .find(|(t, _, _)| t == tab)
            .map(|(_, _, p)| p.clone())
    }

    /// Points the stand-ins of the window at the actions of the document in
    /// sight, and puts the name of its tab in the title of the window.
    fn route_actions(self: &Rc<Self>) {
        let Some(tab) = self.view.selected_page() else {
            return;
        };
        self.window.set_title(Some(&tab.title()));
        let Some(page) = self.page_of(&tab) else {
            return;
        };
        let group = page.actions();
        if let Some((old, handlers)) = self.current.take() {
            for handler in handlers {
                old.disconnect(handler);
            }
        }
        let names = group.list_actions();
        // The stand-ins of actions that this document lacks do nothing.
        for name in self.window.list_actions() {
            if !names.contains(&name)
                && let Some(action) = self.window.lookup_action(&name)
                && let Ok(action) = action.downcast::<gio::SimpleAction>()
            {
                action.set_enabled(false);
            }
        }
        for name in &names {
            self.stand_in(&group, name);
            self.mirror(&group, name);
        }
        let weak = Rc::downgrade(self);
        let enabled = group.connect_action_enabled_changed(None, move |group, name, _| {
            if let Some(this) = weak.upgrade() {
                this.mirror(group, name);
            }
        });
        let weak = Rc::downgrade(self);
        let state = group.connect_action_state_changed(None, move |group, name, _| {
            if let Some(this) = weak.upgrade() {
                this.mirror(group, name);
            }
        });
        let weak = Rc::downgrade(self);
        let added = group.connect_action_added(None, move |group, name| {
            if let Some(this) = weak.upgrade() {
                this.stand_in(group, name);
                this.mirror(group, name);
            }
        });
        *self.current.borrow_mut() = Some((group, vec![enabled, state, added]));
    }

    /// Adds to the window a stand-in for the action `name` of `group`, if
    /// it has none: it runs the action of the document in sight.
    fn stand_in(self: &Rc<Self>, group: &gio::ActionGroup, name: &str) {
        if self.window.lookup_action(name).is_some() {
            return;
        }
        let parameter = group.action_parameter_type(name);
        let action = match group.action_state(name) {
            Some(state) => gio::SimpleAction::new_stateful(name, parameter.as_deref(), &state),
            None => gio::SimpleAction::new(name, parameter.as_deref()),
        };
        let weak = Rc::downgrade(self);
        let target = name.to_owned();
        action.connect_activate(move |_, value| {
            if let Some(group) = weak.upgrade().and_then(|this| this.current_group()) {
                group.activate_action(&target, value);
            }
        });
        let weak = Rc::downgrade(self);
        let target = name.to_owned();
        action.connect_change_state(move |_, value| {
            if let (Some(group), Some(value)) =
                (weak.upgrade().and_then(|this| this.current_group()), value)
            {
                group.change_action_state(&target, value);
            }
        });
        self.window.add_action(&action);
    }

    /// Gives the stand-in of `name` the state of the action of `group`, and
    /// whether it is enabled.
    fn mirror(&self, group: &gio::ActionGroup, name: &str) {
        let Some(action) = self
            .window
            .lookup_action(name)
            .and_then(|a| a.downcast::<gio::SimpleAction>().ok())
        else {
            return;
        };
        action.set_enabled(group.is_action_enabled(name));
        if let Some(state) = group.action_state(name)
            && action.state().is_some_and(|s| s.type_() == state.type_())
        {
            action.set_state(&state);
        }
    }

    fn current_group(&self) -> Option<gio::ActionGroup> {
        self.current.borrow().as_ref().map(|(g, _)| g.clone())
    }

    fn connect(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        self.overview.connect_create_tab(move |overview| {
            let tab = weak
                .upgrade()
                .and_then(|this| this.new_tab.borrow().as_ref().and_then(|open| open()));
            // The overview needs a tab back; without a way to open one it
            // gives back the tab in sight.
            tab.or_else(|| overview.view().and_then(|v| v.selected_page()))
                .expect("a window always has a tab while its overview is open")
        });
        let weak = Rc::downgrade(self);
        self.view.connect_selected_page_notify(move |_| {
            if let Some(this) = weak.upgrade() {
                this.route_actions();
            }
        });
        // A tab closes once its document says so: at once when nothing
        // would be lost, after the dialog that asks to save otherwise.
        let weak = Rc::downgrade(self);
        self.view.connect_close_page(move |view, tab| {
            let Some(this) = weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            let Some(page) = this.page_of(tab) else {
                view.close_page_finish(tab, true);
                return glib::Propagation::Stop;
            };
            let frame = Rc::downgrade(&this);
            let tab_for_done = tab.clone();
            page.ask_close(Box::new(move |confirm| {
                let Some(this) = frame.upgrade() else { return };
                this.view.close_page_finish(&tab_for_done, confirm);
                if confirm {
                    this.forget(&tab_for_done);
                }
            }));
            glib::Propagation::Stop
        });
        // A tab dragged here from another window of the app is held here
        // from now on: asked before it closes, and running its actions
        // here (review RV, batch 5).
        let weak = Rc::downgrade(self);
        self.view.connect_page_attached(move |_, tab, _| {
            if let Some(this) = weak.upgrade() {
                this.adopt(tab);
            }
        });
        // The window goes with its last tab.
        // Not at once: the last tab dragged to another window leaves this
        // one before it arrives there, and the window that holds it must
        // still be listed then (review of RV batch 5).
        let weak = Rc::downgrade(self);
        self.view.connect_n_pages_notify(move |view| {
            if view.n_pages() == 0 {
                let weak = weak.clone();
                glib::idle_add_local_once(move || {
                    if let Some(this) = weak.upgrade()
                        && this.view.n_pages() == 0
                    {
                        this.window.destroy();
                    }
                });
            }
        });
        // Closing the window closes each tab, which asks its document.
        let weak = Rc::downgrade(self);
        self.window.connect_close_request(move |_| {
            let Some(this) = weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            if this.view.n_pages() == 0 {
                return glib::Propagation::Proceed;
            }
            let tabs: Vec<adw::TabPage> = this
                .pages
                .borrow()
                .iter()
                .map(|(t, _, _)| t.clone())
                .collect();
            for tab in tabs {
                this.view.close_page(&tab);
            }
            glib::Propagation::Stop
        });
        let weak: Weak<Self> = Rc::downgrade(self);
        self.window.connect_destroy(move |_| {
            // The list is gone when the app ends with windows in it.
            let _ = FRAMES.try_with(|f| {
                if let Ok(mut f) = f.try_borrow_mut() {
                    f.retain(|o| weak.upgrade().is_none_or(|me| !Rc::ptr_eq(o, &me)));
                }
            });
        });
    }

    /// Takes over a tab that another window of the app held.
    fn adopt(self: &Rc<Self>, tab: &adw::TabPage) {
        if self.page_of(tab).is_some() {
            return;
        }
        let entry = FRAMES.with(|frames| {
            frames.borrow().iter().find_map(|other| {
                let mut pages = other.pages.borrow_mut();
                let at = pages.iter().position(|(t, _, _)| t == tab)?;
                Some(pages.remove(at))
            })
        });
        let Some(entry) = entry else { return };
        let page = entry.2.clone();
        self.pages.borrow_mut().push(entry);
        // The title already follows the tab, wherever it is.
        if tab.is_selected() {
            self.window.set_title(Some(&tab.title()));
        }
        page.moved_to(self);
        self.route_actions();
    }

    fn forget(&self, tab: &adw::TabPage) {
        let page = {
            let mut pages = self.pages.borrow_mut();
            let at = pages.iter().position(|(t, _, _)| t == tab);
            at.map(|i| pages.remove(i).2)
        };
        if let Some(page) = page {
            page.closed();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    /// A document that counts how it was told to close and to move.
    struct Counted {
        actions: gio::SimpleActionGroup,
        asked: Rc<Cell<u32>>,
        moved: Rc<Cell<u32>>,
    }

    impl FramePage for Counted {
        fn actions(&self) -> gio::ActionGroup {
            self.actions.clone().upcast()
        }

        fn ask_close(self: Rc<Self>, done: Box<dyn FnOnce(bool)>) {
            self.asked.set(self.asked.get() + 1);
            done(false);
        }

        fn moved_to(&self, _frame: &Rc<Frame>) {
            self.moved.set(self.moved.get() + 1);
        }
    }

    #[test]
    fn a_tab_dragged_to_another_window_still_asks_before_it_closes() {
        // Review RV, batch 5: the window a tab was dragged to did not know
        // its document, and closed it without asking to save.
        crate::with_gtk(|| {
            let _ = adw::init();
            let app = adw::Application::builder()
                .application_id("br.com.brahm.Desk.TabsTest")
                .flags(gio::ApplicationFlags::NON_UNIQUE)
                .build();
            let _ = app.register(gio::Cancellable::NONE);
            let (a, b) = (Frame::new(&app, "A"), Frame::new(&app, "B"));
            let asked = Rc::new(Cell::new(0));
            let moved = Rc::new(Cell::new(0));
            let page = Rc::new(Counted {
                actions: gio::SimpleActionGroup::new(),
                asked: asked.clone(),
                moved: moved.clone(),
            });
            let tab = a.add(&gtk::Label::new(Some("x")), page);
            // A second document in A, so that A stays after the first goes.
            let other = a.add(
                &gtk::Label::new(Some("y")),
                Rc::new(Counted {
                    actions: gio::SimpleActionGroup::new(),
                    asked: Rc::new(Cell::new(0)),
                    moved: Rc::new(Cell::new(0)),
                }),
            );
            a.view.transfer_page(&tab, &b.view, 0);
            assert!(a.page_of(&tab).is_none());
            assert!(b.page_of(&tab).is_some());
            assert_eq!(moved.get(), 1);
            b.view.close_page(&tab);
            assert_eq!(asked.get(), 1);
            assert_eq!(b.view.n_pages(), 1);
            // The last tab of a window: the window empties before the tab
            // arrives in the other (second review of RV batch 5).
            a.view.transfer_page(&other, &b.view, 0);
            assert_eq!(a.view.n_pages(), 0);
            assert!(b.page_of(&other).is_some());
            // The title of the tab in sight is that of the window it is in
            // now, and B follows it alone (third review).
            b.view.set_selected_page(&other);
            other.set_title("moved");
            assert_eq!(b.window.title().as_deref(), Some("moved"));
            a.window.set_title(Some("A"));
            other.set_title("again");
            assert_eq!(a.window.title().as_deref(), Some("A"));
            a.window.destroy();
            b.window.destroy();
        });
    }

    #[test]
    fn what_a_document_follows_of_its_window_moves_with_it() {
        // Third review of RV batch 5: a tab dragged to another window
        // followed full screen in the window it had left.
        crate::with_gtk(|| {
            let _ = adw::init();
            let (a, b) = (
                adw::ApplicationWindow::builder().build(),
                adw::ApplicationWindow::builder().build(),
            );
            let seen = Rc::new(RefCell::new(Vec::new()));
            let notifies = WindowBindings::default();
            let kept = seen.clone();
            notifies.connect(&a, "title", move |w| {
                kept.borrow_mut()
                    .push(w.title().map(String::from).unwrap_or_default());
            });
            a.set_title(Some("a1"));
            let keys = gtk::EventControllerKey::new();
            notifies.add_controller(&a, keys.clone());
            let holds = |w: &adw::ApplicationWindow| {
                let list = w.observe_controllers();
                (0..list.n_items()).any(|i| list.item(i).as_ref() == Some(keys.upcast_ref()))
            };
            assert!(holds(&a));
            notifies.move_to(&a, &b);
            // The controllers go along.
            assert!(!holds(&a) && holds(&b));
            a.set_title(Some("a2"));
            b.set_title(Some("b1"));
            // Told of the new window when it moves, then of its changes
            // alone.
            assert_eq!(*seen.borrow(), ["a1", "", "b1"]);
            a.destroy();
            b.destroy();
        });
    }
}
