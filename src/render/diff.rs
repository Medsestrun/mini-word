//! Render diff protocol for incremental updates

use crate::document::ParagraphId;
use crate::render::{DisplayItem, DisplayItemId, DisplayPage};
use crate::{Point, Rect};
use rustc_hash::FxHashSet;

/// Intermediate diff computed during layout
#[derive(Debug, Default)]
pub struct LayoutDiff {
    /// Paragraphs that changed
    pub changed_paragraphs: FxHashSet<ParagraphId>,
    /// Whether pagination needs to be recalculated
    pub pagination_dirty: bool,
}

impl LayoutDiff {
    pub fn new() -> Self {
        Self::default()
    }
}

/// A single patch operation for the renderer
#[derive(Debug, Clone, PartialEq)]
pub enum RenderPatch {
    /// Insert new display items
    Insert {
        page_index: usize,
        items: Vec<DisplayItem>,
    },
    /// Update an existing display item
    Update {
        page_index: usize,
        item_id: DisplayItemId,
        new_item: DisplayItem,
    },
    /// Remove display items
    Remove {
        page_index: usize,
        item_ids: Vec<DisplayItemId>,
    },
    /// Translate items vertically (scroll optimization)
    TranslateY {
        page_index: usize,
        item_ids: Vec<DisplayItemId>,
        delta_y: f32,
    },
    /// Insert a new page
    InsertPage {
        page: DisplayPage,
    },
    /// Remove a page
    RemovePage {
        page_index: usize,
    },
    /// Move the cursor caret
    MoveCaret {
        old_position: Option<Point>,
        new_position: Point,
    },
    /// Update selection rectangles
    UpdateSelection {
        remove_rects: Vec<Rect>,
        add_rects: Vec<Rect>,
    },
}

/// Complete render diff to send to renderer
#[derive(Debug, Clone, Default)]
pub struct RenderDiff {
    pub version: u64,
    pub patches: Vec<RenderPatch>,
}

impl RenderDiff {
    /// Create empty diff
    pub fn new(version: u64) -> Self {
        Self {
            version,
            patches: Vec::new(),
        }
    }

    /// Create diff from layout diff
    pub fn from_layout_diff(layout_diff: LayoutDiff, version: u64) -> Self {
        // In a full implementation, this would compute actual render patches
        // by comparing old and new display lists
        Self {
            version,
            patches: if layout_diff.changed_paragraphs.is_empty() {
                Vec::new()
            } else {
                // For now, just indicate that changes occurred
                // A real implementation would compute minimal patches
                Vec::new()
            },
        }
    }

    /// Add a patch
    pub fn add_patch(&mut self, patch: RenderPatch) {
        self.patches.push(patch);
    }

    /// Check if there are any patches
    pub fn has_patches(&self) -> bool {
        !self.patches.is_empty()
    }

    /// Get patch count
    pub fn patch_count(&self) -> usize {
        self.patches.len()
    }
}

/// Diff engine for computing render diffs
pub struct DiffEngine {
    /// Previous display list for comparison
    previous_version: u64,
    /// Previous item IDs per page
    previous_items: Vec<FxHashSet<DisplayItemId>>,
}

impl Default for DiffEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl DiffEngine {
    pub fn new() -> Self {
        Self {
            previous_version: 0,
            previous_items: Vec::new(),
        }
    }

    /// Compute diff between previous and current display lists
    pub fn compute_diff(
        &mut self,
        previous: &crate::render::DisplayList,
        current: &crate::render::DisplayList,
        changed_paragraphs: &FxHashSet<ParagraphId>,
    ) -> RenderDiff {
        let mut diff = RenderDiff::new(current.version);

        // Handle page changes
        let prev_page_count = previous.pages.len();
        let curr_page_count = current.pages.len();

        // Remove pages that no longer exist
        for idx in curr_page_count..prev_page_count {
            diff.add_patch(RenderPatch::RemovePage { page_index: idx });
        }

        // Process each current page
        for (page_idx, curr_page) in current.pages.iter().enumerate() {
            if page_idx >= prev_page_count {
                // New page
                diff.add_patch(RenderPatch::InsertPage {
                    page: curr_page.clone(),
                });
            } else {
                // Existing page - compute item diff
                let prev_page = &previous.pages[page_idx];
                self.diff_page_items(page_idx, prev_page, curr_page, changed_paragraphs, &mut diff);
            }
        }

        // Update tracking
        self.previous_version = current.version;
        self.previous_items = current.pages
            .iter()
            .map(|p| {
                p.items
                    .iter()
                    .filter_map(|item| item.id())
                    .collect()
            })
            .collect();

        diff
    }

