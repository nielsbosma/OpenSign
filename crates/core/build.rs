//! Embeds `docs/*.md` so the docs tools can serve them without files next to the binary. English only: the translations
//! in `docs/<language>/` are for the docs site, and this reads no subfolders.

use std::path::Path;
use std::{env, fs};

fn main() {
    let dir = Path::new(&env::var("CARGO_MANIFEST_DIR").expect("manifest dir")).join("../../docs");
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut pages: Vec<_> = fs::read_dir(&dir)
        .expect("docs directory")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "md"))
        .collect();
    pages.sort();
    let mut out = String::from("pub static PAGES: &[(&str, &str)] = &[\n");
    for page in pages {
        println!("cargo:rerun-if-changed={}", page.display());
        let slug = page.file_stem().and_then(|s| s.to_str()).expect("file name");
        let path = page.canonicalize().expect("docs page");
        out.push_str(&format!("    ({slug:?}, include_str!({path:?})),\n"));
    }
    out.push_str("];\n");
    fs::write(Path::new(&env::var("OUT_DIR").expect("out dir")).join("docs.rs"), out).expect("write docs.rs");
}
