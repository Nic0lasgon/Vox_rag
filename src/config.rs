use anyhow::Result;

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub port: u16,
    pub log_level: String,
    pub embedding_provider: String,
    pub embedding_model: String,
    pub embedding_dimension: usize,
    pub embedding_batch_size: usize,
    pub embedding_max_retries: u32,
    pub embedding_timeout_ms: u64,
    pub octen_api_key: String,
    pub octen_api_base_url: String,
    pub similarity_near_duplicate_threshold: f64,
    pub similarity_same_story_threshold: f64,
    pub similarity_context_threshold: f64,
    pub similarity_weak_threshold: f64,
    pub topic_strong_match_threshold: f64,
    pub topic_ambiguous_match_threshold: f64,
    pub topic_weak_match_threshold: f64,
    pub topic_markdown_max_words: usize,
    pub topic_active_days: i64,
    pub topic_archive_days: i64,
    pub use_llm_for_topics: bool,
    pub llm_api_key: String,
    pub llm_model: String,
    pub llm_base_url: String,
    pub llm_timeout_ms: u64,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            database_url: env_required("DATABASE_URL")?,
            port: env_or("PORT", "3000")?.parse()?,
            log_level: env_or("LOG_LEVEL", "info")?,
            embedding_provider: env_or("EMBEDDING_PROVIDER", "octen")?,
            embedding_model: env_or("EMBEDDING_MODEL", "octen-embedding-8b")?,
            embedding_dimension: env_or("EMBEDDING_DIMENSION", "4096")?.parse()?,
            embedding_batch_size: env_or("EMBEDDING_BATCH_SIZE", "32")?.parse()?,
            embedding_max_retries: env_or("EMBEDDING_MAX_RETRIES", "3")?.parse()?,
            embedding_timeout_ms: env_or("EMBEDDING_TIMEOUT_MS", "60000")?.parse()?,
            octen_api_key: env_required("OCTEN_API_KEY")?,
            octen_api_base_url: env_or("OCTEN_API_BASE_URL", "https://api.octen.ai")?,
            similarity_near_duplicate_threshold: env_or(
                "SIMILARITY_NEAR_DUPLICATE_THRESHOLD",
                "0.92",
            )?
            .parse()?,
            similarity_same_story_threshold: env_or("SIMILARITY_SAME_STORY_THRESHOLD", "0.86")?
                .parse()?,
            similarity_context_threshold: env_or("SIMILARITY_CONTEXT_THRESHOLD", "0.78")?
                .parse()?,
            similarity_weak_threshold: env_or("SIMILARITY_WEAK_THRESHOLD", "0.70")?.parse()?,
            topic_strong_match_threshold: env_or("TOPIC_STRONG_MATCH_THRESHOLD", "0.84")?
                .parse()?,
            topic_ambiguous_match_threshold: env_or("TOPIC_AMBIGUOUS_MATCH_THRESHOLD", "0.76")?
                .parse()?,
            topic_weak_match_threshold: env_or("TOPIC_WEAK_MATCH_THRESHOLD", "0.70")?.parse()?,
            topic_markdown_max_words: env_or("TOPIC_MARKDOWN_MAX_WORDS", "1500")?.parse()?,
            topic_active_days: env_or("TOPIC_ACTIVE_DAYS", "3")?.parse()?,
            topic_archive_days: env_or("TOPIC_ARCHIVE_DAYS", "14")?.parse()?,
            use_llm_for_topics: env_or("USE_LLM_FOR_TOPICS", "false")?.parse()?,
            llm_api_key: env_or("LLM_API_KEY", "")?,
            llm_model: env_or("LLM_MODEL", "deepseek-v4-flash")?,
            llm_base_url: env_or("LLM_BASE_URL", "https://api.deepseek.com")?,
            llm_timeout_ms: env_or("LLM_TIMEOUT_MS", "30000")?.parse()?,
        })
    }
}

fn env_required(key: &str) -> Result<String> {
    std::env::var(key).map_err(|_| anyhow::anyhow!("{} environment variable must be set", key))
}

fn env_or(key: &str, default: &str) -> Result<String> {
    Ok(std::env::var(key).unwrap_or_else(|_| default.to_string()))
}
