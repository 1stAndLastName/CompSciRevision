//! `cargo run --bin export -- --base /CompSciRevision --out dist`
//!
//! Renders every page of the site into plain files for GitHub Pages.
//! `--base` is the folder the site is served from ("" for the root of a
//! domain); `--out` is where the files go (default: dist).

use revision_site::{export, load_site};
use std::path::PathBuf;

fn main() {
    let mut base = String::new();
    let mut out = PathBuf::from("dist");
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match (arg.as_str(), args.next()) {
            ("--base", Some(value)) => base = value,
            ("--out", Some(value)) => out = PathBuf::from(value),
            _ => {
                eprintln!("usage: cargo run --bin export -- [--base /Folder] [--out dist]");
                std::process::exit(2);
            }
        }
    }
    // "/CompSciRevision/" and "CompSciRevision" both become "/CompSciRevision".
    let base = base.trim_matches('/');
    let base = if base.is_empty() {
        String::new()
    } else {
        format!("/{base}")
    };

    let library = match load_site() {
        Ok(library) => library,
        Err(error) => {
            eprintln!("Could not load content.\n  {error}");
            std::process::exit(1);
        }
    };
    match export::export_site(&library, &base, &out) {
        Ok(pages) => println!(
            "Exported {pages} pages to {} (links start with \"{base}/\").",
            out.display()
        ),
        Err(error) => {
            eprintln!("Export failed: {error}");
            std::process::exit(1);
        }
    }
}
