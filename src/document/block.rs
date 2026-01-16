//! Block-level element metadata

/// Unique identifier for a list
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ListId(pub u64);

/// Type of list marker
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListMarker {
    Bullet,
    Numbered { ordinal: u32 },
}

impl ListMarker {
    /// Get the display string for this marker
    pub fn display(&self) -> String {
        match self {
            ListMarker::Bullet => "•".to_string(),
            ListMarker::Numbered { ordinal } => format!("{}.", ordinal),
        }
    }
}

/// The kind of block element
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockKind {
    /// Regular paragraph
    Paragraph,
    /// Heading with level (1-6)
    Heading { level: u8 },
    /// List item
    ListItem {
        list_id: ListId,
        indent_level: u8,
        marker: ListMarker,
    },
}

impl Default for BlockKind {
    fn default() -> Self {
        BlockKind::Paragraph
    }
}

impl BlockKind {
    /// Get the line height multiplier for this block kind
    pub fn line_height_multiplier(&self) -> f32 {
        match self {
            BlockKind::Paragraph => 1.0,
            BlockKind::Heading { level } => match level {
                1 => 1.5,
                2 => 1.4,
                3 => 1.3,
                _ => 1.2,
            },
            BlockKind::ListItem { .. } => 1.0,
        }
    }

    /// Get the spacing after this block (in line heights)
    pub fn spacing_after(&self) -> f32 {
        match self {
            BlockKind::Paragraph => 1.0,
            BlockKind::Heading { .. } => 0.5,
            BlockKind::ListItem { .. } => 0.25,
        }
    }

    /// Check if this is a heading
    pub fn is_heading(&self) -> bool {
        matches!(self, BlockKind::Heading { .. })
    }

    /// Check if this is a list item
    pub fn is_list_item(&self) -> bool {
        matches!(self, BlockKind::ListItem { .. })
    }
}

/// Metadata for a block-level element
#[derive(Debug, Clone, PartialEq)]
pub struct BlockMeta {
    /// The kind of block
    pub kind: BlockKind,
    /// Byte offset where this block starts in the document
    pub start_offset: usize,
    /// Length of this block in bytes
    pub byte_len: usize,
}

impl BlockMeta {
    /// Create a new paragraph block
    pub fn paragraph(start_offset: usize, byte_len: usize) -> Self {
        Self {
            kind: BlockKind::Paragraph,
            start_offset,
            byte_len,
        }
    }

    /// Create a new heading block
    pub fn heading(level: u8, start_offset: usize, byte_len: usize) -> Self {
        Self {
            kind: BlockKind::Heading { level: level.min(6).max(1) },
            start_offset,
            byte_len,
        }
    }

    /// Create a new list item block
    pub fn list_item(
        list_id: ListId,
        indent_level: u8,
        marker: ListMarker,
        start_offset: usize,
        byte_len: usize,
    ) -> Self {
        Self {
            kind: BlockKind::ListItem {
                list_id,
                indent_level,
                marker,
            },
            start_offset,
            byte_len,
        }
    }

    /// Get the end offset of this block
    pub fn end_offset(&self) -> usize {
        self.start_offset + self.byte_len
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block_kind() {
        let para = BlockKind::Paragraph;
        assert!(!para.is_heading());
        assert!(!para.is_list_item());

        let heading = BlockKind::Heading { level: 1 };
        assert!(heading.is_heading());

        let list = BlockKind::ListItem {
            list_id: ListId(0),
            indent_level: 0,
            marker: ListMarker::Bullet,
        };
        assert!(list.is_list_item());
    }

    #[test]
    fn test_list_marker_display() {
        assert_eq!(ListMarker::Bullet.display(), "•");
        assert_eq!(ListMarker::Numbered { ordinal: 1 }.display(), "1.");
        assert_eq!(ListMarker::Numbered { ordinal: 10 }.display(), "10.");
    }
}
