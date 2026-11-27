// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The drawings of Pages 15.4 on the page (W117, PXV-26). Insert >
//! Drawing, or Edit Drawing on a selected one, turns drawing on: a drag
//! with the pointer or a stylus draws a line with the tool, the colour and
//! the width of the bar of drawing, a click with the fill fills the area
//! a line closes, and the eraser takes out the lines it goes over. Each
//! line is one step of the history. Done, Esc or Return turn drawing off
//! and select the drawing. The Drawing tab plays its animation, which
//! draws its lines again over the page.

use std::sync::Arc;

use desk_model::{
    Anchor, Drawing, DrawingAnimation, DrawingTool, Edit, Frame, Length, ModelError, NewStroke,
    NodeId, Object, ObjectKind, PathSegment, Rgba,
};
use gtk::gdk;
use gtk::prelude::*;

use super::TextEditor;
use super::objects::find;
use crate::view::PathMark;

/// What a press does while drawing: draw with a tool, or erase.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrawingPick {
    /// Draws with a tool.
    Tool(DrawingTool),
    /// Takes out the lines it goes over.
    Eraser,
}

/// The tool, the colour and the width the next line is drawn with, as the
/// bar of drawing shows them; they stay from one drawing to the next.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DrawingTools {
    /// What a press does.
    pub pick: DrawingPick,
    /// The colour of the lines and fills.
    pub color: Rgba,
    /// The width of the lines, in points.
    pub width: f64,
}

impl Default for DrawingTools {
    fn default() -> Self {
        Self {
            pick: DrawingPick::Tool(DrawingTool::Pen),
            color: Rgba::BLACK,
            width: 3.0,
        }
    }
}

/// The widths a line can have, in points.
pub const DRAWING_WIDTHS: std::ops::RangeInclusive<f64> = 0.5..=50.0;

/// Drawing, while it is on.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Drawn {
    /// The drawing drawn on; none until the first line makes one.
    target: Option<NodeId>,
    /// The line being drawn, or the way of the eraser, in points on its
    /// page.
    line: Vec<(f64, f64)>,
    /// The page of the line being drawn.
    page: Option<usize>,
    /// Where the press that started it was, in the view.
    pressed: Option<(f64, f64)>,
}

/// A drawing being animated (W117).
pub(super) struct Playing {
    /// The drawing.
    id: NodeId,
    /// The page it is on, by index.
    page: usize,
    /// When it started, in microseconds of the clock of the frames.
    started: Option<i64>,
    /// The callback that draws each frame.
    tick: Option<gtk::TickCallbackId>,
}

impl Playing {
    /// The drawing.
    pub(super) fn id(&self) -> NodeId {
        self.id
    }
}

/// How far a line goes between the points it keeps, in pixels: nearer
/// points are left out.
const STEP: f64 = 1.5;

/// How near the eraser takes out a line, in pixels, beyond its width.
const ERASER_REACH: f64 = 6.0;

impl TextEditor {
    /// Turns drawing on to make a drawing, from Insert > Drawing (W117).
    pub fn start_drawing(&self) {
        self.start_drawing_on(None);
    }

    /// Turns drawing on to draw on the selected drawing (Edit Drawing).
    /// Returns whether a drawing alone is selected, and unlocked.
    pub fn edit_drawing(&self) -> bool {
        let Some(info) = self.object_info() else {
            return false;
        };
        if info.count != 1 || info.object.locked || info.object.drawing().is_none() {
            return false;
        }
        self.start_drawing_on(Some(info.object.id));
        true
    }

    fn start_drawing_on(&self, target: Option<NodeId>) {
        self.stop_drawing_animation();
        self.stop_instant_alpha();
        self.leave_story(None);
        {
            let mut state = self.state.borrow_mut();
            state.pen = None;
            state.objects.clear();
            state.drawn = Some(Drawn {
                target,
                ..Drawn::default()
            });
        }
        self.view.set_path_mark(None);
        self.view.area().set_cursor_from_name(Some("crosshair"));
        self.show_objects();
        self.selection_changed();
        self.changed();
    }

    /// Whether drawing is on.
    pub fn drawing_on(&self) -> bool {
        self.state.borrow().drawn.is_some()
    }

    /// The tool, the colour and the width of the next line.
    pub fn drawing_tools(&self) -> DrawingTools {
        self.state.borrow().drawing_tools
    }

