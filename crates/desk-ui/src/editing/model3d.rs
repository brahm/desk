// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! 3D objects on the page (W116, PXV-25): put there from a file, and
//! turned by dragging the handle in the middle of a selected one, as in
//! Pages 15.4, or with the rows of the Object tab. While the handle is
//! dragged the model turns on the page; the turn is one change, made
//! when the drag ends. A model that moves, as a glTF may, plays its
//! animation on the page, over and over until it is paused; playing it
//! changes nothing in the document.

use desk_layout::PlacedObject;
use desk_model::{Anchor, Model3d, NodeId, Object, ObjectKind};
use gtk::prelude::*;

use super::TextEditor;
use super::objects::find;

/// How large a new 3D object stands on the page, in points.
const NEW_MODEL: f64 = 240.0;

/// The radius of the handle that turns a selected 3D object, in pixels.
pub(crate) const HANDLE_RADIUS: f64 = 14.0;

/// How far a model turns for each pixel the handle is dragged, in
/// hundredths of a degree: half a degree.
const TURN_PER_PIXEL: f64 = 50.0;

/// A drag of the handle of a 3D object.
#[derive(Clone, Debug)]
pub(super) struct ModelTurn {
    id: NodeId,
    page: usize,
    /// Where it started, in points on the page.
    start: (f32, f32),
    /// The model as it was.
    model: Model3d,
    /// The model as the drag turns it.
    turned: Model3d,
}

/// The animation of a 3D object as it plays on the page.
pub(super) struct ModelPlaying {
    /// The object.
    id: NodeId,
    /// The page it is on, by index.
    page: usize,
    /// How long the animation lasts, in milliseconds.
    duration: u32,
    /// When it started, in microseconds of the clock of the frames.
    started: Option<i64>,
    /// The callback that draws each frame.
    tick: Option<gtk::TickCallbackId>,
}

/// Where the handle that turns a 3D object is, in points on its page; none
/// for another object.
pub(super) fn model_handle(placed: &PlacedObject) -> Option<(f32, f32)> {
    placed.object.model()?;
    Some(placed.center())
}

impl TextEditor {
    /// Puts a 3D model on a page, centered on `at` or in the middle of the
    /// page shown, and selects it (W116).
    pub fn insert_model(&self, model: Model3d, at: Option<(usize, f64, f64)>) {
        let (page, frame) = self.placed_frame(
            |room| {
                let side = NEW_MODEL.min(room.0).min(room.1);
                (side, side)
            },
            at,
        );
        self.leave_story(None);
        let selection = self.selection();
        let result = self
            .state
            .borrow_mut()
            .editor
            .edit("Insert 3D Object", |edit| {
                let id = edit.new_id();
                let mut object = Object::new(id, ObjectKind::Model(model), frame);
                object.anchor = Anchor::Page(page as u32);
                edit.add_object(object)?;
                Ok(id)
            });
        match result {
            Ok(id) => self.select_new_object(selection, id),
            Err(error) => gtk::glib::g_warning!("desk", "cannot insert: {error}"),
        }
    }

    /// The selected 3D model, when one alone is selected.
    pub fn selected_model(&self) -> Option<(NodeId, Model3d)> {
        let info = self.object_info()?;
        (info.count == 1)
            .then(|| info.object.model().map(|m| (info.object.id, m.clone())))
            .flatten()
    }

    /// Turns the selected 3D model to a yaw and a pitch, in hundredths of
    /// a degree.
    pub fn turn_selected_model(&self, name: &str, yaw: i32, pitch: i32) {
        let Some((_, model)) = self.selected_model() else {
            return;
        };
        let turned = model.turned(yaw, pitch);
        if turned == model {
            return;
        }
        self.set_object(name, move |object| {
            if let ObjectKind::Model(m) = &mut object.kind {
                *m = turned;
            }
        });
    }

    /// Shows the selected 3D model from the front again.
    pub fn reset_model_rotation(&self) {
        self.turn_selected_model("Reset Rotation", 0, 0);
    }

    /// How long the animation of the selected 3D model lasts, in
    /// milliseconds; none when it does not move.
    pub fn selected_model_animation(&self) -> Option<u32> {
        let (_, model) = self.selected_model()?;
        let seconds = model.scene().ok()?.animation?.duration();
        Some((seconds * 1000.0).round().clamp(1.0, u32::MAX as f32) as u32)
    }

    /// Whether the animation of a 3D model plays.
    pub fn model_playing(&self) -> bool {
        self.state.borrow().model_playing.is_some()
    }