    /// Compute diff for a single page's items
    fn diff_page_items(
        &self,
        page_index: usize,
        prev_page: &DisplayPage,
        curr_page: &DisplayPage,
        changed_paragraphs: &FxHashSet<ParagraphId>,
        diff: &mut RenderDiff,
    ) {
        use std::collections::HashMap;

        // Build index of previous items
        let prev_items: HashMap<DisplayItemId, &DisplayItem> = prev_page
            .items
            .iter()
            .filter_map(|item| item.id().map(|id| (id, item)))
            .collect();

        let curr_items: HashMap<DisplayItemId, &DisplayItem> = curr_page
            .items
            .iter()
            .filter_map(|item| item.id().map(|id| (id, item)))
            .collect();

        // Find removed items
        let removed: Vec<_> = prev_items
            .keys()
            .filter(|id| !curr_items.contains_key(id))
            .copied()
            .collect();

        if !removed.is_empty() {
            diff.add_patch(RenderPatch::Remove {
                page_index,
                item_ids: removed,
            });
        }

        // Find added items
        let added: Vec<_> = curr_page
            .items
            .iter()
            .filter(|item| {
                item.id()
                    .map(|id| !prev_items.contains_key(&id))
                    .unwrap_or(true)
            })
            .cloned()
            .collect();

        if !added.is_empty() {
            diff.add_patch(RenderPatch::Insert {
                page_index,
                items: added,
            });
        }

        // Find updated items (only for changed paragraphs)
        for (id, curr_item) in &curr_items {
            if !changed_paragraphs.contains(&id.para_id) {
                continue;
            }

            if let Some(prev_item) = prev_items.get(id) {
                if *prev_item != *curr_item {
                    diff.add_patch(RenderPatch::Update {
                        page_index,
                        item_id: *id,
                        new_item: (*curr_item).clone(),
                    });
                }
            }
        }

        // Handle cursor and selection separately
        let prev_caret = prev_page.items.iter().find_map(|item| {
            if let DisplayItem::Caret { position, .. } = item {
                Some(*position)
            } else {
                None
            }
        });

        let curr_caret = curr_page.items.iter().find_map(|item| {
            if let DisplayItem::Caret { position, .. } = item {
                Some(*position)
            } else {
                None
            }
        });

        if prev_caret != curr_caret {
            if let Some(new_pos) = curr_caret {
                diff.add_patch(RenderPatch::MoveCaret {
                    old_position: prev_caret,
                    new_position: new_pos,
                });
            }
        }
    }
}

/// WASM-friendly serialization for render patches
#[cfg(target_arch = "wasm32")]
pub mod wasm {
    use super::*;

    /// Serialized patch header for WASM transfer
    #[repr(C)]
    #[derive(Debug, Clone, Copy)]
    pub struct WasmPatchHeader {
        pub kind: u8,
        pub page_index: u32,
        pub data_offset: u32,
        pub data_len: u32,
    }

    /// Buffer for WASM transfer
    pub struct WasmBuffer {
        data: Vec<u8>,
        headers: Vec<WasmPatchHeader>,
    }

    impl WasmBuffer {
        pub fn new() -> Self {
            Self {
                data: Vec::new(),
                headers: Vec::new(),
            }
        }

        pub fn clear(&mut self) {
            self.data.clear();
            self.headers.clear();
        }

        pub fn data_ptr(&self) -> *const u8 {
            self.data.as_ptr()
        }

        pub fn data_len(&self) -> usize {
            self.data.len()
        }

        pub fn header_ptr(&self) -> *const WasmPatchHeader {
            self.headers.as_ptr()
        }

        pub fn header_count(&self) -> usize {
            self.headers.len()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_diff() {
        let mut diff = RenderDiff::new(1);
        assert!(!diff.has_patches());
        
        diff.add_patch(RenderPatch::RemovePage { page_index: 0 });
        assert!(diff.has_patches());
        assert_eq!(diff.patch_count(), 1);
    }

    #[test]
    fn test_layout_diff() {
        let mut layout_diff = LayoutDiff::new();
        layout_diff.changed_paragraphs.insert(ParagraphId(0));
        layout_diff.pagination_dirty = true;

        let render_diff = RenderDiff::from_layout_diff(layout_diff, 1);
        assert_eq!(render_diff.version, 1);
    }
}
