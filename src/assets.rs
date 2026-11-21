//! The terminal UI (HTML, JS, CSS and the pinned xterm.js files) compiled into the executable,
//! so HUDshell runs from any folder with no loose files. `npm run build` fills `web/dist`;
//! `build.rs` refuses to compile if it is missing. Served from memory by `wv.rs` for
//! `https://hudshell.local/*` (the name never touches the network).

pub struct Asset {
    pub path: &'static str,
    pub mime: &'static str,
    pub data: &'static [u8],
}

macro_rules! asset {
    ($path:literal, $mime:literal, $file:literal) => {
        Asset {
            path: $path,
            mime: $mime,
            data: include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/web/dist/", $file)),
        }
    };
}

pub static ASSETS: &[Asset] = &[
    asset!("/index.html", "text/html; charset=utf-8", "index.html"),
    asset!("/app.js", "text/javascript; charset=utf-8", "app.js"),
    asset!("/xterm.js", "text/javascript; charset=utf-8", "xterm.js"),
    asset!("/xterm.css", "text/css; charset=utf-8", "xterm.css"),
    asset!("/addon-fit.js", "text/javascript; charset=utf-8", "addon-fit.js"),
    asset!("/addon-serialize.js", "text/javascript; charset=utf-8", "addon-serialize.js"),
    asset!("/addon-unicode11.js", "text/javascript; charset=utf-8", "addon-unicode11.js"),
    asset!("/addon-unicode-graphemes.js", "text/javascript; charset=utf-8", "addon-unicode-graphemes.js"),
];

pub fn find(path: &str) -> Option<&'static Asset> {
    ASSETS.iter().find(|a| a.path == path)
}