    /// Changes the tool, the colour or the width of the next line.
    pub fn set_drawing_tools(&self, tools: DrawingTools) {
        let tools = DrawingTools {
            width: tools
                .width
                .clamp(*DRAWING_WIDTHS.start(), *DRAWING_WIDTHS.end()),
            ..tools
        };
        if self.state.borrow().drawing_tools != tools {
            self.state.borrow_mut().drawing_tools = tools;
            self.changed();
        }
    }

    /// Turns drawing off: the drawing drawn on, if any, is selected.
    pub fn finish_drawing(&self) {
        let Some(drawn) = self.state.borrow_mut().drawn.take() else {
            return;
        };
        self.view.set_path_mark(None);
        self.view.area().set_cursor_from_name(None);
        let target = drawn.target.filter(|&id| self.drawing_object(id).is_some());
        self.state.borrow_mut().objects = target.into_iter().collect();
        self.show_objects();
        self.selection_changed();
        self.changed();
    }

    /// The drawing `id`, as the document has it.
    fn drawing_object(&self, id: NodeId) -> Option<Object> {
        let state = self.state.borrow();
        let object = state.editor.document().objects().get(id)?;
        object.drawing().is_some().then(|| object.clone())
    }

    /// Changes the document as one step of the history, keeping the
    /// selection as it is.
    fn edit_drawn<T>(
        &self,
        name: &str,
        change: impl FnOnce(&mut Edit) -> Result<T, ModelError>,
    ) -> Option<T> {
        let selection = self.selection();
        let depth = self.state.borrow().editor.recorded();
        let result = self.state.borrow_mut().editor.edit(name, change);
        self.state.borrow_mut().typing = false;
        match result {
            Ok(value) => {
                if self.state.borrow().editor.recorded() > depth {
                    {
                        let mut state = self.state.borrow_mut();
                        let action = state.next_action;
                        state.next_action += 1;
                        state.undo.push((selection, selection, action));
                        state.redo.clear();
                    }
                    self.after_change(selection);
                }
                Some(value)
            }
            Err(error) => {
                gtk::glib::g_warning!("desk", "cannot draw: {error}");
                None
            }
        }
    }

    /// The point of the frame of the drawing drawn on at a point of a
    /// page, or none when it is on another page or none is drawn on.
    fn on_target(&self, page: usize, px: f64, py: f64) -> Option<(NodeId, (f64, f64))> {
        let state = self.state.borrow();
        let id = state.drawn.as_ref()?.target?;
        let (on, placed) = find(&state.layout, id)?;
        (on == page).then(|| (id, super::points::to_frame(placed, (px, py))))
    }

    /// A press while drawing: the start of a line or of the way of the
    /// eraser, or a fill. Returns whether the press is used.
    pub(super) fn press_drawing(&self, x: f64, y: f64) -> bool {
        if !self.drawing_on() {
            return false;
        }
        if let Some((page, px, py)) = self.view.page_at(x, y) {
            self.press_drawing_at(page, px, py, (x, y));
        }
        true
    }

    /// A press while drawing at a point of a page, from a point of the
    /// view.
    fn press_drawing_at(&self, page: usize, px: f64, py: f64, (x, y): (f64, f64)) {
        // A drawing drawn on stays on its page.
        let target_page = {
            let state = self.state.borrow();
            let target = state.drawn.as_ref().and_then(|d| d.target);
            target.and_then(|id| find(&state.layout, id).map(|(p, _)| p))
        };
        if target_page.is_some_and(|p| p != page) {
            return;
        }
        let tools = self.drawing_tools();
        if tools.pick == DrawingPick::Tool(DrawingTool::Fill) {
            if let Some((id, at)) = self.on_target(page, px, py) {
                self.edit_drawn("Fill", |edit| {
                    edit.fill_drawing_at(id, at, tools.color).map(|_| ())
                });
            }
            return;
        }
        if let Some(drawn) = self.state.borrow_mut().drawn.as_mut() {
            drawn.line = vec![(px, py)];
            drawn.page = Some(page);
            drawn.pressed = Some((x, y));
        }
        self.show_drawn();
    }

    /// A drag while drawing draws the line on, or the way of the eraser.
    /// Returns whether the drag is used.
    pub(super) fn drag_drawing(&self, dx: f64, dy: f64) -> bool {
        let (pressed, page) = match self.state.borrow().drawn.as_ref() {
            Some(drawn) => (drawn.pressed, drawn.page),
            None => return false,
        };
        let (Some((x, y)), Some(page)) = (pressed, page) else {
            return true;
        };
        let Some((on, px, py)) = self.view.page_at(x + dx, y + dy) else {
            return true;
        };
        if on != page {
            return true;
        }
        let step = STEP / self.view.scale();
        if let Some(drawn) = self.state.borrow_mut().drawn.as_mut()
            && drawn.line.len() < desk_model::MAX_STROKE_POINTS
            && drawn
                .line
                .last()
                .is_none_or(|&(lx, ly)| (lx - px).hypot(ly - py) >= step)
        {
            drawn.line.push((px, py));
        }
        self.show_drawn();
        true
    }

