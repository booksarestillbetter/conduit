use std::fs;
use std::path::Path;

fn main() {
    // Ensure web/dist exists so #[derive(RustEmbed)] succeeds even in clean checkouts without npm build
    let dist_dir = Path::new("web/dist");
    if !dist_dir.exists() {
        let _ = fs::create_dir_all(dist_dir);
        let index_file = dist_dir.join("index.html");
        if !index_file.exists() {
            let _ = fs::write(index_file, "<!DOCTYPE html><html><body>Conduit</body></html>");
        }
    }
}
