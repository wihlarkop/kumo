/// Model ID constants for all supported LLM providers.
///
/// These are plain `&str` values — you can also pass any model string directly
/// to `.model("custom-model-id")` without needing a constant here.
// ── Anthropic Claude ──────────────────────────────────────────────────────────
pub mod anthropic {
    pub const CLAUDE_OPUS_4_7: &str = "claude-opus-4-7";
    pub const CLAUDE_SONNET_4_6: &str = "claude-sonnet-4-6";
    pub const CLAUDE_HAIKU_4_5: &str = "claude-haiku-4-5";
}

// ── OpenAI ────────────────────────────────────────────────────────────────────
pub mod openai {
    // GPT-5.4 family (latest)
    pub const GPT_5_4_PRO: &str = "gpt-5.4-pro-2026-03-05";
    pub const GPT_5_4: &str = "gpt-5.4";
    pub const GPT_5_4_MINI: &str = "gpt-5.4-mini";
    pub const GPT_5_4_NANO: &str = "gpt-5.4-nano";

    // GPT-5 family
    pub const GPT_5: &str = "gpt-5-2025-08-07";
    pub const GPT_5_MINI: &str = "gpt-5-mini-2025-08-07";
    pub const GPT_5_NANO: &str = "gpt-5-nano-2025-08-07";

    // GPT-4 family
    pub const GPT_4_1: &str = "gpt-4.1-2025-04-14";
}

// ── Google Gemini ─────────────────────────────────────────────────────────────
pub mod gemini {
    // Gemini 3.x (preview)
    pub const GEMINI_3_1_PRO: &str = "gemini-3.1-pro-preview";
    pub const GEMINI_3_1_FLASH_LITE: &str = "gemini-3.1-flash-lite-preview";
    pub const GEMINI_3_FLASH: &str = "gemini-3-flash-preview";

    // Gemini 2.5 (stable)
    pub const GEMINI_2_5_PRO: &str = "gemini-2.5-pro";
    pub const GEMINI_2_5_FLASH: &str = "gemini-2.5-flash";
    pub const GEMINI_2_5_FLASH_LITE: &str = "gemini-2.5-flash-lite";
}

// ── Ollama (local) ────────────────────────────────────────────────────────────
/// Common model names for Ollama. The actual model must be pulled locally first
/// with `ollama pull <model>`.
pub mod ollama {
    // Llama family (Meta)
    pub const LLAMA_4: &str = "llama4";
    pub const LLAMA_3_3: &str = "llama3.3";
    pub const LLAMA_3_2: &str = "llama3.2";
    pub const LLAMA_3_1: &str = "llama3.1";
    pub const LLAMA_3: &str = "llama3";

    // Gemma family (Google)
    pub const GEMMA_4: &str = "gemma4";
    pub const GEMMA_3: &str = "gemma3";
    pub const GEMMA_3N: &str = "gemma3n";
    pub const GEMMA_2: &str = "gemma2";

    // Qwen family (Alibaba)
    pub const QWEN_3_6: &str = "qwen3.6";
    pub const QWEN_3_5: &str = "qwen3.5";

    // GLM family (Zhipu AI)
    pub const GLM_5_1: &str = "glm-5.1";
    pub const GLM_5: &str = "glm-5";
    pub const GLM_4_7_FLASH: &str = "glm-4.7-flash";
    pub const GLM_4_7: &str = "glm-4.7";
    pub const GLM_4_6: &str = "glm-4.6";

    // Kimi family (Moonshot AI)
    pub const KIMI_K2_5: &str = "kimi-k2.5";
    pub const KIMI_K2: &str = "kimi-k2";
    pub const KIMI_K2_THINKING: &str = "kimi-k2-thinking";

    // MiniMax family
    pub const MINIMAX_M2_7: &str = "minimax-m2.7";
    pub const MINIMAX_M2_5: &str = "minimax-m2.5";
    pub const MINIMAX_M2_1: &str = "minimax-m2.1";
    pub const MINIMAX_M2: &str = "minimax-m2";

    // Mistral family
    pub const MISTRAL: &str = "mistral";
    pub const MISTRAL_NEMO: &str = "mistral-nemo";

    // Gemini Family
    pub const GEMINI_3_FLASH_PREVIEW: &str = "gemini-3-flash-preview";

    // OpenAI Family
    pub const GPT_OSS: &str = "gpt-oss";

    // Deepseek Family
    pub const DEEPSEEK_V3_2: &str = "deepseek-v3.2";
    pub const DEEPSEEK_V3_1: &str = "deepseek-v3.1";
    pub const DEEPSEEK_V3: &str = "deepseek-v3";
    pub const DEEPSEEK_R1: &str = "deepseek-r1";
}
