//! Line breaking algorithm

use crate::document::{BlockKind, BlockMeta, ParagraphId};
use crate::layout::engine::{ClusterInfo, LineLayout, ParagraphLayout, BASELINE, INDENT_WIDTH, LINE_HEIGHT};
use std::hash::{Hash, Hasher};
use unicode_segmentation::UnicodeSegmentation;

/// Character width provider (simplified - assumes monospace)
pub struct CharWidthProvider {
    /// Default character width
    default_width: f32,
}

impl Default for CharWidthProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl CharWidthProvider {
    pub fn new() -> Self {
        Self { default_width: 8.0 }
    }

    /// Get width of a grapheme cluster
    pub fn width(&self, grapheme: &str) -> f32 {
        // Simplified: each grapheme has the same width
        // In a real implementation, this would query font metrics
        if grapheme == "\t" {
            self.default_width * 4.0
        } else if grapheme.chars().all(|c| c.is_control()) {
            0.0
        } else {
            self.default_width * grapheme.chars().count() as f32
        }
    }
}

/// Line breaker
pub struct LineBreaker {
    char_widths: CharWidthProvider,
}

impl Default for LineBreaker {
    fn default() -> Self {
        Self::new()
    }
}

impl LineBreaker {
    pub fn new() -> Self {
        Self {
            char_widths: CharWidthProvider::new(),
        }
    }

    /// Layout a paragraph into lines
    pub fn layout_paragraph(
        &self,
        para_id: ParagraphId,
        text: &str,
        block_meta: &BlockMeta,
        max_width: f32,
    ) -> ParagraphLayout {
        // Adjust width for list indentation
        let effective_width = match &block_meta.kind {
            BlockKind::ListItem { indent_level, .. } => {
                max_width - (*indent_level as f32 * INDENT_WIDTH)
            }
            _ => max_width,
        };

        let line_height = LINE_HEIGHT * block_meta.kind.line_height_multiplier();

        let mut lines = Vec::new();

        if text.is_empty() {
            // Empty paragraph still has one line
            lines.push(LineLayout {
                byte_range: 0..0,
                clusters: Vec::new(),
                height: line_height,
                baseline: BASELINE,
                width: 0.0,
            });
        } else {
            let mut line_start: usize = 0;
            let mut x: f32 = 0.0;
            let mut clusters = Vec::new();
            let mut last_break_point: Option<usize> = None;
            let mut last_break_x: f32 = 0.0;

            for (byte_idx, grapheme) in text.grapheme_indices(true) {
                // Check for explicit line break
                if grapheme == "\n" {
                    lines.push(LineLayout {
                        byte_range: line_start..byte_idx,
                        clusters: std::mem::take(&mut clusters),
                        height: line_height,
                        baseline: BASELINE,
                        width: x,
                    });
                    line_start = byte_idx + grapheme.len();
                    x = 0.0;
                    last_break_point = None;
                    continue;
                }

                let cluster_width = self.char_widths.width(grapheme);

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

                    lines.push(LineLayout {
                        byte_range: line_start..break_offset,
                        clusters: line_clusters,
                        height: line_height,
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
                lines.push(LineLayout {
                    byte_range: line_start..text.len(),
                    clusters,
                    height: line_height,
                    baseline: BASELINE,
                    width: x,
                });
            }
        }

        let total_height = lines.iter().map(|l| l.height).sum::<f32>()
            + self.paragraph_spacing(block_meta);

        ParagraphLayout {
            para_id,
            lines,
            total_height,
            content_hash: hash_text(text),
        }
    }

    /// Get spacing after paragraph
    fn paragraph_spacing(&self, block_meta: &BlockMeta) -> f32 {
        LINE_HEIGHT * block_meta.kind.spacing_after()
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
        }
    }

    #[test]
    fn test_empty_paragraph() {
        let breaker = test_breaker();
        let layout = breaker.layout_paragraph(
            ParagraphId(0),
            "",
            &para_meta(),
            100.0,
        );

        assert_eq!(layout.lines.len(), 1);
        assert_eq!(layout.lines[0].byte_range, 0..0);
    }

    #[test]
    fn test_single_line() {
        let breaker = test_breaker();
        let layout = breaker.layout_paragraph(
            ParagraphId(0),
            "Hello",
            &para_meta(),
            100.0,
        );

        assert_eq!(layout.lines.len(), 1);
        assert_eq!(layout.lines[0].byte_range, 0..5);
        assert_eq!(layout.lines[0].clusters.len(), 5);
    }

    #[test]
    fn test_line_wrap() {
        let breaker = test_breaker();
        // With 8px per char, 40px width = 5 chars per line
        let layout = breaker.layout_paragraph(
            ParagraphId(0),
            "Hello World",
            &para_meta(),
            40.0,
        );

        assert_eq!(layout.lines.len(), 2);
    }

    #[test]
    fn test_explicit_newline() {
        let breaker = test_breaker();
        let layout = breaker.layout_paragraph(
            ParagraphId(0),
            "Hello\nWorld",
            &para_meta(),
            1000.0,
        );

        assert_eq!(layout.lines.len(), 2);
        assert_eq!(layout.lines[0].byte_range, 0..5);
        assert_eq!(layout.lines[1].byte_range, 6..11);
    }
}
