//! Line breaking algorithm

use crate::document::{BlockKind, BlockMeta, ParagraphId};
use crate::layout::engine::{ClusterInfo, LineLayout, ParagraphLayout, BASELINE, INDENT_WIDTH};
use crate::layout::font::FontMetrics;
use std::hash::{Hash, Hasher};
use unicode_segmentation::UnicodeSegmentation;

/// Line breaker
#[derive(Default)]
pub struct LineBreaker;

impl LineBreaker {
    pub fn new() -> Self {
        Self
    }

    /// Layout a paragraph into lines
    pub fn layout_paragraph(
        &self,
        para_id: ParagraphId,
        text: &str,
        block_meta: &BlockMeta,
        max_width: f32,
        font_library: &crate::layout::font::FontLibrary,
    ) -> ParagraphLayout {
        // Adjust width for list indentation
        let effective_width = match &block_meta.kind {
            BlockKind::ListItem { indent_level, .. } => {
                max_width - (*indent_level as f32 * INDENT_WIDTH)
            }
            _ => max_width,
        };

        // Determine default font (ID 0 usually)
        let default_font_id = crate::layout::font::FontId(0);
        
        let mut lines = Vec::new();

        if text.is_empty() {
             // Empty paragraph height depends on default font?
             let height = font_library.get(default_font_id).map(|m| m.line_height).unwrap_or(16.0);
             
            // Empty paragraph still has one line
            lines.push(LineLayout {
                byte_range: 0..0,
                clusters: Vec::new(),
                height,
                baseline: BASELINE,
                width: 0.0,
            });
        } else {
            let mut line_start: usize = 0;
            let mut x: f32 = 0.0;
            let mut clusters = Vec::new();
            let mut last_break_point: Option<usize> = None;
            let mut last_break_x: f32 = 0.0;
            
            // Track line height (max of current line)
            let mut current_line_height: f32 = 0.0;

            for (byte_idx, grapheme) in text.grapheme_indices(true) {
                // Determine font for this grapheme
                let font_id = block_meta.styles.iter()
                    .find(|s| byte_idx >= s.start && byte_idx < s.end)
                    .map(|s| s.font_id)
                    .unwrap_or(default_font_id);
                    
                let metrics = font_library.get(font_id)
                    .or_else(|| font_library.get(default_font_id))
                    .expect("Default font missing");

                current_line_height = current_line_height.max(metrics.line_height);

                // Check for explicit line break
                if grapheme == "\n" {
                    lines.push(LineLayout {
                        byte_range: line_start..byte_idx,
                        clusters: std::mem::take(&mut clusters),
                        height: if current_line_height == 0.0 { metrics.line_height } else { current_line_height },
                        baseline: BASELINE,
                        width: x,
                    });
                    line_start = byte_idx + grapheme.len();
                    x = 0.0;
                    last_break_point = None;
                    current_line_height = 0.0;
                    continue;
                }

                // Calculate width using provided metrics
                let cluster_width = if grapheme == "\t" {
                    metrics.default_width * 4.0
                } else if grapheme.chars().all(|c| c.is_control()) {
                    0.0
                } else if grapheme.len() == 1 {
                     metrics.width(grapheme.chars().next().unwrap())
                } else {
                     grapheme.chars().map(|c| metrics.width(c)).sum()
                };

                // Track potential break points (after whitespace)
                if grapheme.chars().all(|c| c.is_whitespace()) {
                    last_break_point = Some(byte_idx + grapheme.len());
                    last_break_x = x + cluster_width;
                }

                // Check for soft wrap
                if x + cluster_width > effective_width && !clusters.is_empty() {
                    // Break at last break point if available
                    let (break_offset, break_x) = if let Some(bp) = last_break_point {
                        (bp, last_break_x)
                    } else {
                        // Emergency break at current position
                        (byte_idx, x)
                    };

                    // Split clusters at break point
                    let break_idx = clusters
                        .iter()
                        .position(|c: &ClusterInfo| c.byte_offset >= break_offset)
                        .unwrap_or(clusters.len());

                    let line_clusters: Vec<_> = clusters.drain(..break_idx).collect();
                    let line_width = line_clusters.last()
                        .map(|c| c.x + c.width)
                        .unwrap_or(0.0);
                    
                    // Note: height should be calculated from the clusters in the line properly if we wrapped.
                    // But we used accumulating max height. 
                    // Simplifying assumption: line height is determined by max height of content *seen so far* on this line.
                    // If we wrap, the next line starts fresh.

                    lines.push(LineLayout {
                        byte_range: line_start..break_offset,
                        clusters: line_clusters,
                        height: current_line_height,
                        baseline: BASELINE,
                        width: line_width,
                    });

                    // Adjust remaining clusters
                    for cluster in &mut clusters {
                        cluster.x -= break_x;
                    }

                    line_start = break_offset;
                    x -= break_x;
                    last_break_point = None;
                    current_line_height = metrics.line_height; // Start next line with current char's height
                }

                clusters.push(ClusterInfo {
                    byte_offset: byte_idx,
                    x,
                    width: cluster_width,
                });
                x += cluster_width;
            }

            // Final line
            if line_start <= text.len() {
                // If last line is empty (e.g. ended with newline), height might be 0
                let final_height = if current_line_height == 0.0 { 
                     font_library.get(default_font_id).map(|m| m.line_height).unwrap_or(16.0)
                } else {
                    current_line_height
                };

                lines.push(LineLayout {
                    byte_range: line_start..text.len(),
                    clusters,
                    height: final_height,
                    baseline: BASELINE,
                    width: x,
                });
            }
        }

        let total_height = lines.iter().map(|l| l.height).sum::<f32>()
            + (block_meta.kind.spacing_after() * 16.0); // Spacing after uses default/fixed unit? 
            // Or should correspond to last line height?
            
        ParagraphLayout {
            para_id,
            lines,
            total_height,
            content_hash: hash_text(text),
        }
    }
}

