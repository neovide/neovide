use indoc::writedoc;
use itertools::Itertools;
use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

fn main() {
    #[cfg(windows)]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("assets/neovide.ico");
        res.compile().expect("Could not attach exe icon");
        println!("cargo::rerun-if-changed=assets/neovide.ico");
    }
    // Build a function that generates a row a column from a Kitty graphics protocol diacritic.
    let base_diacritics_filename = "kitty_rowcolumn_diacritics";
    let input_filename = format!("src/renderer/{base_diacritics_filename}.txt");
    let out_dir = env::var_os("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join(format!("{base_diacritics_filename}.rs"));

    let f =
        File::open(&input_filename).unwrap_or_else(|_| panic!("Could not open {input_filename}"));
    let reader = BufReader::new(f);
    let mut start = 0;
    let mut end = 0;
    let mut index = 0;
    let match_arms = reader
        .lines()
        .flat_map(|line| {
            let line = line.expect("Failed to read line");
            if line.is_empty() || line.starts_with("#") {
                return None;
            }
            let diacritic = line.split_once(';').expect("Failed to split line").0;
            let diacritic = u32::from_str_radix(diacritic, 16).expect("Not a hex value");
            if start == 0 {
                start = diacritic;
                end = diacritic;
                None
            } else if diacritic == end + 1 {
                end = diacritic;
                None
            } else if start == end {
                let ret = format!("        {start} => {index},");
                start = diacritic;
                end = diacritic;
                index += 1;
                Some(ret)
            } else {
                let ret = format!("        {start}..={end} => {index} + diacritic - {start},");
                index += end - start + 1;
                start = diacritic;
                end = diacritic;
                Some(ret)
            }
        })
        .join("\n");
    let mut dest_file = File::create(&dest_path)
        .unwrap_or_else(|_| panic!("Could not open destination file {:?}", dest_path));
    writedoc! {
        &mut dest_file,
        r#"
        fn get_row_or_col(diacritic: char) -> u32 {{
            let diacritic = diacritic as u32;
            match diacritic {{
        {match_arms}
                _ => 0,
            }}
        }}
        "#
    }
    .unwrap();
    println!("{:#?}", dest_path);
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-changed={input_filename}");
}
