//! Benchmarks for the editor core

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use mini_word::{Editor, LayoutConstraints, Rect};

fn default_constraints() -> LayoutConstraints {
    LayoutConstraints {
        page_width: 612.0,
        page_height: 792.0,
        margin_top: 72.0,
        margin_bottom: 72.0,
        margin_left: 72.0,
        margin_right: 72.0,
    }
}

fn bench_insert_single_char(c: &mut Criterion) {
    c.bench_function("insert_single_char", |b| {
        let mut editor = Editor::new(default_constraints());
        b.iter(|| {
            editor.insert_text(black_box("x"));
        });
    });
}

fn bench_insert_word(c: &mut Criterion) {
    c.bench_function("insert_word", |b| {
        let mut editor = Editor::new(default_constraints());
        b.iter(|| {
            editor.insert_text(black_box("hello "));
        });
    });
}

fn bench_layout_small(c: &mut Criterion) {
    c.bench_function("layout_small_document", |b| {
        let mut editor = Editor::new(default_constraints());
        editor.insert_text("Hello, World! This is a small document.\n\nIt has a few paragraphs.");
        
        b.iter(|| {
            editor.update_layout();
        });
    });
}

fn bench_layout_medium(c: &mut Criterion) {
    c.bench_function("layout_medium_document", |b| {
        let mut editor = Editor::new(default_constraints());
        
        // Create ~10 pages of content
        for i in 0..50 {
            editor.insert_text(&format!(
                "Paragraph {} contains enough text to span multiple lines and test the line breaking algorithm. ",
                i
            ));
            if i % 3 == 0 {
                editor.insert_text("\n\n");
            }
        }
        
        b.iter(|| {
            editor.update_layout();
        });
    });
}

fn bench_build_display_list(c: &mut Criterion) {
    c.bench_function("build_display_list", |b| {
        let mut editor = Editor::new(default_constraints());
        editor.insert_text("Hello, World! This is a test document.\n\nWith multiple paragraphs.");
        editor.update_layout();
        
        let viewport = Rect::new(0.0, 0.0, 612.0, 792.0);
        
        b.iter(|| {
            black_box(editor.build_display_list(viewport));
        });
    });
}

fn bench_undo_redo(c: &mut Criterion) {
    c.bench_function("undo_redo_cycle", |b| {
        let mut editor = Editor::new(default_constraints());
        
        // Create some history
        for i in 0..10 {
            editor.insert_text(&format!("Text {} ", i));
        }
        
        b.iter(|| {
            if editor.undo() {
                editor.redo();
            }
        });
    });
}

criterion_group!(
    benches,
    bench_insert_single_char,
    bench_insert_word,
    bench_layout_small,
    bench_layout_medium,
    bench_build_display_list,
    bench_undo_redo,
);

criterion_main!(benches);
