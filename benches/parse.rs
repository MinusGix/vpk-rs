use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn bench_skip_cstring(c: &mut Criterion) {
    let cursor = std::io::Cursor::new(b"hello world for the moon is fake and unreal\0" as &[u8]);
    c.bench_function("skip-cstring", |b| {
        b.iter(|| {
            let mut cursor = cursor.clone();
            let data = vpk::vpk::skip_cstring(black_box(&mut cursor));

            let _data = black_box(data).unwrap();
            // assert_eq!(data, 0..43);
        });
    });
}

criterion_group!(benches, bench_skip_cstring);
criterion_main!(benches);
