use serde_json::Value;
use uuid::Uuid;

use crate::db::schema::TopicCandidate;
use crate::llm::provider::LlmDecisionResponse;

#[derive(Debug, Clone)]
pub struct TopicThresholds {
    pub near_duplicate: f64,
    pub same_story: f64,
    pub topic_strong_match: f64,
    pub topic_ambiguous_match: f64,
    pub topic_weak_match: f64,
    pub context: f64,
}

impl Default for TopicThresholds {
    fn default() -> Self {
        Self {
            near_duplicate: 0.92,
            same_story: 0.86,
            topic_strong_match: 0.84,
            topic_ambiguous_match: 0.76,
            topic_weak_match: 0.70,
            context: 0.78,
        }
    }
}

#[derive(Debug, Clone)]
pub enum DecisionResult {
    LinkToTopic {
        topic_id: Uuid,
        relation_type: String,
        confidence: f64,
    },
    CreateNewTopic,
    Ambiguous {
        candidates: Vec<TopicCandidate>,
    },
}

pub fn decide_article_topic_relation(
    candidates: &[TopicCandidate],
    article_entities: Option<&Value>,
    thresholds: &TopicThresholds,
) -> DecisionResult {
    if candidates.is_empty() {
        return DecisionResult::CreateNewTopic;
    }

    let best = &candidates[0];
    let best_score = best.score;

    if best_score >= thresholds.near_duplicate {
        return DecisionResult::LinkToTopic {
            topic_id: best.topic_id,
            relation_type: "near_duplicate".to_string(),
            confidence: best_score,
        };
    }

    if best_score >= thresholds.same_story || best_score >= thresholds.topic_strong_match {
        return DecisionResult::LinkToTopic {
            topic_id: best.topic_id,
            relation_type: "same_story_update".to_string(),
            confidence: best_score,
        };
    }

    if best_score >= thresholds.context {
        let shared = shared_entities(article_entities, &serde_json::json!([]));
        let relation_type = if shared.is_empty() {
            "related_but_different".to_string()
        } else {
            "background_context".to_string()
        };
        return DecisionResult::LinkToTopic {
            topic_id: best.topic_id,
            relation_type,
            confidence: best_score,
        };
    }

    if best_score >= thresholds.topic_ambiguous_match {
        let ambiguous: Vec<TopicCandidate> = candidates
            .iter()
            .filter(|c| c.score >= thresholds.topic_ambiguous_match)
            .cloned()
            .collect();
        return DecisionResult::Ambiguous {
            candidates: ambiguous,
        };
    }

    DecisionResult::CreateNewTopic
}

pub fn shared_entities(article_entities: Option<&Value>, topic_entities: &Value) -> Vec<String> {
    let article_vec: Vec<String> = match article_entities {
        Some(Value::Array(arr)) => arr
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect(),
        _ => return Vec::new(),
    };

    let topic_vec: Vec<String> = match topic_entities {
        Value::Array(arr) => arr
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect(),
        _ => return Vec::new(),
    };

    article_vec
        .into_iter()
        .filter(|e| topic_vec.contains(e))
        .collect()
}

