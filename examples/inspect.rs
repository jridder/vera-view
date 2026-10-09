//! Print page and timing information for a drawing without opening a window.
//! Usage: `cargo run --release --example inspect -- FILE.vsdx`

use std::time::Instant;

use vera_view::model::Document;
use vera_view::scene;

fn main() -> anyhow::Result<()> {
    let path = std::env::args().nth(1).ok_or_else(|| anyhow::anyhow!("usage: inspect FILE.vsdx"))?;
    let start = Instant::now();
    let doc = Document::open(path.as_ref())?;
    println!("parsed in {:.2?}: {} pages, {} masters, {} styles", start.elapsed(), doc.pages.len(), doc.masters.len(), doc.styles.len());
    for i in 0..doc.pages.len() {
        let start = Instant::now();
        let scene = scene::build(&doc, i);
        println!(
            "page {} '{}': {:.2} x {:.2} in, {} shapes, {} items, built in {:.2?}",
            i + 1,
            doc.pages[i].name,
            scene.width,
            scene.height,
            scene.shape_count,
            scene.items.len(),
            start.elapsed()
        );
        for w in &scene.warnings {
            println!("  note: {w}");
        }
    }
    Ok(())
}
