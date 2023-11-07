#![feature(test)]

use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn bench_vpk_read(c: &mut Criterion) {
    let file_path = std::env::var("VPK_FILE")
        .expect("Please set VPK_FILE env var to the VPK file to benchmark");
    let file_path = std::path::Path::new(&file_path);

    c.bench_function("basic-vpk", |b| {
        b.iter(|| {
            let res = vpk::VPK::read(file_path).unwrap();

            let _res = black_box(res);
        });
    });
}

criterion_group!(benches, bench_vpk_read);
criterion_main!(benches);
