//! Render output: display list and diff protocol

mod diff;
mod display;

pub use diff::{LayoutDiff, RenderDiff, RenderPatch};
pub use display::{DisplayItem, DisplayItemId, DisplayList, DisplayPage, ListMarkerDisplay};
