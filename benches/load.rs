#![feature(test)]

extern crate test;

use test::Bencher;

#[bench]
fn bench_vpk_read(b: &mut Bencher) {
    let file_path = std::env::var("VPK_FILE")
        .expect("Please set VPK_FILE env var to the VPK file to benchmark");
    let file_path = std::path::Path::new(&file_path);

    b.iter(|| {
        let _res = vpk::VPK::read(file_path).unwrap();
    });
}