pub fn apply_llm_decision(
    llm_response: &LlmDecisionResponse,
    best_candidate: Option<&TopicCandidate>,
) -> DecisionResult {
    if llm_response.decision == "new_topic" {
        return DecisionResult::CreateNewTopic;
    }

    if let Some(candidate) = best_candidate {
        DecisionResult::LinkToTopic {
            topic_id: candidate.topic_id,
            relation_type: llm_response.decision.clone(),
            confidence: llm_response.confidence,
        }
    } else {
        DecisionResult::CreateNewTopic
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;

    fn make_candidate(score: f64) -> TopicCandidate {
        TopicCandidate {
            topic_id: Uuid::new_v4(),
            title: "Test Topic".to_string(),
            status: "active".to_string(),
            score,
            last_seen_at: Some(Utc::now()),
        }
    }

    #[test]
    fn no_candidates_creates_new_topic() {
        let thresholds = TopicThresholds::default();
        let result = decide_article_topic_relation(&[], None, &thresholds);
        assert!(matches!(result, DecisionResult::CreateNewTopic));
    }

    #[test]
    fn high_score_near_duplicate() {
        let thresholds = TopicThresholds::default();
        let candidates = vec![make_candidate(0.95)];
        let result = decide_article_topic_relation(&candidates, None, &thresholds);
        match result {
            DecisionResult::LinkToTopic {
                relation_type,
                confidence,
                ..
            } => {
                assert_eq!(relation_type, "near_duplicate");
                assert!((confidence - 0.95).abs() < f64::EPSILON);
            }
            _ => panic!("expected LinkToTopic"),
        }
    }

    #[test]
    fn strong_match_same_story_update() {
        let thresholds = TopicThresholds::default();
        let candidates = vec![make_candidate(0.85)];
        let result = decide_article_topic_relation(&candidates, None, &thresholds);
        match result {
            DecisionResult::LinkToTopic {
                relation_type,
                confidence,
                ..
            } => {
                assert_eq!(relation_type, "same_story_update");
                assert!((confidence - 0.85).abs() < f64::EPSILON);
            }
            _ => panic!("expected LinkToTopic"),
        }
    }

    #[test]
    fn ambiguous_range_returns_ambiguous() {
        let thresholds = TopicThresholds::default();
        let candidates = vec![make_candidate(0.77)];
        let result = decide_article_topic_relation(&candidates, None, &thresholds);
        match result {
            DecisionResult::Ambiguous { candidates: c } => {
                assert_eq!(c.len(), 1);
            }
            _ => panic!("expected Ambiguous"),
        }
    }

    #[test]
    fn below_weak_creates_new_topic() {
        let thresholds = TopicThresholds::default();
        let candidates = vec![make_candidate(0.65)];
        let result = decide_article_topic_relation(&candidates, None, &thresholds);
        assert!(matches!(result, DecisionResult::CreateNewTopic));
    }

    #[test]
    fn shared_entities_finds_intersection() {
        let article = serde_json::json!(["Iran", "Israel", "Tehran"]);
        let topic = serde_json::json!(["Israel", "Iran", "UN"]);
        let shared = shared_entities(Some(&article), &topic);
        assert_eq!(shared.len(), 2);
        assert!(shared.contains(&"Iran".to_string()));
        assert!(shared.contains(&"Israel".to_string()));
    }

    #[test]
    fn shared_entities_empty_when_none() {
        let topic = serde_json::json!(["Israel", "Iran"]);
        let shared = shared_entities(None, &topic);
        assert!(shared.is_empty());
    }

    #[test]
    fn apply_llm_decision_new_topic() {
        let llm = LlmDecisionResponse {
            decision: "new_topic".to_string(),
            confidence: 0.9,
            should_update_topic_summary: false,
            should_create_timeline_event: false,
            timeline_event_type: None,
            reason: "test".to_string(),
        };
        let candidate = make_candidate(0.78);
        let result = apply_llm_decision(&llm, Some(&candidate));
        assert!(matches!(result, DecisionResult::CreateNewTopic));
    }

    #[test]
    fn apply_llm_decision_same_story_update() {
        let llm = LlmDecisionResponse {
            decision: "same_story_update".to_string(),
            confidence: 0.85,
            should_update_topic_summary: true,
            should_create_timeline_event: true,
            timeline_event_type: Some("update".to_string()),
            reason: "test".to_string(),
        };
        let candidate = make_candidate(0.78);
        let result = apply_llm_decision(&llm, Some(&candidate));
        match result {
            DecisionResult::LinkToTopic {
                relation_type,
                confidence,
                ..
            } => {
                assert_eq!(relation_type, "same_story_update");
                assert!((confidence - 0.85).abs() < f64::EPSILON);
            }
            _ => panic!("expected LinkToTopic"),
        }
    }

    #[test]
    fn apply_llm_decision_no_candidate_fallback() {
        let llm = LlmDecisionResponse {
            decision: "same_story_update".to_string(),
            confidence: 0.85,
            should_update_topic_summary: true,
            should_create_timeline_event: true,
            timeline_event_type: Some("update".to_string()),
            reason: "test".to_string(),
        };
        let result = apply_llm_decision(&llm, None);
        assert!(matches!(result, DecisionResult::CreateNewTopic));
    }
}
