extern crate vpk;

use std::env;
use std::fs;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::vec::Vec;

fn main() -> std::io::Result<()> {
    let args: Vec<_> = env::args().collect();

    if args.len() != 3 {
        panic!("Usage: extract <path to vpk_dir.vpk> <path to export dir>");
    }

    // Check destination dir
    let path = Path::new(&args[2]);
    if !path.is_dir() {
        panic!("Given export path is not directory or doesn't exists");
    }

    let vpk_file = match vpk::from_path(&args[1]) {
        Err(e) => panic!("Error while open file {}, err {}", &args[1], e),
        Ok(vpk_file) => vpk_file,
    };

    for (file, vpk_entry) in vpk_file.tree.iter() {
        println!(
            "Extract {}, archive index {}...",
            file, vpk_entry.dir_entry.archive_index
        );
        let file_path = Path::new(file);
        fs::create_dir_all(path.join(&file_path.parent().unwrap()))?;

        let buf = vpk_entry.get(&vpk_file)?;

        let mut out_buf = File::create(&path.join(&file_path))?;
        out_buf.write(&buf)?;
    }

    Ok(())
}