    /// Plays the animation of the selected 3D model, or pauses the one
    /// that plays.
    pub fn play_model_animation(&self) {
        if self.model_playing() {
            self.stop_model_animation();
            return;
        }
        let (Some((id, _)), Some(duration)) =
            (self.selected_model(), self.selected_model_animation())
        else {
            return;
        };
        let Some(page) = find(&self.state.borrow().layout, id).map(|(p, _)| p) else {
            return;
        };
        let weak = self.downgrade();
        let tick = self.view.area().add_tick_callback(move |_, clock| {
            let Some(this) = weak.upgrade() else {
                return gtk::glib::ControlFlow::Break;
            };
            if this.model_frame(clock.frame_time()) {
                gtk::glib::ControlFlow::Continue
            } else {
                // The callback ends here: it is not removed again.
                if let Some(playing) = this.state.borrow_mut().model_playing.as_mut() {
                    playing.tick = None;
                }
                this.stop_model_animation();
                gtk::glib::ControlFlow::Break
            }
        });
        self.state.borrow_mut().model_playing = Some(ModelPlaying {
            id,
            page,
            duration,
            started: None,
            tick: Some(tick),
        });
        self.changed();
    }

    /// Stops the animation of a 3D model, which shows at rest again.
    pub fn stop_model_animation(&self) {
        let Some(mut playing) = self.state.borrow_mut().model_playing.take() else {
            return;
        };
        if let Some(tick) = playing.tick.take() {
            tick.remove();
        }
        self.show_pages();
        self.changed();
    }

    /// Draws the page of the 3D model being played as it is at a time of
    /// the clock. Returns whether the animation goes on: not once the
    /// model is gone.
    fn model_frame(&self, now: i64) -> bool {
        let (id, page, moment) = {
            let mut state = self.state.borrow_mut();
            let Some(playing) = state.model_playing.as_mut() else {
                return false;
            };
            let started = *playing.started.get_or_insert(now);
            let elapsed = ((now - started).max(0) / 1000) as u64;
            let moment = (elapsed % u64::from(playing.duration.max(1))) as u32;
            (playing.id, playing.page, moment)
        };
        let list = {
            let state = self.state.borrow();
            let layout = Self::main_layout(&state);
            let Some(shown) = layout.pages.get(page) else {
                return false;
            };
            let mut shown = shown.clone();
            let Some(placed) = shown.objects.iter_mut().find(|o| o.object.id == id) else {
                return false;
            };
            let Some(model) = placed.object.model() else {
                return false;
            };
            placed.object.kind = ObjectKind::Model(model.at(Some(moment)));
            desk_render::DisplayList::from_page(layout, &shown)
        };
        self.view.set_page(page, list);
        true
    }

    /// Starts turning the selected 3D model when a press at a point of a
    /// page lands on its handle.
    pub(super) fn press_model_handle(&self, page: usize, x: f32, y: f32) -> bool {
        let reach = (HANDLE_RADIUS / self.view.scale()) as f32;
        let turn = {
            let state = self.state.borrow();
            let [id] = state.objects[..] else {
                return false;
            };
            let Some((on, placed)) = find(&state.layout, id) else {
                return false;
            };
            let (Some(model), Some((hx, hy))) = (placed.object.model(), model_handle(placed))
            else {
                return false;
            };
            if on != page || placed.object.locked || (hx - x).hypot(hy - y) > reach {
                return false;
            }
            ModelTurn {
                id,
                page,
                start: (x, y),
                model: model.clone(),
                turned: model.clone(),
            }
        };
        self.state.borrow_mut().model_turn = Some(turn);
        true
    }

    /// Turns the model of the handle being dragged, to a point of the
    /// area, and shows it turned. Whether a handle is being dragged.
    pub(super) fn drag_model(&self, x: f64, y: f64) -> bool {
        let Some(turn) = self.state.borrow().model_turn.clone() else {
            return false;
        };
        let Some((ox, oy)) = self.view.to_area(turn.page, 0.0, 0.0) else {
            return true;
        };
        let scale = self.view.scale();
        // In pixels, so that the model turns as fast at any zoom.
        let dx = x - ox - f64::from(turn.start.0) * scale;
        let dy = y - oy - f64::from(turn.start.1) * scale;
        // Dragged right, the front goes right and the left side comes forward.
        let yaw = f64::from(turn.model.yaw) - dx * TURN_PER_PIXEL;
        let pitch = f64::from(turn.model.pitch) + dy * TURN_PER_PIXEL;
        let turned = turn.model.turned(yaw.round() as i32, pitch.round() as i32);
        if turned == turn.turned {
            return true;
        }
        {
            let mut state = self.state.borrow_mut();
            for page in &mut state.layout.pages {
                for placed in page.objects.iter_mut().filter(|o| o.object.id == turn.id) {
                    placed.object.kind = ObjectKind::Model(turned.clone());
                }
            }
            if let Some(t) = &mut state.model_turn {
                t.turned = turned;
            }
        }
        self.show_pages();
        true
    }