    /// The end of a drag while drawing: the line goes into the drawing,
    /// which the first line makes, or the eraser takes out the lines it
    /// went over.
    pub(super) fn end_drawing_drag(&self) {
        let (line, page, target) = {
            let mut state = self.state.borrow_mut();
            let Some(drawn) = state.drawn.as_mut() else {
                return;
            };
            if drawn.pressed.take().is_none() {
                return;
            }
            (
                std::mem::take(&mut drawn.line),
                drawn.page.take(),
                drawn.target,
            )
        };
        self.view.set_path_mark(None);
        let (Some(page), false) = (page, line.is_empty()) else {
            return;
        };
        let tools = self.drawing_tools();
        let target = target.filter(|&id| self.drawing_object(id).is_some());
        match (tools.pick, target) {
            (DrawingPick::Eraser, Some(id)) => self.erase(id, &line),
            (DrawingPick::Eraser, None) => {}
            (DrawingPick::Tool(tool), Some(id)) => {
                let points = {
                    let state = self.state.borrow();
                    let Some((_, placed)) = find(&state.layout, id) else {
                        return;
                    };
                    line.iter()
                        .map(|&p| super::points::to_frame(placed, p))
                        .collect()
                };
                let stroke = NewStroke {
                    tool,
                    color: tools.color,
                    width: Length::from_pt(tools.width),
                    points,
                };
                self.edit_drawn("Draw", |edit| edit.add_drawing_stroke(id, stroke));
            }
            (DrawingPick::Tool(tool), None) => {
                let stroke = NewStroke {
                    tool,
                    color: tools.color,
                    width: Length::from_pt(tools.width),
                    points: line,
                };
                let Some(((x, y, w, h), strokes)) = desk_model::drawing_frame(&[stroke]) else {
                    return;
                };
                let made = self.edit_drawn("Draw", |edit| {
                    let id = edit.new_id();
                    let drawing = Drawing {
                        strokes: strokes.into(),
                        animation: None,
                    };
                    let mut object = Object::new(
                        id,
                        ObjectKind::Drawing(Arc::new(drawing)),
                        Frame::from_pt(x, y, w, h),
                    );
                    object.anchor = Anchor::Page(page as u32);
                    edit.add_object(object)?;
                    Ok(id)
                });
                if let (Some(id), Some(drawn)) = (made, self.state.borrow_mut().drawn.as_mut()) {
                    drawn.target = Some(id);
                }
            }
        }
    }

    /// Takes out of the drawing `id` the lines that the eraser went over,
    /// in points on its page.
    fn erase(&self, id: NodeId, way: &[(f64, f64)]) {
        let hit: Vec<usize> = {
            let state = self.state.borrow();
            let Some((_, placed)) = find(&state.layout, id) else {
                return;
            };
            let Some(drawing) = placed.object.drawing() else {
                return;
            };
            let (w, h) = (f64::from(placed.width), f64::from(placed.height));
            let reach = ERASER_REACH / self.view.scale();
            let way: Vec<(f64, f64)> = way
                .iter()
                .map(|&p| super::points::to_frame(placed, p))
                .collect();
            let lines: Vec<usize> = drawing
                .strokes
                .iter()
                .enumerate()
                .filter(|(_, s)| {
                    s.tool != DrawingTool::Fill && way.iter().any(|&p| s.touches(p, reach, w, h))
                })
                .map(|(i, _)| i)
                .collect();
            if lines.is_empty() {
                // A fill goes when the eraser touches no line: the
                // frontmost fill under it.
                drawing
                    .strokes
                    .iter()
                    .rposition(|s| {
                        s.tool == DrawingTool::Fill && way.iter().any(|&p| s.encloses(p, w, h))
                    })
                    .into_iter()
                    .collect()
            } else {
                lines
            }
        };
        if !hit.is_empty() {
            self.edit_drawn("Erase", |edit| edit.remove_drawing_strokes(id, &hit));
        }
    }

