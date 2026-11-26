// The UI files are embedded into the executable (src/assets.rs). They are produced by
// `npm run build`; fail early with a clear message instead of a cryptic include_bytes! error,
// and refuse a web/dist that is partial or differs from what it is copied from (a stale or hand-edited copy).
use std::path::Path;

/// (source, file in web/dist): the same list as scripts/copy-assets.mjs.
const FILES: &[(&str, &str)] = &[
    ("node_modules/@xterm/xterm/lib/xterm.js", "xterm.js"),
    ("node_modules/@xterm/xterm/css/xterm.css", "xterm.css"),
    ("node_modules/@xterm/addon-fit/lib/addon-fit.js", "addon-fit.js"),
    ("node_modules/@xterm/addon-serialize/lib/addon-serialize.js", "addon-serialize.js"),
    ("node_modules/@xterm/addon-unicode11/lib/addon-unicode11.js", "addon-unicode11.js"),
    ("node_modules/@xterm/addon-unicode-graphemes/lib/addon-unicode-graphemes.js", "addon-unicode-graphemes.js"),
    ("web/index.html", "index.html"),
    ("web/app.js", "app.js"),
];

fn main() {
    println!("cargo:rerun-if-changed=web/dist");
    println!("cargo:rerun-if-changed=package-lock.json");
    let dist = Path::new("web").join("dist");
    for (src, name) in FILES {
        println!("cargo:rerun-if-changed={src}");
        let Ok(copy) = std::fs::read(dist.join(name)) else {
            panic!("web/dist/{name} is missing: run `npm ci` and `npm run build` before `cargo build`");
        };
        // a source that is not there (no node_modules on this machine) cannot be compared; one that is must match
        if let Ok(orig) = std::fs::read(src)
            && orig != copy
        {
            panic!("web/dist/{name} differs from {src}: run `npm run build` (after `npm ci` if the lock changed) before `cargo build`");
        }
    }
}
