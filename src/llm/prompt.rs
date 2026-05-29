use anyhow::{Context, Result};

use super::provider::LlmDecisionResponse;
use crate::db::schema::{Article, Topic, TopicCandidate};

const SYSTEM_PROMPT: &str = r#"Tu es un assistant qui décide si un nouvel article appartient à un dossier d'actualité existant. Tu dois TOUJOURS répondre en français.

Les types de relation possibles :
1. near_duplicate — même information, reprise quasi identique
2. same_story_update — nouvelle étape dans la même histoire
3. background_context — article utile pour comprendre le contexte
4. reaction — réaction politique, diplomatique, économique
5. analysis — article d'analyse, pas forcément événement nouveau
6. related_but_different — sujet proche mais événement différent
7. new_topic — sujet nouveau, pas lié aux dossiers existants

Règle absolue : un faux rattachement est plus grave qu'un article manqué. En cas de doute, préfère new_topic.

Réponds UNIQUEMENT en JSON avec ce format :
{"decision": "same_story_update", "confidence": 0.85, "should_update_topic_summary": true, "should_create_timeline_event": true, "timeline_event_type": "update", "reason": "L'article décrit une nouvelle étape du même conflit."}"#;

pub fn system_prompt() -> &'static str {
    SYSTEM_PROMPT
}

pub fn build_user_prompt(
    article: &Article,
    candidates: &[TopicCandidate],
    topic_details: Option<&Topic>,
    timeline_summary: &str,
) -> String {
    let published_at = article
        .published_at
        .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| "inconnue".to_string());

    let description = article.description.as_deref().unwrap_or("aucun");

    let mut candidates_text = String::new();
    for c in candidates {
        candidates_text.push_str(&format!(
            "  - Titre : {}\n    Score : {:.2}\n    Statut : {}\n",
            c.title, c.score, c.status
        ));
    }

    let mut topic_block = String::new();
    if let Some(topic) = topic_details {
        let summary = topic.short_summary.as_deref().unwrap_or("aucun");
        topic_block = format!(
            "\nDossier le plus probable :\nTitre : {}\nRésumé : {}\nEntités : {}\n",
            topic.title, summary, topic.main_entities
        );
    }

    let timeline_block = if timeline_summary.is_empty() {
        "aucune timeline".to_string()
    } else {
        timeline_summary.to_string()
    };

    format!(
        "Nouvel article :\nTitre : {}\nDate : {}\nSource : {}\nRésumé : {}\n\nDossier candidat :\n{}{}\nTimeline du dossier :\n{}",
        article.title,
        published_at,
        article.source_name,
        description,
        candidates_text,
        topic_block,
        timeline_block
    )
}

pub fn build_decision_prompt(
    article: &Article,
    candidates: &[TopicCandidate],
    topic_details: Option<&Topic>,
    timeline_summary: &str,
) -> String {
    let system = system_prompt();
    let user = build_user_prompt(article, candidates, topic_details, timeline_summary);
    format!("{}\n\n{}", system, user)
}

pub fn parse_llm_response(raw: &str) -> Result<LlmDecisionResponse> {
    let cleaned = raw
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    serde_json::from_str(cleaned).context("Failed to parse LLM response as JSON")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    fn make_article() -> Article {
        Article {
            id: Uuid::new_v4(),
            source_name: "Le Monde".to_string(),
            source_url: None,
            canonical_url: None,
            title: "Tensions au Proche-Orient".to_string(),
            description: Some("Nouvelles frappes signalées".to_string()),
            author: None,
            language: Some("fr".to_string()),
            country: None,
            published_at: Some(Utc::now()),
            fetched_at: None,
            raw_text: None,
            cleaned_text: None,
            title_hash: None,
            content_hash: None,
            simhash: None,
            status: None,
            created_at: None,
            updated_at: None,
        }
    }

    fn make_candidates() -> Vec<TopicCandidate> {
        vec![TopicCandidate {
            topic_id: Uuid::new_v4(),
            title: "Conflit Israël-Iran".to_string(),
            status: "active".to_string(),
            score: 0.78,
            last_seen_at: Some(Utc::now()),
        }]
    }

    #[test]
    fn prompt_contains_article_info() {
        let article = make_article();
        let candidates = make_candidates();
        let prompt = build_decision_prompt(&article, &candidates, None, "");
        assert!(prompt.contains("Tensions au Proche-Orient"));
        assert!(prompt.contains("Le Monde"));
        assert!(prompt.contains("Nouvelles frappes signalées"));
    }

    #[test]
    fn prompt_contains_candidates() {
        let article = make_article();
        let candidates = make_candidates();
        let prompt = build_decision_prompt(&article, &candidates, None, "");
        assert!(prompt.contains("Conflit Israël-Iran"));
        assert!(prompt.contains("0.78"));
    }

    #[test]
    fn prompt_contains_decision_options() {
        let article = make_article();
        let candidates = make_candidates();
        let prompt = build_decision_prompt(&article, &candidates, None, "");
        assert!(prompt.contains("near_duplicate"));
        assert!(prompt.contains("same_story_update"));
        assert!(prompt.contains("background_context"));
        assert!(prompt.contains("new_topic"));
    }

    #[test]
    fn parse_llm_response_strips_markdown() {
        let raw = r#"```json
{"decision": "new_topic", "confidence": 0.9, "should_update_topic_summary": false, "should_create_timeline_event": false, "timeline_event_type": null, "reason": "test"}
```"#;
        let result = parse_llm_response(raw).unwrap();
        assert_eq!(result.decision, "new_topic");
        assert!((result.confidence - 0.9).abs() < f64::EPSILON);
    }
}
