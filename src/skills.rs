use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use serde::Deserialize;

/// Parsed SKILL.md YAML frontmatter.
#[derive(Debug, Clone, Deserialize)]
pub struct SkillManifest {
    pub name: String,
    pub description: String,
    pub version: Option<String>,
    pub domain: Option<String>,
    pub triggers: Option<SkillTriggers>,
    /// Full SKILL.md body (everything after the closing `---` of frontmatter).
    #[serde(skip)]
    pub content: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SkillTriggers {
    pub keywords: Option<Vec<String>>,
    pub semantic: Option<Vec<String>>,
}

/// Parse SKILL.md: extract YAML frontmatter between `---` delimiters and the body.
/// Returns None if the file doesn't start with `---` or frontmatter is malformed.
fn parse_skill_file(raw: &str) -> Option<SkillManifest> {
    let rest = raw.strip_prefix("---")?;
    let end = rest.find("\n---")?;
    let yaml = &rest[..end];
    let body = rest[end..].strip_prefix("\n---").unwrap_or("").trim_start_matches('\n').to_string();

    let mut manifest: SkillManifest = serde_yml::from_str(yaml).ok()?;
    manifest.content = format!("# {}\n\n{}\n\n{}", manifest.name, manifest.description, body);
    Some(manifest)
}

/// Walk `dir` recursively and collect all SKILL.md files as `SkillManifest`s.
/// Files with missing `name` or `description` are skipped with a warning.
fn load_skills_from_dir(dir: &Path) -> Vec<SkillManifest> {
    let mut skills = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        tracing::warn!(dir = %dir.display(), "PROXY_SKILLS_DIRS: cannot read directory");
        return skills;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            skills.extend(load_skills_from_dir(&path));
        } else if path.file_name().and_then(|n| n.to_str()) == Some("SKILL.md") {
            match std::fs::read_to_string(&path) {
                Ok(raw) => match parse_skill_file(&raw) {
                    Some(m) => {
                        tracing::debug!(name = %m.name, path = %path.display(), "loaded skill");
                        skills.push(m);
                    }
                    None => {
                        tracing::warn!(path = %path.display(), "SKILL.md missing name/description or invalid frontmatter — skipped");
                    }
                },
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "cannot read SKILL.md — skipped");
                }
            }
        }
    }
    skills
}

/// Load skills from colon-separated directory paths.
pub fn load_skills(dirs: &[PathBuf]) -> Vec<SkillManifest> {
    dirs.iter().flat_map(|d| load_skills_from_dir(d)).collect()
}

