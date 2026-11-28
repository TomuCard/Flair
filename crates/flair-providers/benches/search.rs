//! Benchmarks of the hot paths: one keystroke must answer within 16 ms for
//! apps and calculations, 50 ms for files (spec, section 3).
//!
//! `cargo bench -p flair-providers`

// Benchmark setup: panicking on a broken fixture is fine.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use flair_core::CancellationToken;
use flair_core::rank::{Prepared, Ranker};
use flair_providers::calc::{Calculator, Settings};
use flair_providers::files::index::{Builder, NO_PARENT, Needle, search, to_bytes, view};

fn calculator(c: &mut Criterion) {
    let calc = Calculator::new(&Settings::default(), &[]);
    let cancel = CancellationToken::new();
    let mut group = c.benchmark_group("calculator");
    for input in [
        "15% de 240",
        "5 km en miles",
        "1 Gio en Mo",
        "5h36 + 65min",
        "26/11/2026 + 45 jours",
        "heure à Tokyo en Paris",
        "sin(pi/6) + sqrt(2)",
        "firefox",
    ] {
        group.bench_function(input, |b| {
            b.iter(|| calc.calculate(black_box(input), &cancel))
        });
    }
    group.finish();
}

/// One million entries, half of the names accented, in 100 000 folders.
fn million() -> rkyv::util::AlignedVec {
    let mut b = Builder::default();
    let root = b.push(NO_PARENT, "/home/me", true).unwrap();
    let mut dirs = vec![root];
    for i in 0..1_000_000u32 {
        let parent = dirs[(i as usize * 7919) % dirs.len()];
        let dir = i % 10 == 0;
        let name = match (dir, i % 2) {
            (true, _) => format!("Dossier {i}"),
            (false, 0) => format!("document_{i}_rapport été.pdf"),
            (false, _) => format!("document_{i}_rapport.pdf"),
        };
        let id = b.push(parent, &name, dir).unwrap();
        if dir {
            dirs.push(id);
        }
    }
    to_bytes(&b.finish(String::new())).unwrap()
}

fn files(c: &mut Criterion) {
    let bytes = million();
    let index = view(&bytes).unwrap();
    let mut group = c.benchmark_group("files_1m");
    group.sample_size(20);
    for query in ["rapport", "document_99999", "ete", "zzz"] {
        let needle = Needle::new(query);
        group.bench_function(query, |b| {
            b.iter(|| search(index, black_box(&needle), 64, &|| false))
        });
    }
    group.finish();
}

fn apps(c: &mut Criterion) {
    let names: Vec<String> = (0..500)
        .map(|i| format!("Application {i} Studio Code Manager"))
        .collect();
    c.bench_function("apps_500_rank", |b| {
        b.iter(|| {
            let prepared = Prepared::new(black_box("vsc"));
            let mut ranker = Ranker::new();
            names
                .iter()
                .filter_map(|n| ranker.match_text(&prepared, n))
                .count()
        });
    });
}

criterion_group!(benches, calculator, files, apps);
criterion_main!(benches);
