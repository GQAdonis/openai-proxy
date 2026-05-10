use std::{
    collections::{HashMap, HashSet},
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

/// Tokenize a string into lowercase alphanumeric tokens.
fn tokenize(s: &str) -> HashSet<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(String::from)
        .collect()
}

/// Skill index with IDF weights precomputed at build time.
///
/// IDF (Inverse Document Frequency) gives higher weight to keywords that appear
/// in fewer skills — rare, specific keywords are more diagnostic than common ones.
/// This mirrors the TF-IDF hybrid scoring in opencode's `skill/index.ts`.
#[derive(Debug, Clone)]
pub struct SkillIndex {
    pub manifests: Vec<SkillManifest>,
    /// IDF weight per lowercased keyword token: ln(N / df).
    idf: HashMap<String, f32>,
}

impl SkillIndex {
    /// Build the index from loaded manifests, computing IDF weights once.
    pub fn build(manifests: Vec<SkillManifest>) -> Self {
        let n = manifests.len();
        let mut df: HashMap<String, usize> = HashMap::new();

        for manifest in &manifests {
            // Collect unique tokens across name, description, and trigger keywords.
            let mut seen: HashSet<String> = HashSet::new();
            for token in tokenize(&manifest.name) {
                seen.insert(token);
            }
            for token in tokenize(&manifest.description) {
                seen.insert(token);
            }
            if let Some(triggers) = &manifest.triggers {
                if let Some(kws) = &triggers.keywords {
                    for kw in kws {
                        for token in tokenize(kw) {
                            seen.insert(token);
                        }
                    }
                }
            }
            for token in seen {
                *df.entry(token).or_insert(0) += 1;
            }
        }

        // IDF = ln(N / df). When N == 0 or df == 0, weight is 0.
        let idf = if n == 0 {
            HashMap::new()
        } else {
            df.into_iter()
                .map(|(token, count)| {
                    let weight = (n as f32 / count as f32).ln();
                    (token, weight)
                })
                .collect()
        };

        Self { manifests, idf }
    }

    pub fn is_empty(&self) -> bool {
        self.manifests.is_empty()
    }

    pub fn len(&self) -> usize {
        self.manifests.len()
    }

    /// Select up to `max` skills relevant to `message`.
    ///
    /// Scoring: 60% IDF-weighted keyword overlap + 40% raw keyword overlap + domain boost.
    /// This matches opencode's 60/40 TF-IDF / keyword hybrid in `skill/index.ts`.
    ///
    /// Fallback: if all scores are 0 (no keyword overlap at all), return up to `max`
    /// skills in insertion order so there is always some context injected.
    pub fn select<'a>(
        &'a self,
        message: &str,
        max: usize,
        backend_profile_name: &str,
    ) -> Vec<&'a SkillManifest> {
        if self.manifests.is_empty() || max == 0 {
            return Vec::new();
        }

        let message_tokens = tokenize(message);
        let is_codex_profile = backend_profile_name.contains("ChatGPT") || backend_profile_name.contains("ChatGptCodex");

        let mut scored: Vec<(f32, &SkillManifest)> = self.manifests
            .iter()
            .map(|skill| {
                let keywords: Vec<String> = skill
                    .triggers
                    .as_ref()
                    .and_then(|t| t.keywords.as_ref())
                    .map(|kws| kws.iter().flat_map(|kw| tokenize(kw)).collect())
                    .unwrap_or_default();

                let (idf_score, raw_score) = if keywords.is_empty() {
                    (0.0_f32, 0.0_f32)
                } else {
                    // IDF-weighted score: sum of IDF weights for matched keywords /
                    // sum of IDF weights for all skill keywords.
                    let total_idf: f32 = keywords.iter()
                        .map(|kw| self.idf.get(kw).copied().unwrap_or(0.0))
                        .sum();

                    let matched_idf: f32 = keywords.iter()
                        .filter(|kw| message_tokens.contains(*kw))
                        .map(|kw| self.idf.get(kw).copied().unwrap_or(0.0))
                        .sum();

                    let idf = if total_idf > 0.0 { matched_idf / total_idf } else { 0.0 };

                    // Raw keyword overlap: matched count / total keyword count.
                    let matched_count = keywords.iter()
                        .filter(|kw| message_tokens.contains(*kw))
                        .count();
                    let raw = matched_count as f32 / keywords.len() as f32;

                    (idf, raw)
                };

                // Blend: 60% IDF-weighted + 40% raw overlap.
                let keyword_score = 0.6 * idf_score + 0.4 * raw_score;

                // Domain boost: coding/rust domains get a bump; extra on codex backend.
                let domain_boost = match skill.domain.as_deref().map(|d| d.to_lowercase()).as_deref() {
                    Some("coding") | Some("rust") | Some("programming") => {
                        if is_codex_profile { 0.3 } else { 0.1 }
                    }
                    _ => 0.0,
                };

                (keyword_score + domain_boost, skill)
            })
            .collect();

        // Sort by score descending, stable for ties.
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        // Deduplicate by name.
        let mut seen = HashSet::new();
        let unique: Vec<&SkillManifest> = scored
            .iter()
            .filter_map(|(_, skill)| {
                if seen.insert(skill.name.clone()) { Some(*skill) } else { None }
            })
            .collect();

        // If all scores are 0 (no keyword overlap, no domain boost), fall back to
        // insertion order so context is always injected rather than nothing.
        let all_zero = scored.iter().all(|(s, _)| *s == 0.0);
        if all_zero {
            return self.manifests.iter().take(max).collect();
        }

        unique.into_iter().take(max).collect()
    }
}

