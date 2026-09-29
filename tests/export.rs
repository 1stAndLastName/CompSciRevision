//! The static export used for GitHub Pages writes every page, with links that
//! start with the base path.

use revision_site::{content, export};
use std::fs;
use std::path::{Path, PathBuf};

fn export_to(name: &str, base: &str) -> PathBuf {
    let out = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let library = content::load(Path::new("content")).unwrap();
    export::export_site(&library, base, &out).expect("export should work");
    out
}

#[test]
fn every_page_is_written() {
    let out = export_to("export-pages", "/CompSciRevision");
    let library = content::load(Path::new("content")).unwrap();
    assert!(out.join("index.html").exists());
    assert!(out.join("404.html").exists());
    assert!(out.join(".nojekyll").exists());
    assert!(out.join("static/css/site.css").exists());
    assert!(out.join("static/js/quiz.js").exists());
    for topic in &library.topics {
        for page in ["index.html", "flashcards/index.html", "quiz/index.html"] {
            let path = out.join("topics").join(&topic.slug).join(page);
            assert!(path.exists(), "{} should exist", path.display());
        }
    }
}

#[test]
fn links_start_with_the_base_path() {
    let out = export_to("export-links", "/CompSciRevision");
    let home = fs::read_to_string(out.join("index.html")).unwrap();
    assert!(home.contains("href=\"/CompSciRevision/static/css/site.css\""));
    assert!(home.contains("href=\"/CompSciRevision/topics/"));
    // No link may skip the base path (the skip link "#main" is fine).
    assert!(!home.contains("href=\"/static"));
    assert!(!home.contains("href=\"/topics"));
}

#[test]
fn a_folder_not_made_by_the_export_is_left_alone() {
    let out = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("export-refuse");
    let _ = fs::remove_dir_all(&out);
    fs::create_dir_all(&out).unwrap();
    fs::write(out.join("keep-me.txt"), "important").unwrap();
    let library = content::load(Path::new("content")).unwrap();
    assert!(export::export_site(&library, "", &out).is_err());
    assert!(
        out.join("keep-me.txt").exists(),
        "the file must not be deleted"
    );
}
