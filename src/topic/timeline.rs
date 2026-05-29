use anyhow::{Context, Result};
use chrono::Utc;
use sqlx::PgPool;
use tracing::{info, instrument};
use uuid::Uuid;

use crate::db::schema::{Article, TopicTimelineEvent};
use crate::db::topic_queries;

pub fn infer_event_type(article_title: &str, existing_events_count: usize) -> &'static str {
    if existing_events_count == 0 {
        return "start";
    }

    let lower = article_title.to_lowercase();

    let escalation_keywords = [
        "escalad",
        "frappe",
        "attaque",
        "bombe",
        "invasion",
        "offensive",
        "strike",
        "attack",
    ];
    if escalation_keywords.iter().any(|k| lower.contains(k)) {
        return "escalation";
    }

    let deescalation_keywords = [
        "cession-le-feu",
        "cessez-le-feu",
        "cessez le feu",
        "paix",
        "pacification",
        "désescalade",
        "desescalade",
        "ceasefire",
        "trêve",
        "treve",
        "accord",
    ];
    if deescalation_keywords.iter().any(|k| lower.contains(k)) {
        return "deescalation";
    }

    let correction_keywords = [
        "correction",
        "rectification",
        "démenti",
        "dementi",
        "infirme",
        "dément",
    ];
    if correction_keywords.iter().any(|k| lower.contains(k)) {
        return "correction";
    }

    let announcement_keywords = [
        "annonce",
        "déclaration",
        "decision",
        "décision",
        "nomme",
        "promulgue",
        "signe",
        "annonce",
    ];
    if announcement_keywords.iter().any(|k| lower.contains(k)) {
        return "announcement";
    }

    let reaction_keywords = [
        "réaction",
        "reaction",
        "condamne",
        "approuve",
        "soutien",
        "critique",
    ];
    if reaction_keywords.iter().any(|k| lower.contains(k)) {
        return "reaction";
    }

    "update"
}

pub fn infer_importance(article_title: &str) -> &'static str {
    let lower = article_title.to_lowercase();

    let critical_keywords = [
        "cessez-le-feu",
        "cessez le feu",
        "cession-le-feu",
        "déclaration de guerre",
        "démission",
        "demission",
        "assassinat",
        "attentat",
        "breakthrough",
        "historique",
    ];
    if critical_keywords.iter().any(|k| lower.contains(k)) {
        return "critical";
    }

    let high_keywords = [
        "frappe",
        "attaque",
        "invasion",
        "annonce",
        "élection",
        "election",
        "résultat",
        "resultat",
        "accord",
    ];
    if high_keywords.iter().any(|k| lower.contains(k)) {
        return "high";
    }

    "medium"
}

#[instrument(skip(pool, article))]
pub async fn add_article_to_timeline(
    pool: &PgPool,
    topic_id: Uuid,
    article: &Article,
    relation_type: &str,
    confidence_score: f64,
) -> Result<Option<TopicTimelineEvent>> {
    match relation_type {
        "near_duplicate" | "weak_context" => return Ok(None),
        "same_story_update"
        | "background_context"
        | "reaction"
        | "analysis"
        | "fact_check"
        | "contradiction"
        | "related_but_different" => {}
        _ => return Ok(None),
    }

    let events = topic_queries::get_timeline_events(pool, topic_id).await?;

    let event_type = infer_event_type(&article.title, events.len());
    let importance = infer_importance(&article.title);

    let event_date = article.published_at.unwrap_or_else(Utc::now);

    let summary = article
        .description
        .clone()
        .or_else(|| {
            article
                .cleaned_text
                .as_deref()
                .map(|s| s.chars().take(300).collect::<String>())
        })
        .unwrap_or_else(|| article.title.clone());

    let event = topic_queries::insert_timeline_event(
        pool,
        topic_id,
        event_date,
        &article.title,
        &summary,
        importance,
        Some(event_type),
        &[article.id],
        confidence_score,
    )
    .await
    .context("Failed to insert timeline event")?;

    info!(
        event_id = %event.id,
        topic_id = %topic_id,
        article_id = %article.id,
        event_type = event_type,
        "Timeline event created"
    );

    Ok(Some(event))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infer_event_type_first_event_is_start() {
        assert_eq!(infer_event_type("Anything here", 0), "start");
    }

    #[test]
    fn infer_event_type_reaction() {
        assert_eq!(
            infer_event_type("L'opposition condamne fermement cette mesure", 3),
            "reaction"
        );
    }

    #[test]
    fn infer_event_type_escalation() {
        assert_eq!(
            infer_event_type("Forte attaque signalée dans la région", 3),
            "escalation"
        );
    }

    #[test]
    fn infer_event_type_deescalation() {
        assert_eq!(
            infer_event_type("Cessez-le-feu signé entre les parties", 3),
            "deescalation"
        );
    }

    #[test]
    fn infer_event_type_correction() {
        assert_eq!(
            infer_event_type("Démenti officiel : rectification du communiqué", 3),
            "correction"
        );
    }

    #[test]
    fn infer_event_type_announcement() {
        assert_eq!(
            infer_event_type("Nouvelle déclaration du gouvernement", 3),
            "announcement"
        );
    }

    #[test]
    fn infer_event_type_default_update() {
        assert_eq!(
            infer_event_type("Météo du jour : nuageux avec éclaircies", 3),
            "update"
        );
    }

    #[test]
    fn infer_importance_critical() {
        assert_eq!(
            infer_importance("Assassinat du premier ministre"),
            "critical"
        );
    }

    #[test]
    fn infer_importance_high() {
        assert_eq!(infer_importance("Résultat des élections annoncé"), "high");
    }

    #[test]
    fn infer_importance_medium() {
        assert_eq!(
            infer_importance("Mise à jour météo pour la semaine"),
            "medium"
        );
    }
}