/// Load skills from a list of directory paths and build a `SkillIndex`.
pub fn load_skills(dirs: &[PathBuf]) -> SkillIndex {
    let manifests = dirs.iter().flat_map(|d| load_skills_from_dir(d)).collect();
    SkillIndex::build(manifests)
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
        let idx = load_skills(&[tmp.path().to_path_buf()]);
        assert_eq!(idx.len(), 2);
    }

    #[test]
    fn select_keyword_match() {
        let tmp = TempDir::new().unwrap();
        make_skill_file(tmp.path(), "rust-helper", "Helps with Rust", &["rust", "cargo"], None);
        make_skill_file(tmp.path(), "python-helper", "Helps with Python", &["python", "pip"], None);
        let idx = load_skills(&[tmp.path().to_path_buf()]);

        let selected = idx.select("I need help with rust cargo build", 3, "OpenAiResponses");
        assert_eq!(selected.len(), 2);
        assert_eq!(selected[0].name, "rust-helper");
    }

    #[test]
    fn select_cap_respected() {
        let tmp = TempDir::new().unwrap();
        for i in 0..5 {
            make_skill_file(tmp.path(), &format!("skill-{i}"), &format!("Skill {i}"), &["foo"], None);
        }
        let idx = load_skills(&[tmp.path().to_path_buf()]);
        let selected = idx.select("foo bar baz", 2, "OpenAiResponses");
        assert_eq!(selected.len(), 2);
    }

    #[test]
    fn select_fallback_when_no_match() {
        let tmp = TempDir::new().unwrap();
        make_skill_file(tmp.path(), "skill-a", "Skill A", &["alpha"], None);
        make_skill_file(tmp.path(), "skill-b", "Skill B", &["beta"], None);
        let idx = load_skills(&[tmp.path().to_path_buf()]);
        let selected = idx.select("completely unrelated message", 2, "OpenAiResponses");
        assert_eq!(selected.len(), 2);
    }

    #[test]
    fn select_deduplicates() {
        let raw = "---\nname: dup\ndescription: Duplicate skill\ntriggers:\n  keywords:\n    - foo\n---\n\nBody.\n";
        let m1 = parse_skill_file(raw).unwrap();
        let m2 = m1.clone();
        let idx = SkillIndex::build(vec![m1, m2]);
        let selected = idx.select("foo", 5, "OpenAiResponses");
        assert_eq!(selected.len(), 1);
    }

    #[test]
    fn idf_weights_rare_keywords_higher() {
        // "actix" appears in only 1 skill (df=1); "rust" appears in both (df=2).
        // A message mentioning "actix" should rank the actix skill above a rust-only skill
        // because actix has higher IDF weight.
        let raw_actix = "---\nname: actix-helper\ndescription: Actix web framework\ntriggers:\n  keywords:\n    - actix\n    - rust\n---\n\nBody.\n";
        let raw_rust = "---\nname: rust-helper\ndescription: General Rust\ntriggers:\n  keywords:\n    - rust\n---\n\nBody.\n";
        let m_actix = parse_skill_file(raw_actix).unwrap();
        let m_rust = parse_skill_file(raw_rust).unwrap();
        let idx = SkillIndex::build(vec![m_actix, m_rust]);

        let selected = idx.select("I need help with actix", 2, "OpenAiResponses");
        assert_eq!(selected[0].name, "actix-helper");
    }

    #[test]
    fn empty_index_returns_empty() {
        let idx = SkillIndex::build(vec![]);
        assert!(idx.select("any message", 3, "OpenAiResponses").is_empty());
    }

    #[test]
    fn single_skill_select_works() {
        let raw = "---\nname: solo\ndescription: Solo skill\ntriggers:\n  keywords:\n    - solo\n---\n\nBody.\n";
        let m = parse_skill_file(raw).unwrap();
        let idx = SkillIndex::build(vec![m]);
        // When N=1, IDF for all tokens = ln(1/1) = 0; falls through to raw overlap only.
        let selected = idx.select("solo task here", 1, "OpenAiResponses");
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].name, "solo");
    }
}
