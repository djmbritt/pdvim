//! Times the work pdvim does on every frame or key press: drawing a page,
//! recolouring it, reading and searching its text, jumping with flash,
//! finding sentences and paragraphs, and finding the pages in view of a
//! long document. Run with `cargo bench`; criterion compares each run with
//! the last and keeps reports in `target/criterion/`.

#![expect(
    clippy::significant_drop_tightening,
    reason = "a group of benchmarks has to live until it is finished, as each is"
)]
#![expect(clippy::print_stderr, reason = "what is timed, said beside criterion's report")]

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use pdvim::bench::{self, Page};

/// The pages of prose behind the page timed. One is enough for what is
/// done a page at a time; the rest make the document a realistic size.
const PAGES: u32 = 12;

fn page(criterion: &mut Criterion) {
    let pdf = bench::prose_pdf(PAGES);
    let Some(page) = Page::open(&pdf) else {
        eprintln!("MuPDF could not open the PDF written to time; nothing timed");
        return;
    };
    eprintln!("timing a page of {} lines", page.lines());

    let mut group = criterion.benchmark_group("page");
    group.bench_function("record", |bencher| bencher.iter(|| page.record()));
    // Scales as at 100% zoom, and on a large high-DPI screen.
    for scale in [1.0_f32, 2.5] {
        for (name, recolour) in [("render", false), ("render recoloured", true)] {
            group.bench_with_input(BenchmarkId::new(name, scale), &scale, |bencher, &scale| {
                bencher.iter(|| page.render(black_box(scale), recolour));
            });
        }
    }
    group.bench_function("read text", |bencher| bencher.iter(|| page.read_text()));
    group.finish();

    let mut group = criterion.benchmark_group("text");
    for needle in ["e", "the", "a sentence ending"] {
        group.bench_with_input(BenchmarkId::new("search", needle), needle, |bencher, needle| {
            bencher.iter(|| page.search(black_box(needle)));
        });
    }
    for typed in ["t", "the", "the cu"] {
        group.bench_with_input(BenchmarkId::new("flash", typed), typed, |bencher, typed| {
            bencher.iter(|| page.flash(black_box(typed)));
        });
    }
    group.bench_function("sentences", |bencher| bencher.iter(|| page.sentences()));
    group.bench_function("paragraphs", |bencher| bencher.iter(|| page.paragraphs()));
    group.finish();
}

fn document(criterion: &mut Criterion) {
    let defaults = include_str!("../defaults.toml");
    criterion.bench_function("config defaults", |bencher| {
        bencher.iter(|| bench::config(black_box(defaults)));
    });

    let mut group = criterion.benchmark_group("layout");
    for pages in [100_u32, 10_000] {
        group.bench_with_input(BenchmarkId::new("lay out", pages), &pages, |bencher, &pages| {
            bencher.iter(|| bench::column(black_box(pages)));
        });
        // A screen's height of scrolling, a frame at a time, through the
        // middle of the document.
        let layout = bench::column(pages);
        let middle = 792.0 * f32::from(u16::try_from(pages / 2).unwrap_or(u16::MAX));
        let views: Vec<f32> = (0_u16..60)
            .map(|frame| 18.0_f32.mul_add(f32::from(frame), middle))
            .collect();
        group.bench_with_input(BenchmarkId::new("pages in view", pages), &views, |bencher, views| {
            bencher.iter(|| bench::pages_in_view(&layout, black_box(views), 1080.0));
        });
    }
    group.finish();
}

criterion_group!(benches, page, document);
criterion_main!(benches);
