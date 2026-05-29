use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use tracing::{debug, instrument};
use uuid::Uuid;

use crate::db::topic_queries;

#[instrument(skip(pool))]
pub async fn build_topic_markdown(
    pool: &PgPool,
    topic_id: Uuid,
    max_words: usize,
) -> Result<String> {
    let topic = topic_queries::get_topic_by_id(pool, topic_id)
        .await?
        .context("Topic not found")?;

    let articles = topic_queries::get_topic_articles(pool, topic_id).await?;
    let events = topic_queries::get_timeline_events(pool, topic_id).await?;

    let mut sections = String::new();

    sections.push_str(&format!("# {}\n\n", topic.title));

    sections.push_str(&format!("## Statut\n\n{}\n\n", topic.status));

    let summary = topic
        .short_summary
        .as_deref()
        .or(topic.long_summary.as_deref())
        .unwrap_or("Résumé en cours de construction.");
    sections.push_str(&format!("## Résumé\n\n{}\n\n", summary));

    sections.push_str("## Entités principales\n\n");
    if let Some(arr) = topic.main_entities.as_array() {
        for val in arr {
            if let Some(s) = val.as_str() {
                sections.push_str(&format!("- {}\n", s));
            }
        }
    }
    sections.push('\n');

    if !events.is_empty() {
        sections.push_str("## Chronologie\n\n");
        for event in &events {
            let date = event.event_date.format("%Y-%m-%d");
            sections.push_str(&format!(
                "- {} : {} — {}\n",
                date, event.title, event.summary
            ));
        }
        sections.push('\n');
    }

    if !articles.is_empty() {
        sections.push_str("## Articles liés\n\n");
        for ta in &articles {
            let short_id = &ta.article_id.to_string()[..8];
            sections.push_str(&format!("  - {}\n", short_id));
        }
        sections.push('\n');
    }

    let last_update = topic
        .last_seen_at
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "N/A".to_string());
    sections.push_str(&format!("## Dernière mise à jour\n\n{}\n", last_update));

    debug!(topic_id = %topic_id, words = sections.split_whitespace().count(), "Built topic markdown");

    Ok(truncate_to_words(&sections, max_words))
}

pub fn compute_markdown_hash(markdown: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(markdown.as_bytes());
    hex::encode(hasher.finalize())
}

pub fn truncate_to_words(text: &str, max_words: usize) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() <= max_words {
        text.to_string()
    } else {
        words
            .into_iter()
            .take(max_words)
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_markdown_hash_is_deterministic() {
        let h1 = compute_markdown_hash("# Test\n\nContent here");
        let h2 = compute_markdown_hash("# Test\n\nContent here");
        assert_eq!(h1, h2);
    }

    #[test]
    fn compute_markdown_hash_different_inputs() {
        let h1 = compute_markdown_hash("# Topic A");
        let h2 = compute_markdown_hash("# Topic B");
        assert_ne!(h1, h2);
    }

    #[test]
    fn truncate_to_words_short_text() {
        let result = truncate_to_words("hello world", 10);
        assert_eq!(result, "hello world");
    }

    #[test]
    fn truncate_to_words_exact_length() {
        let result = truncate_to_words("one two three", 3);
        assert_eq!(result, "one two three");
    }

    #[test]
    fn truncate_to_words_long_text() {
        let result = truncate_to_words("one two three four five", 3);
        assert_eq!(result, "one two three");
    }

    #[test]
    fn truncate_to_words_empty() {
        let result = truncate_to_words("", 5);
        assert_eq!(result, "");
    }
}