    /// Shows the line being drawn as it will be, or the way of the eraser.
    fn show_drawn(&self) {
        let tools = self.drawing_tools();
        let scale = self.view.scale();
        let mark = self.state.borrow().drawn.as_ref().and_then(|drawn| {
            let page = drawn.page?;
            let (&(x0, y0), rest) = drawn.line.split_first()?;
            let mut outline = vec![PathSegment::Move(x0, y0)];
            outline.extend(rest.iter().map(|&(x, y)| PathSegment::Line(x, y)));
            if rest.is_empty() {
                outline.push(PathSegment::Line(x0 + 0.01, y0));
            }
            let ink = match tools.pick {
                DrawingPick::Tool(_) => (tools.color, tools.width),
                // The eraser shows its way in a light gray, as wide as it
                // reaches on the screen.
                DrawingPick::Eraser => (Rgba::rgb(0xc0, 0xc0, 0xc0), 2.0 * ERASER_REACH / scale),
            };
            Some(PathMark {
                page,
                outline,
                points: Vec::new(),
                handles: Vec::new(),
                ink: Some(ink),
            })
        });
        self.view.set_path_mark(mark);
    }

    /// The keys while drawing: Esc and Return turn it off. Returns whether
    /// the key is used.
    pub(super) fn drawing_key(&self, key: gdk::Key) -> bool {
        if !self.drawing_on() {
            return false;
        }
        match key {
            gdk::Key::Escape | gdk::Key::Return | gdk::Key::KP_Enter => {
                self.finish_drawing();
                true
            }
            _ => false,
        }
    }

    /// The selected drawing, when one alone is selected.
    pub fn selected_drawing(&self) -> Option<(NodeId, Drawing)> {
        let info = self.object_info()?;
        if info.count != 1 {
            return None;
        }
        let drawing = info.object.drawing()?.clone();
        Some((info.object.id, drawing))
    }

    /// Animates the selected drawing as `animation` says, or stops
    /// animating it, as one step of the history.
    pub fn set_selected_drawing_animation(&self, animation: Option<DrawingAnimation>) {
        let Some((id, drawing)) = self.selected_drawing() else {
            return;
        };
        if drawing.animation == animation {
            return;
        }
        self.arrange_objects("Animate Drawing", |edit, _| {
            edit.set_drawing_animation(id, animation)?;
            Ok(None)
        });
    }

    /// Plays the animation of the selected drawing over the page: its
    /// lines drawn again in their order, once or until it is stopped.
    pub fn play_drawing(&self) {
        self.stop_drawing_animation();
        let Some((id, _)) = self.selected_drawing() else {
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
            if this.drawing_frame(clock.frame_time()) {
                gtk::glib::ControlFlow::Continue
            } else {
                // The callback ends here: it is not removed again.
                if let Some(playing) = this.state.borrow_mut().playing.as_mut() {
                    playing.tick = None;
                }
                this.stop_drawing_animation();
                gtk::glib::ControlFlow::Break
            }
        });
        self.state.borrow_mut().playing = Some(Playing {
            id,
            page,
            started: None,
            tick: Some(tick),
        });
        self.changed();
    }

    /// Whether the animation of a drawing is playing.
    pub fn drawing_playing(&self) -> bool {
        self.state.borrow().playing.is_some()
    }

    /// Stops the animation of a drawing, which shows whole again.
    pub fn stop_drawing_animation(&self) {
        let Some(mut playing) = self.state.borrow_mut().playing.take() else {
            return;
        };
        if let Some(tick) = playing.tick.take() {
            tick.remove();
        }
        self.show_pages();
        self.changed();
    }

    /// Draws the page of the drawing being animated as it is at a time of
    /// the clock. Returns whether the animation goes on.
    fn drawing_frame(&self, now: i64) -> bool {
        let (id, page, started) = {
            let mut state = self.state.borrow_mut();
            let Some(playing) = state.playing.as_mut() else {
                return false;
            };
            let started = *playing.started.get_or_insert(now);
            (playing.id, playing.page, started)
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
            let Some(drawing) = placed.object.drawing() else {
                return false;
            };
            let animation = drawing.animation.unwrap_or_default();
            let elapsed = (now - started).max(0) as f64 / 1000.0;
            let duration = f64::from(animation.duration.max(1));
            let progress = if animation.repeat {
                (elapsed % duration) / duration
            } else {
                elapsed / duration
            };
            if !animation.repeat && progress >= 1.0 {
                return false;
            }
            let (w, h) = (f64::from(placed.width), f64::from(placed.height));
            let part = drawing.drawn_to(progress, w, h);
            placed.object.kind = ObjectKind::Drawing(Arc::new(part));
            desk_render::DisplayList::from_page(layout, &shown)
        };
        self.view.set_page(page, list);
        true
    }
}