/// Hash text content for change detection
fn hash_text(text: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::BlockMeta;

    fn test_breaker() -> LineBreaker {
        LineBreaker::new()
    }

    fn para_meta() -> BlockMeta {
        BlockMeta {
            kind: BlockKind::Paragraph,
            start_offset: 0,
            byte_len: 0,
            styles: Vec::new(),
        }
    }

    #[test]
    fn test_empty_paragraph() {
        let breaker = test_breaker();
        let lib = crate::layout::font::FontLibrary::default();
        let layout = breaker.layout_paragraph(
            ParagraphId(0),
            "",
            &para_meta(),
            100.0,
            &lib,
        );

        assert_eq!(layout.lines.len(), 1);
        assert_eq!(layout.lines[0].byte_range, 0..0);
    }

    #[test]
    fn test_single_line() {
        let breaker = test_breaker();
        let lib = crate::layout::font::FontLibrary::default();
        let layout = breaker.layout_paragraph(
            ParagraphId(0),
            "Hello",
            &para_meta(),
            100.0,
            &lib,
        );

        assert_eq!(layout.lines.len(), 1);
        assert_eq!(layout.lines[0].byte_range, 0..5);
        assert_eq!(layout.lines[0].clusters.len(), 5);
    }

    #[test]
    fn test_line_wrap() {
        let breaker = test_breaker();
        let mut lib = crate::layout::font::FontLibrary::new();
        lib.set(crate::layout::font::FontId(0), crate::layout::font::FontMetrics { 
             line_height: 10.0, 
             char_widths: vec![8.0; 256], 
             default_width: 8.0 
        });

        // With 8px per char, 40px width = 5 chars per line
        let layout = breaker.layout_paragraph(
            ParagraphId(0),
            "Hello World",
            &para_meta(),
            40.0,
            &lib,
        );

        assert_eq!(layout.lines.len(), 2);
    }

    #[test]
    fn test_explicit_newline() {
        let breaker = test_breaker();
        let lib = crate::layout::font::FontLibrary::default();
        let layout = breaker.layout_paragraph(
            ParagraphId(0),
            "Hello\nWorld",
            &para_meta(),
            1000.0,
            &lib,
        );

        assert_eq!(layout.lines.len(), 2);
        assert_eq!(layout.lines[0].byte_range, 0..5);
        assert_eq!(layout.lines[1].byte_range, 6..11);
    }
}
