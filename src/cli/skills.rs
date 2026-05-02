use std::path::PathBuf;

use clap::Args;

use crate::config::expand_tilde;

#[derive(Args, Debug)]
pub struct SkillsListArgs {
    /// Colon-separated skill directories (overrides config)
    #[arg(long)]
    pub dirs: Option<String>,
}

#[derive(Args, Debug)]
pub struct SkillsValidateArgs {
    /// Directory to validate
    pub dir: PathBuf,
}

#[derive(Args, Debug)]
pub struct SkillsTestArgs {
    /// Message to test skill selection against
    pub message: String,

    /// Colon-separated skill directories (overrides config)
    #[arg(long)]
    pub dirs: Option<String>,

    /// Maximum skills to show
    #[arg(long, default_value_t = 3)]
    pub max: usize,

    /// Backend profile to use for domain boosting
    #[arg(long, default_value = "OpenAiResponses")]
    pub profile: String,
}

pub fn skills_list(args: &SkillsListArgs, cfg_dirs: &[String]) {
    let dirs = resolve_dirs(args.dirs.as_deref(), cfg_dirs);
    if dirs.is_empty() {
        println!("No skill directories configured. Set [skills] dirs in config.toml or use --dirs.");
        return;
    }
    let skills = crate::skills::load_skills(&dirs);
    if skills.is_empty() {
        println!("No skills found in: {}", dirs.iter().map(|d| d.display().to_string()).collect::<Vec<_>>().join(":"));
        return;
    }
    println!("{:<30} {:<10} {:<12} {}", "NAME", "VERSION", "DOMAIN", "KEYWORDS");
    println!("{}", "-".repeat(65));
    for s in &skills {
        let version = s.version.as_deref().unwrap_or("-");
        let domain = s.domain.as_deref().unwrap_or("-");
        let kw_count = s.triggers.as_ref()
            .and_then(|t| t.keywords.as_ref())
            .map(|kws| kws.len())
            .unwrap_or(0);
        println!("{:<30} {:<10} {:<12} {}", s.name, version, domain, kw_count);
    }
    println!();
    println!("{} skill(s) found", skills.len());
}

pub fn skills_validate(args: &SkillsValidateArgs) {
    let skill_mds = find_skill_mds(&args.dir);
    let total = skill_mds.len();
    let skills = crate::skills::load_skills(&[args.dir.clone()]);
    let valid = skills.len();
    let invalid = total.saturating_sub(valid);

    println!("{}/{} SKILL.md files valid", valid, total);
    if invalid > 0 {
        println!("{invalid} invalid (see tracing warnings above for details)");
        std::process::exit(1);
    }
}

fn find_skill_mds(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else { return out };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(find_skill_mds(&path));
        } else if path.file_name().and_then(|n| n.to_str()) == Some("SKILL.md") {
            out.push(path);
        }
    }
    out
}

pub fn skills_test(args: &SkillsTestArgs, cfg_dirs: &[String]) {
    let dirs = resolve_dirs(args.dirs.as_deref(), cfg_dirs);
    let skills = crate::skills::load_skills(&dirs);
    if skills.is_empty() {
        println!("No skills found.");
        return;
    }

    println!("Message: {:?}", args.message);
    println!("Profile: {}", args.profile);
    println!();

    let selected = crate::skills::select_skills(&args.message, &skills, args.max, &args.profile);
    if selected.is_empty() {
        println!("No skills matched.");
    } else {
        println!("Selected (top {}):", selected.len());
        for (i, s) in selected.iter().enumerate() {
            let kws: Vec<&str> = s.triggers.as_ref()
                .and_then(|t| t.keywords.as_ref())
                .map(|kws| kws.as_slice())
                .unwrap_or(&[])
                .iter()
                .filter(|kw| args.message.to_lowercase().contains(&kw.to_lowercase()))
                .map(|kw| kw.as_str())
                .collect();
            let kw_str = if kws.is_empty() { String::new() } else { format!("  keywords matched: {}", kws.join(", ")) };
            println!("  {}. {}{}", i + 1, s.name, kw_str);
        }
    }

    let not_selected: Vec<_> = skills.iter().filter(|s| !selected.iter().any(|sel| sel.name == s.name)).collect();
    if !not_selected.is_empty() {
        println!();
        println!("Not selected:");
        for s in &not_selected {
            println!("  - {}", s.name);
        }
    }
}

fn resolve_dirs(flag: Option<&str>, cfg_dirs: &[String]) -> Vec<PathBuf> {
    if let Some(raw) = flag {
        raw.split(':').filter(|s| !s.is_empty()).map(|d| expand_tilde(d)).collect()
    } else {
        cfg_dirs.iter().map(|d| expand_tilde(d)).collect()
    }
}
