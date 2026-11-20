// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Selected cells dragged, as in Pages 15 (TAB-26, PXV-17): a drag from
//! inside the selected cells moves them to where it ends in the same
//! table, or with Alt copies them; ending on another table puts them
//! there, and ending on the page makes them a new table of their own,
//! leaving the source as it was. A press that does not move selects the
//! cell under it, as a click does.

use std::sync::Arc;

use desk_model::table::Area;
use desk_model::{NodeId, ObjectKind};
use gtk::gdk;
use gtk::prelude::*;

use super::TextEditor;
use super::objects::{REACH, Selectable, find, hit};
use super::tables::CellSelection;

/// How far, in pixels, the pointer goes before a press becomes a drag.
const SLOP: f64 = 4.0;

/// Selected cells being dragged.
#[derive(Clone, Copy, Debug)]
pub(super) struct CellDrag {
    /// The table.
    id: NodeId,
    /// The cells dragged.
    area: Area,
    /// The cell the press landed on.
    grabbed: (u32, u32),
    /// Whether the pointer went far enough to be a drag.
    moved: bool,
}

impl TextEditor {
    /// Starts dragging the selected cells of a table from the cell
    /// `grabbed`.
    pub(super) fn start_cell_drag(&self, id: NodeId, area: Area, grabbed: (u32, u32)) {
        self.state.borrow_mut().cell_drag = Some(CellDrag {
            id,
            area,
            grabbed,
            moved: false,
        });
    }

    /// Follows the pointer while cells are dragged, `dx` and `dy` pixels
    /// from the press. Whether cells are being dragged.
    pub(super) fn drag_cells(&self, dx: f64, dy: f64, copying: bool) -> bool {
        let mut state = self.state.borrow_mut();
        let Some(drag) = state.cell_drag.as_mut() else {
            return false;
        };
        if !drag.moved && dx.hypot(dy) >= SLOP {
            drag.moved = true;
        }
        let moved = drag.moved;
        drop(state);
        if moved {
            let cursor = if copying { "copy" } else { "grabbing" };
            self.view.area().set_cursor_from_name(Some(cursor));
        }
        true
    }

    /// Drops the dragged cells where the pointer is, at `x` and `y` of the
    /// view: see the module. Whether cells were being dragged.
    pub(super) fn end_cell_drag(&self, x: f64, y: f64, copying: bool) -> bool {
        let Some(drag) = self.state.borrow_mut().cell_drag.take() else {
            return false;
        };
        self.view.area().set_cursor_from_name(None);
        if !drag.moved {
            self.select_cell(drag.id, drag.grabbed);
            return true;
        }
        let Some((page, px, py)) = self.view.page_at(x, y) else {
            return true;
        };
        let (fx, fy) = (px as f32, py as f32);
        let reach = (REACH / self.view.scale()) as f32;
        let target = {
            let state = self.state.borrow();
            let any = Selectable {
                background: true,
                masters: false,
            };
            hit(&state.layout, page, fx, fy, reach, any).and_then(|id| {
                let (p, placed) = find(&state.layout, id)?;
                let table = placed.table.as_ref()?;
                (p == page)
                    .then(|| (id, table.cell_at(fx - placed.x, fy - placed.y)))
                    .filter(|_| matches!(placed.object.kind, ObjectKind::Table(_)))
                    .map(|(id, cell)| (id, cell, placed.object.locked))
            })
        };
        let Some(source) = self.table_of(drag.id) else {
            return true;
        };
        // The first cell goes where the grabbed cell would land.
        let first = |(r, c): (u32, u32)| {
            (
                r.saturating_sub(drag.grabbed.0 - drag.area.row),
                c.saturating_sub(drag.grabbed.1 - drag.area.col),
            )
        };
        match target {
            Some((_, _, true)) => {}
            Some((id, Some(cell), false)) if id == drag.id => {
                let to = first(cell);
                if to != (drag.area.row, drag.area.col) || copying {
                    let name = if copying { "Copy Cells" } else { "Move Cells" };
                    self.change_table(name, |table, cells| {
                        let landed = table.move_cells(drag.area, to, copying);
                        *cells = select(id, landed);
                    });
                } else {
                    self.select_cell(drag.id, drag.grabbed);
                }
            }
            Some((id, Some(cell), false)) => {
                let Some(mut table) = self.table_of(id).map(|t| (*t).clone()) else {
                    return true;
                };
                let landed = table.put_cells(first(cell), &source, drag.area);
                let (w, h) = self.table_size(&table);
                let mut left = (*source).clone();
                if !copying {
                    left.clear_cells(drag.area);
                }
                let name = if copying { "Copy Cells" } else { "Move Cells" };
                let from = drag.id;
                self.arrange_objects(name, move |edit, _| {
                    let objects = edit.document().objects();
                    let (Some(mut into), Some(mut out)) =
                        (objects.get(id).cloned(), objects.get(from).cloned())
                    else {
                        return Ok(None);
                    };
                    into.kind = ObjectKind::Table(Arc::new(table));
                    into.frame.width = desk_model::Length::from_pt(w);
                    into.frame.height = desk_model::Length::from_pt(h);
                    edit.replace_object(into)?;
                    if !copying {
                        out.kind = ObjectKind::Table(Arc::new(left));
                        edit.replace_object(out)?;
                    }
                    Ok(Some(vec![id]))
                });
                self.state.borrow_mut().cells = Some(select(id, landed));
                self.show_objects();
                self.show_cells();
                self.selection_changed();
            }
            // Between the cells of a table, nothing changes.
            Some((_, None, false)) => self.select_cell(drag.id, drag.grabbed),
            None => {
                let mut table = source.sub_table(drag.area);
                table.options.show_name = true;
                table.options.name_centered = true;
                self.add_table(table, Some((page, px, py)), (0, 0));
            }
        }
        true
    }

    /// The table of an object.
    fn table_of(&self, id: NodeId) -> Option<Arc<desk_model::Table>> {
        let state = self.state.borrow();
        match &state.editor.document().objects().get(id)?.kind {
            ObjectKind::Table(table) => Some(table.clone()),
            _ => None,
        }
    }

    /// Selects one cell of a table, as a click on it does.
    fn select_cell(&self, id: NodeId, cell: (u32, u32)) {
        self.state.borrow_mut().cells = Some(CellSelection {
            id,
            anchor: cell,
            focus: cell,
        });
        self.show_cells();
        self.selection_changed();
    }
}

/// The selection of the cells of an area.
fn select(id: NodeId, area: Area) -> CellSelection {
    CellSelection {
        id,
        anchor: (area.row, area.col),
        focus: (area.row_end() - 1, area.col_end() - 1),
    }
}

/// Whether Alt is held, which makes a drag copy.
pub(super) fn copying(state: gdk::ModifierType) -> bool {
    state.contains(gdk::ModifierType::ALT_MASK)
}
