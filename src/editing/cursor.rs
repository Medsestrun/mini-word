//! Cursor and selection management

use crate::document::ParagraphId;

/// Position in document as (paragraph_id, offset_within_paragraph)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DocPosition {
    /// The paragraph containing this position
    pub para_id: ParagraphId,
    /// Byte offset within the paragraph
    pub offset: usize,
}

impl DocPosition {
    /// Create a new document position
    pub fn new(para_id: ParagraphId, offset: usize) -> Self {
        Self { para_id, offset }
    }
}

impl PartialOrd for DocPosition {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for DocPosition {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match self.para_id.cmp(&other.para_id) {
            std::cmp::Ordering::Equal => self.offset.cmp(&other.offset),
            other => other,
        }
    }
}

/// Cursor affinity for ambiguous positions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Affinity {
    /// Prefer end of previous line
    Upstream,
    /// Prefer start of next line
    #[default]
    Downstream,
}

/// The text cursor (caret)
#[derive(Debug, Clone, Default)]
pub struct Cursor {
    /// Current position in the document
    pub position: DocPosition,
    /// Affinity for ambiguous positions
    pub affinity: Affinity,
    /// Remembered X coordinate for vertical movement
    pub preferred_x: Option<f32>,
}

impl Cursor {
    /// Create a new cursor at the given position
    pub fn new(position: DocPosition) -> Self {
        Self {
            position,
            affinity: Affinity::Downstream,
            preferred_x: None,
        }
    }

    /// Create cursor at document start
    pub fn at_start() -> Self {
        Self::default()
    }

    /// Move cursor to a new position
    pub fn move_to(&mut self, position: DocPosition) {
        self.position = position;
        self.preferred_x = None;
    }

    /// Move cursor to position, keeping preferred X
    pub fn move_to_vertical(&mut self, position: DocPosition) {
        self.position = position;
    }
}

/// Text selection (anchor + active point)
#[derive(Debug, Clone, Default)]
pub struct Selection {
    /// The anchor point (fixed during extension)
    pub anchor: DocPosition,
    /// The active point (moves during extension)
    pub active: DocPosition,
}

impl Selection {
    /// Create a new selection
    pub fn new(anchor: DocPosition, active: DocPosition) -> Self {
        Self { anchor, active }
    }

    /// Create a collapsed selection (cursor)
    pub fn collapsed(position: DocPosition) -> Self {
        Self {
            anchor: position.clone(),
            active: position,
        }
    }

    /// Check if selection is collapsed (no text selected)
    pub fn is_collapsed(&self) -> bool {
        self.anchor == self.active
    }

    /// Get ordered start and end positions
    pub fn ordered(&self) -> (DocPosition, DocPosition) {
        if self.anchor <= self.active {
            (self.anchor.clone(), self.active.clone())
        } else {
            (self.active.clone(), self.anchor.clone())
        }
    }

    /// Get the start position
    pub fn start(&self) -> &DocPosition {
        if self.anchor <= self.active {
            &self.anchor
        } else {
            &self.active
        }
    }

    /// Get the end position
    pub fn end(&self) -> &DocPosition {
        if self.anchor <= self.active {
            &self.active
        } else {
            &self.anchor
        }
    }

    /// Extend selection to a new active position
    pub fn extend_to(&mut self, position: DocPosition) {
        self.active = position;
    }

    /// Check if a position is within the selection
    pub fn contains(&self, pos: &DocPosition) -> bool {
        let (start, end) = self.ordered();
        *pos >= start && *pos < end
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_doc_position_ordering() {
        let pos1 = DocPosition::new(ParagraphId(0), 5);
        let pos2 = DocPosition::new(ParagraphId(0), 10);
        let pos3 = DocPosition::new(ParagraphId(1), 0);

        assert!(pos1 < pos2);
        assert!(pos2 < pos3);
        assert!(pos1 < pos3);
    }

    #[test]
    fn test_selection_ordered() {
        let anchor = DocPosition::new(ParagraphId(0), 10);
        let active = DocPosition::new(ParagraphId(0), 5);

        let sel = Selection::new(anchor, active);
        let (start, end) = sel.ordered();

        assert_eq!(start.offset, 5);
        assert_eq!(end.offset, 10);
    }

    #[test]
    fn test_selection_collapsed() {
        let pos = DocPosition::new(ParagraphId(0), 5);
        let sel = Selection::collapsed(pos);
        assert!(sel.is_collapsed());
    }
}
