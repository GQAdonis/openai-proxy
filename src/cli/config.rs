use crate::config::{ProxyConfig, config_dir};

pub fn config_show(cfg: &ProxyConfig) {
    println!("# Resolved configuration");
    println!("[server]");
    println!("host = {:?}", cfg.server.host);
    println!("port = {}", cfg.server.port);
    println!();
    println!("[backend]");
    println!("wire_api = {:?}", cfg.backend.wire_api);
    println!();
    println!("[skills]");
    println!("dirs = {:?}", cfg.skills.dirs);
    println!("max_injected = {}", cfg.skills.max_injected);
    println!();
    println!("[mcp]");
    if let Some(p) = &cfg.mcp.config_path {
        println!("config_path = {:?}", p);
    } else {
        println!("# config_path not set");
    }
    println!();
    println!("[hooks]");
    if let Some(p) = &cfg.hooks.config_path {
        println!("config_path = {:?}", p);
    } else {
        println!("# config_path not set");
    }
    println!();
    println!("[memory]");
    println!("enabled = {}", cfg.memory.enabled);
    if !cfg.memory.db_path.is_empty() {
        println!("db_path = {:?}", cfg.memory.db_path);
    } else {
        println!("# db_path defaults to $XDG_DATA_HOME/oproxy/memory.db");
    }
    println!("embedding_model = {:?}", cfg.memory.embedding_model);
    println!();
    println!("[modes]");
    println!("a2a = {}", cfg.modes.a2a);
}

pub fn config_path() {
    let dir = config_dir();
    let path = dir.join("config.toml");
    let exists = if path.exists() { "exists" } else { "does not exist" };
    println!("{} ({})", path.display(), exists);
}
