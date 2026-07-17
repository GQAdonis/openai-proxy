use crate::codex::BackendProfile;

/// Single source of truth for the gpt-5.x model catalogue. `resolve_model()`,
/// `models::list_models()`, and `cli::setup` all read from this table instead
/// of maintaining independent copies.
#[derive(Debug, Clone, Copy)]
pub struct ModelCatalogEntry {
    pub model_id: &'static str,
    pub context_length: u32,
    pub max_output_tokens: u32,
    pub supports_codex_backend: bool,
    pub supports_responses_api: bool,
    pub supports_chat_completions: bool,
    /// Some models report a smaller context window through the ChatGPT-subscription
    /// Codex backend than through the API-key backends. `None` when the context
    /// window is the same across all supported backends.
    pub codex_backend_context_override: Option<u32>,
}

impl ModelCatalogEntry {
    /// Context window for this model as served by `profile`. Applies
    /// `codex_backend_context_override` when `profile` is `ChatGptCodex`.
    pub fn context_length_for(&self, profile: BackendProfile) -> u32 {
        if profile == BackendProfile::ChatGptCodex {
            self.codex_backend_context_override
                .unwrap_or(self.context_length)
        } else {
            self.context_length
        }
    }

    /// Whether this model is available through `profile`.
    pub fn supports(&self, profile: BackendProfile) -> bool {
        match profile {
            BackendProfile::ChatGptCodex => self.supports_codex_backend,
            BackendProfile::OpenAiResponses => self.supports_responses_api,
            BackendProfile::OpenAiChatCompletions => self.supports_chat_completions,
        }
    }
}

pub const CATALOG: &[ModelCatalogEntry] = &[
    ModelCatalogEntry {
        model_id: "gpt-5.5",
        context_length: 1_000_000,
        max_output_tokens: 32_768,
        supports_codex_backend: true,
        supports_responses_api: true,
        supports_chat_completions: true,
        codex_backend_context_override: Some(400_000),
    },
    ModelCatalogEntry {
        model_id: "gpt-5.5-pro",
        context_length: 1_000_000,
        max_output_tokens: 32_768,
        supports_codex_backend: false,
        supports_responses_api: true,
        supports_chat_completions: true,
        codex_backend_context_override: None,
    },
    ModelCatalogEntry {
        model_id: "gpt-5.4",
        context_length: 400_000,
        max_output_tokens: 32_768,
        supports_codex_backend: true,
        supports_responses_api: true,
        supports_chat_completions: true,
        codex_backend_context_override: None,
    },
    ModelCatalogEntry {
        model_id: "gpt-5.4-mini",
        context_length: 200_000,
        max_output_tokens: 16_384,
        supports_codex_backend: true,
        supports_responses_api: true,
        supports_chat_completions: true,
        codex_backend_context_override: None,
    },
    ModelCatalogEntry {
        model_id: "gpt-5.4-nano",
        context_length: 128_000,
        max_output_tokens: 8_192,
        supports_codex_backend: true,
        supports_responses_api: true,
        supports_chat_completions: true,
        codex_backend_context_override: None,
    },
    ModelCatalogEntry {
        model_id: "gpt-5.3-codex",
        context_length: 400_000,
        max_output_tokens: 32_768,
        supports_codex_backend: true,
        supports_responses_api: true,
        supports_chat_completions: true,
        codex_backend_context_override: None,
    },
    ModelCatalogEntry {
        model_id: "gpt-5.3-chat",
        context_length: 128_000,
        max_output_tokens: 16_384,
        supports_codex_backend: true,
        supports_responses_api: true,
        supports_chat_completions: true,
        codex_backend_context_override: None,
    },
    ModelCatalogEntry {
        model_id: "gpt-5.2-chat",
        context_length: 128_000,
        max_output_tokens: 16_384,
        supports_codex_backend: true,
        supports_responses_api: true,
        supports_chat_completions: true,
        codex_backend_context_override: None,
    },
    // gpt-5.6 (released 2026-07-09) ships as three named tiers instead of the
    // older per-generation suffix scheme. All three confirmed available on the
    // ChatGPT-subscription Codex backend via a live request (2026-07-16) —
    // the bare "gpt-5.6" string is rejected upstream, only the canonical
    // suffixed ids are accepted, hence the alias in `ALIASES` below rather
    // than a fourth catalogue entry.
    ModelCatalogEntry {
        model_id: "gpt-5.6-sol",
        context_length: 1_050_000,
        max_output_tokens: 128_000,
        supports_codex_backend: true,
        supports_responses_api: true,
        supports_chat_completions: true,
        codex_backend_context_override: None,
    },
    ModelCatalogEntry {
        model_id: "gpt-5.6-terra",
        context_length: 1_050_000,
        max_output_tokens: 128_000,
        supports_codex_backend: true,
        supports_responses_api: true,
        supports_chat_completions: true,
        codex_backend_context_override: None,
    },
    ModelCatalogEntry {
        model_id: "gpt-5.6-luna",
        context_length: 1_050_000,
        max_output_tokens: 128_000,
        supports_codex_backend: true,
        supports_responses_api: true,
        supports_chat_completions: true,
        codex_backend_context_override: None,
    },
];

/// Legacy aliases that resolve to a canonical `CATALOG` entry by `model_id`.
pub const ALIASES: &[(&str, &str)] = &[
    ("gpt-5.6", "gpt-5.6-sol"),
    ("codex-mini", "gpt-5.4-mini"),
    ("gpt-4o-mini", "gpt-5.4-mini"),
    ("gpt-4o", "gpt-5.3-codex"),
    ("gpt-4o-2024-11-20", "gpt-5.3-codex"),
    ("gpt-4", "gpt-5.3-codex"),
    ("gpt-4-turbo", "gpt-5.3-codex"),
    ("gpt-4-turbo-preview", "gpt-5.3-codex"),
    ("gpt-3.5-turbo", "gpt-5.3-codex"),
    ("gpt-3.5-turbo-0125", "gpt-5.3-codex"),
];

/// Resolve a requested model id to its canonical catalogue entry, following
/// `ALIASES` first. Returns `None` for anything not in the catalogue —
/// callers handle the `gpt-5.*` passthrough and default fallback themselves.
pub fn lookup(model_id: &str) -> Option<&'static ModelCatalogEntry> {
    let canonical = ALIASES
        .iter()
        .find(|(alias, _)| *alias == model_id)
        .map(|(_, canon)| *canon)
        .unwrap_or(model_id);
    CATALOG.iter().find(|entry| entry.model_id == canonical)
}
