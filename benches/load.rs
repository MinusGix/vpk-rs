#![feature(test)]

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use vpk::vpk::ProbableKind;

fn bench_vpk_read(c: &mut Criterion) {
    let file_path = std::env::var("VPK_FILE")
        .expect("Please set VPK_FILE env var to the VPK file to benchmark");
    let file_path = std::path::Path::new(&file_path);

    let kind = std::env::var("VPK_KIND").unwrap_or_else(|_| "Tf2Textures".to_string());
    let kind = match kind.as_str() {
        "None" => ProbableKind::None,
        "Tf2Textures" => ProbableKind::Tf2Textures,
        "Tf2Misc" => ProbableKind::Tf2Misc,
        _ => panic!("Unknown kind"),
    };

    c.bench_function("basic-vpk", |b| {
        b.iter(|| {
            let res = vpk::VPK::read(file_path, kind).unwrap();

            let _res = black_box(res);
        });
    });
}

criterion_group!(benches, bench_vpk_read);
criterion_main!(benches);
