use std::{
    env,
    ffi::OsStr,
    fmt::Write,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn collect(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("read Vite output") {
        let path = entry.expect("read Vite directory entry").path();
        if path.is_dir() {
            collect(&path, files);
        } else {
            files.push(path);
        }
    }
}

fn main() {
    for input in [
        "frontend/src",
        "frontend/public",
        "frontend/index.html",
        "frontend/package.json",
        "frontend/package-lock.json",
        "frontend/.node-version",
        "frontend/tsconfig.json",
        "frontend/vite.config.ts",
        "frontend/schema-plugin.mjs",
    ] {
        println!("cargo:rerun-if-changed={input}");
    }
    for key in ["PATH", "NODE_OPTIONS", "SOURCE_DATE_EPOCH"] {
        println!("cargo:rerun-if-env-changed={key}");
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo OUT_DIR"));
    let dir = out.join("console");
    let npm = if cfg!(windows) { "npm.cmd" } else { "npm" };
    let status = Command::new(npm)
        .current_dir("frontend")
        .env("NODE_ENV", "production")
        .args(["run", "build", "--", "--outDir"])
        .arg(&dir)
        .arg("--emptyOutDir")
        .status()
        .expect("building the console requires Node and npm; see frontend/README.md");
    assert!(
        status.success(),
        "console build failed; install the locked frontend dependencies with npm ci --include=dev"
    );
    let index = fs::read_to_string(dir.join("index.html")).expect("Vite did not emit index.html");
    let mut entries = 0;
    for link in index
        .split('"')
        .filter_map(|part| part.strip_prefix("/ui/"))
    {
        assert!(
            dir.join(link).is_file(),
            "HTML references missing asset {link}"
        );
        entries += 1;
    }
    assert!(entries > 0, "console index references no emitted assets");
    let mut files = Vec::new();
    collect(&dir, &mut files);
    files.sort();
    let mut table = String::from("static ASSETS: &[(&str, &str, &[u8])] = &[\n");
    for path in files {
        let key = path
            .strip_prefix(&dir)
            .unwrap()
            .to_str()
            .expect("UTF-8 asset path")
            .replace('\\', "/");
        let mime = match path.extension().and_then(OsStr::to_str) {
            Some("html") => "text/html; charset=utf-8",
            Some("css") => "text/css; charset=utf-8",
            Some("js" | "mjs") => "text/javascript; charset=utf-8",
            Some("json") => "application/json",
            Some("woff2") => "font/woff2",
            Some("woff") => "font/woff",
            Some("svg") => "image/svg+xml",
            Some("png") => "image/png",
            Some("ico") => "image/x-icon",
            _ => "application/octet-stream",
        };
        writeln!(table, "({key:?}, {mime:?}, include_bytes!(concat!(env!(\"OUT_DIR\"), \"/console/\", {key:?}))),").unwrap();
    }
    table.push_str("];\n");
    fs::write(out.join("web_assets.rs"), table).expect("write embedded asset table");
}