#[cfg(test)]
mod tests {
    use desk_model::Document;

    use super::*;

    /// Draws a line through points of the first page, as a press and a
    /// drag do.
    fn draw(editor: &TextEditor, line: &[(f64, f64)]) {
        let (&(x, y), rest) = line.split_first().unwrap();
        editor.press_drawing_at(0, x, y, (0.0, 0.0));
        if let Some(drawn) = editor.state.borrow_mut().drawn.as_mut() {
            drawn.line.extend_from_slice(rest);
        }
        editor.end_drawing_drag();
    }

    fn drawings(editor: &TextEditor) -> Vec<Object> {
        let state = editor.state.borrow();
        state
            .editor
            .document()
            .objects()
            .iter()
            .filter(|o| o.drawing().is_some())
            .cloned()
            .collect()
    }

    #[test]
    fn w117_lines_drawn_make_one_drawing_drawn_on_again() {
        crate::with_gtk(|| {
            let editor = TextEditor::new(
                &crate::view::DocumentView::new(),
                Document::new_word_processing(),
            );
            editor.start_drawing();
            assert!(editor.drawing_on());
            draw(
                &editor,
                &[
                    (100.0, 100.0),
                    (200.0, 100.0),
                    (200.0, 200.0),
                    (100.0, 200.0),
                    (100.0, 101.0),
                ],
            );
            editor.set_drawing_tools(DrawingTools {
                pick: DrawingPick::Tool(DrawingTool::Crayon),
                color: Rgba::rgb(0, 0, 0xff),
                width: 6.0,
            });
            draw(&editor, &[(300.0, 300.0), (320.0, 330.0)]);
            let made = drawings(&editor);
            assert_eq!(made.len(), 1, "both lines are in one drawing");
            let drawing = made[0].drawing().unwrap();
            assert_eq!(drawing.strokes.len(), 2);
            assert_eq!(drawing.strokes[1].tool, DrawingTool::Crayon);
            assert_eq!(drawing.strokes[1].width, Length::from_pt(6.0));
            // The fill fills the square of the first line.
            editor.set_drawing_tools(DrawingTools {
                pick: DrawingPick::Tool(DrawingTool::Fill),
                color: Rgba::rgb(0xff, 0, 0),
                width: 6.0,
            });
            editor.press_drawing_at(0, 150.0, 150.0, (0.0, 0.0));
            let drawing = drawings(&editor)[0].drawing().cloned().unwrap();
            assert_eq!(drawing.strokes.len(), 3);
            assert_eq!(drawing.strokes[0].tool, DrawingTool::Fill);
            // The eraser takes out the crayon.
            editor.set_drawing_tools(DrawingTools {
                pick: DrawingPick::Eraser,
                ..editor.drawing_tools()
            });
            draw(&editor, &[(310.0, 315.0)]);
            let drawing = drawings(&editor)[0].drawing().cloned().unwrap();
            assert_eq!(drawing.strokes.len(), 2);
            assert!(
                drawing
                    .strokes
                    .iter()
                    .all(|s| s.tool != DrawingTool::Crayon)
            );
            // Done selects the drawing, and Edit Drawing draws on it again.
            editor.finish_drawing();
            assert!(!editor.drawing_on());
            let id = drawings(&editor)[0].id;
            assert_eq!(editor.selected_objects(), [id]);
            assert!(editor.edit_drawing());
            editor.set_drawing_tools(DrawingTools::default());
            draw(&editor, &[(50.0, 50.0), (60.0, 60.0)]);
            assert_eq!(drawings(&editor).len(), 1);
            assert_eq!(drawings(&editor)[0].drawing().unwrap().strokes.len(), 3);
            editor.finish_drawing();
            // The animation is set in the Drawing tab, as one step.
            editor.set_selected_drawing_animation(Some(DrawingAnimation::default()));
            assert!(drawings(&editor)[0].drawing().unwrap().animation.is_some());
        });
    }

    #[test]
    fn w117_the_tools_start_with_a_black_pen() {
        let tools = DrawingTools::default();
        assert_eq!(tools.pick, DrawingPick::Tool(DrawingTool::Pen));
        assert_eq!(tools.color, Rgba::BLACK);
        assert!(DRAWING_WIDTHS.contains(&tools.width));
    }
}
