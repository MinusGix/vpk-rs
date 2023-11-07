extern crate vpk;

use std::env;

use vpk::vpk::ProbableKind;

fn main() {
    let args: Vec<_> = env::args().collect();

    if args.len() == 1 {
        panic!("Input file is not specified");
    }

    let vpk_file = match vpk::from_path(&args[1], ProbableKind::None) {
        Err(e) => panic!("Error while open file {}, err {}", &args[1], e),
        Ok(vpk_file) => vpk_file,
    };

    // TODO: fix this
    // for (file, _) in &vpk_file.tree {
    //     println!("{}", file);
    // }
}
