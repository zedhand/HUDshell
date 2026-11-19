// The UI files are embedded into the executable (src/assets.rs). They are produced by
// `npm run build`; fail early with a clear message instead of a cryptic include_bytes! error.
fn main() {
    println!("cargo:rerun-if-changed=web/dist");
    let index = std::path::Path::new("web").join("dist").join("index.html");
    if !index.exists() {
        panic!("web/dist/index.html is missing: run `npm install` and `npm run build` before `cargo build`");
    }
}