/// Select up to `max` skills relevant to `message` using keyword scoring.
/// Skills with zero keyword overlap are included as fallback (up to `max`) if
/// no skill scored > 0. Deduplicated by name.
pub fn select_skills<'a>(
    message: &str,
    skills: &'a [SkillManifest],
    max: usize,
    backend_profile_name: &str,
) -> Vec<&'a SkillManifest> {
    if skills.is_empty() || max == 0 {
        return Vec::new();
    }

    let message_tokens: HashSet<String> = message
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(String::from)
        .collect();

    let is_codex_profile = backend_profile_name == "ChatGptCodex";

    let mut scored: Vec<(f32, &SkillManifest)> = skills
        .iter()
        .map(|skill| {
            let keywords = skill
                .triggers
                .as_ref()
                .and_then(|t| t.keywords.as_ref())
                .map(|kws| kws.as_slice())
                .unwrap_or(&[]);

            let score = if keywords.is_empty() {
                0.0_f32
            } else {
                let matches = keywords
                    .iter()
                    .filter(|kw| message_tokens.contains(&kw.to_lowercase()))
                    .count();
                matches as f32 / keywords.len() as f32
            };

            // Domain boost: coding/rust domains get a bump on all profiles; extra on codex.
            let domain_boost = if let Some(domain) = &skill.domain {
                let d = domain.to_lowercase();
                if d == "coding" || d == "rust" || d == "programming" {
                    if is_codex_profile { 0.3 } else { 0.1 }
                } else {
                    0.0
                }
            } else {
                0.0
            };

            (score + domain_boost, skill)
        })
        .collect();

    // Sort by score descending, stable order for ties.
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    // Deduplicate by name.
    let mut seen = HashSet::new();
    let unique: Vec<&SkillManifest> = scored
        .iter()
        .filter_map(|(_, skill)| {
            if seen.insert(skill.name.clone()) {
                Some(*skill)
            } else {
                None
            }
        })
        .collect();

    // If all scores are 0 (no keyword match, no domain), use insertion-order fallback.
    let all_zero = scored.iter().all(|(s, _)| *s == 0.0);
    if all_zero {
        return unique.into_iter().take(max).collect();
    }

    // Return top-k with score > 0 (or include low-score ones up to max if needed).
    unique.into_iter().take(max).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn make_skill_file(dir: &Path, name: &str, description: &str, keywords: &[&str], domain: Option<&str>) {
        let skill_dir = dir.join(name);
        std::fs::create_dir_all(&skill_dir).unwrap();
        let kws = keywords.iter().map(|k| format!("    - {k}")).collect::<Vec<_>>().join("\n");
        let domain_line = domain.map(|d| format!("domain: {d}\n")).unwrap_or_default();
        let content = format!(
            "---\nname: {name}\ndescription: {description}\n{domain_line}triggers:\n  keywords:\n{kws}\n---\n\nSkill body here.\n"
        );
        let mut f = std::fs::File::create(skill_dir.join("SKILL.md")).unwrap();
        f.write_all(content.as_bytes()).unwrap();
    }

    #[test]
    fn parses_valid_skill_md() {
        let raw = "---\nname: foo\ndescription: Foo skill\ntriggers:\n  keywords:\n    - rust\n---\n\nBody text.\n";
        let m = parse_skill_file(raw).expect("should parse");
        assert_eq!(m.name, "foo");
        assert_eq!(m.description, "Foo skill");
        let kws = m.triggers.as_ref().unwrap().keywords.as_ref().unwrap();
        assert_eq!(kws, &["rust"]);
        assert!(m.content.contains("Body text."));
    }

    #[test]
    fn rejects_missing_name() {
        let raw = "---\ndescription: No name\n---\n\nBody.\n";
        assert!(parse_skill_file(raw).is_none());
    }

    #[test]
    fn loads_skills_from_directory() {
        let tmp = TempDir::new().unwrap();
        make_skill_file(tmp.path(), "rust-helper", "Helps with Rust", &["rust", "cargo"], Some("rust"));
        make_skill_file(tmp.path(), "python-helper", "Helps with Python", &["python", "pip"], None);
        let skills = load_skills(&[tmp.path().to_path_buf()]);
        assert_eq!(skills.len(), 2);
    }

    #[test]
    fn select_skills_keyword_match() {
        let tmp = TempDir::new().unwrap();
        make_skill_file(tmp.path(), "rust-helper", "Helps with Rust", &["rust", "cargo"], None);
        make_skill_file(tmp.path(), "python-helper", "Helps with Python", &["python", "pip"], None);
        let skills = load_skills(&[tmp.path().to_path_buf()]);

        let selected = select_skills("I need help with rust cargo build", &skills, 3, "OpenAiResponses");
        assert_eq!(selected.len(), 2); // both returned since max=3
        assert_eq!(selected[0].name, "rust-helper"); // rust-helper scores higher
    }

    #[test]
    fn select_skills_cap_respected() {
        let tmp = TempDir::new().unwrap();
        for i in 0..5 {
            make_skill_file(tmp.path(), &format!("skill-{i}"), &format!("Skill {i}"), &["foo"], None);
        }
        let skills = load_skills(&[tmp.path().to_path_buf()]);
        let selected = select_skills("foo bar baz", &skills, 2, "OpenAiResponses");
        assert_eq!(selected.len(), 2);
    }

    #[test]
    fn select_skills_fallback_when_no_match() {
        let tmp = TempDir::new().unwrap();
        make_skill_file(tmp.path(), "skill-a", "Skill A", &["alpha"], None);
        make_skill_file(tmp.path(), "skill-b", "Skill B", &["beta"], None);
        let skills = load_skills(&[tmp.path().to_path_buf()]);
        // Message has no keyword overlap — fallback to insertion order.
        let selected = select_skills("completely unrelated message", &skills, 2, "OpenAiResponses");
        assert_eq!(selected.len(), 2);
    }

    #[test]
    fn select_skills_deduplicates() {
        let raw = "---\nname: dup\ndescription: Duplicate skill\ntriggers:\n  keywords:\n    - foo\n---\n\nBody.\n";
        let m1 = parse_skill_file(raw).unwrap();
        let m2 = m1.clone();
        let skills = vec![m1, m2];
        let selected = select_skills("foo", &skills, 5, "OpenAiResponses");
        assert_eq!(selected.len(), 1);
    }
}
