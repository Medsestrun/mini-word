//! Editing model: cursor, selection, and edit operations

mod cursor;
mod operation;

pub use cursor::{Affinity, Cursor, DocPosition, Selection};
pub use operation::{AbsoluteOffset, EditOp, EditResult};