    /// Ends a drag of the handle of a 3D model, turning it as the drag
    /// left it, as one change.
    pub(super) fn end_model_drag(&self) {
        let Some(turn) = self.state.borrow_mut().model_turn.take() else {
            return;
        };
        if turn.turned != turn.model {
            self.turn_selected_model("Rotate 3D Object", turn.turned.yaw, turn.turned.pitch);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube() -> Model3d {
        let path = format!(
            "{}/../desk-3d/tests/data/cube-usdc.usdz",
            env!("CARGO_MANIFEST_DIR")
        );
        Model3d::from_bytes(std::fs::read(path).unwrap()).unwrap()
    }

    #[test]
    fn w116_the_handle_of_a_3d_object_turns_it_as_one_change() {
        crate::with_gtk(|| {
            let editor = TextEditor::new(
                &crate::view::DocumentView::new(),
                desk_model::Document::new_word_processing(),
            );
            editor.insert_model(cube(), None);
            let (id, model) = editor.selected_model().unwrap();
            assert_eq!((model.yaw, model.pitch), (0, 0));
            let (page, (hx, hy)) = {
                let state = editor.state.borrow();
                let (page, placed) = find(&state.layout, id).unwrap();
                (page, model_handle(placed).unwrap())
            };
            // Away from the handle, the press is the object's.
            assert!(!editor.press_model_handle(page, hx + 60.0, hy));
            assert!(editor.press_model_handle(page, hx, hy));
            let scale = editor.view.scale();
            let (ox, oy) = editor.view.to_area(page, 0.0, 0.0).unwrap();
            let at = |dx: f64, dy: f64| {
                (
                    ox + (f64::from(hx) * scale) + dx,
                    oy + (f64::from(hy) * scale) + dy,
                )
            };
            let (x, y) = at(40.0, 0.0);
            assert!(editor.drag_model(x, y));
            let (x, y) = at(90.0, -20.0);
            assert!(editor.drag_model(x, y));
            // Shown turned on the page, not yet in the document.
            let shown = |editor: &TextEditor| {
                let state = editor.state.borrow();
                find(&state.layout, id)
                    .unwrap()
                    .1
                    .object
                    .model()
                    .map(|m| (m.yaw, m.pitch))
            };
            assert_eq!(shown(&editor), Some((-4500, -1000)));
            assert_eq!(editor.selected_model().unwrap().1.yaw, 0);
            editor.end_model_drag();
            assert_eq!(
                editor.selected_model().map(|(_, m)| (m.yaw, m.pitch)),
                Some((-4500, -1000))
            );
            editor.reset_model_rotation();
            assert_eq!(editor.selected_model().map(|(_, m)| m.yaw), Some(0));
            editor.undo();
            editor.undo();
            assert_eq!(editor.selected_model().map(|(_, m)| m.yaw), Some(0));
            editor.redo();
            assert_eq!(editor.selected_model().map(|(_, m)| m.yaw), Some(-4500));
        });
    }

    #[test]
    fn w116_a_model_that_moves_plays_and_pauses_without_a_change() {
        crate::with_gtk(|| {
            let editor = TextEditor::new(
                &crate::view::DocumentView::new(),
                desk_model::Document::new_word_processing(),
            );
            editor.insert_model(cube(), None);
            // A cube of USD that rests has no animation to play.
            assert_eq!(editor.selected_model_animation(), None);
            editor.play_model_animation();
            assert!(!editor.model_playing());
            let path = format!(
                "{}/../desk-3d/tests/data/animated.glb",
                env!("CARGO_MANIFEST_DIR")
            );
            let moving = Model3d::from_bytes(std::fs::read(path).unwrap()).unwrap();
            editor.insert_model(moving, None);
            assert_eq!(editor.selected_model_animation(), Some(2000));
            let before = editor.state.borrow().editor.document().clone();
            editor.play_model_animation();
            assert!(editor.model_playing());
            assert!(editor.model_frame(0) && editor.model_frame(1_500_000));
            let (id, _) = editor.selected_model().unwrap();
            assert_eq!(
                editor.state.borrow().editor.document().clone(),
                before,
                "playing changes nothing"
            );
            assert!(find(&editor.state.borrow().layout, id).is_some());
            editor.play_model_animation();
            assert!(!editor.model_playing());
        });
    }
}
