use crate::db::schema::Article;
use regex::Regex;
use tracing::debug;

pub fn build_article_profile_text(article: &Article) -> String {
    let title = article.title.as_str();
    let source_name = article.source_name.as_str();
    let published_at = article
        .published_at
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_else(|| "unknown".to_string());
    let language = article.language.as_deref().unwrap_or("unknown");
    let description = article.description.as_deref().unwrap_or("");

    let lead = article
        .cleaned_text
        .as_deref()
        .map(|text| {
            if text.len() > 1500 {
                &text[..1500]
            } else {
                text
            }
        })
        .unwrap_or("");

    format!(
        "Title: {title}\nSource: {source_name}\nPublished at: {published_at}\nLanguage: {language}\nDescription: {description}\nArticle lead:\n{lead}"
    )
}

pub fn normalize_for_embedding(text: &str) -> String {
    let re_html = Regex::new(r"<[^>]+>").unwrap();
    let cleaned = re_html.replace_all(text, " ");

    let re_script = Regex::new(r"(?is)<script[^>]*>.*?</script>").unwrap();
    let cleaned = re_script.replace_all(&cleaned, " ");
    let re_style = Regex::new(r"(?is)<style[^>]*>.*?</style>").unwrap();
    let cleaned = re_style.replace_all(&cleaned, " ");

    let re_multi_space = Regex::new(r"\s+").unwrap();
    let cleaned = re_multi_space.replace_all(&cleaned, " ");

    let cleaned = cleaned
        .replace(['\u{201C}', '\u{201D}'], "\"")
        .replace(['\u{2018}', '\u{2019}'], "'")
        .replace(['\u{2013}', '\u{2014}'], "-");

    cleaned.trim().to_string()
}

pub fn should_embed(article: &Article) -> bool {
    let cleaned_text = match &article.cleaned_text {
        Some(text) if !text.is_empty() => text,
        _ => {
            debug!(article_id = %article.id, "Skipping article: no cleaned_text");
            return false;
        }
    };

    let useful_word_count = cleaned_text
        .split_whitespace()
        .filter(|w| w.len() > 1)
        .count();

    if useful_word_count < 80 {
        debug!(
            article_id = %article.id,
            word_count = useful_word_count,
            "Skipping article: too few useful words"
        );
        return false;
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    fn make_article(cleaned_text: Option<String>) -> Article {
        Article {
            id: Uuid::new_v4(),
            source_name: "Test Source".to_string(),
            source_url: Some("https://example.com".to_string()),
            canonical_url: Some("https://example.com/article".to_string()),
            title: "Test Article Title".to_string(),
            description: Some("A test description".to_string()),
            author: Some("Test Author".to_string()),
            language: Some("en".to_string()),
            country: Some("US".to_string()),
            published_at: Some(Utc::now()),
            fetched_at: Some(Utc::now()),
            raw_text: Some("raw text".to_string()),
            cleaned_text,
            title_hash: Some("abc123".to_string()),
            content_hash: Some("def456".to_string()),
            simhash: Some("ghi789".to_string()),
            status: Some("processed".to_string()),
            created_at: Some(Utc::now()),
            updated_at: Some(Utc::now()),
        }
    }

    #[test]
    fn test_build_article_profile_text() {
        let article = make_article(Some("This is the cleaned text content.".to_string()));
        let profile = build_article_profile_text(&article);

        assert!(profile.contains("Title: Test Article Title"));
        assert!(profile.contains("Source: Test Source"));
        assert!(profile.contains("Language: en"));
        assert!(profile.contains("Description: A test description"));
        assert!(profile.contains("Article lead:"));
        assert!(profile.contains("This is the cleaned text content."));
    }

    #[test]
    fn test_normalize_for_embedding_strips_html() {
        let input = "<p>Hello <b>world</b></p><script>alert('xss')</script>";
        let result = normalize_for_embedding(input);
        assert!(!result.contains("<p>"));
        assert!(!result.contains("<b>"));
        assert!(!result.contains("<script>"));
        assert!(result.contains("Hello"));
        assert!(result.contains("world"));
    }

    #[test]
    fn test_normalize_for_embedding_normalizes_quotes() {
        let input = "He said \u{201C}hello\u{201D} and \u{2018}goodbye\u{2019}";
        let result = normalize_for_embedding(input);
        assert!(result.contains("\"hello\""));
        assert!(result.contains("'goodbye'"));
    }

    #[test]
    fn test_should_embed_with_enough_text() {
        let words: Vec<&str> = vec!["word"; 100];
        let article = make_article(Some(words.join(" ")));
        assert!(should_embed(&article));
    }

    #[test]
    fn test_should_embed_with_too_few_words() {
        let article = make_article(Some("short text".to_string()));
        assert!(!should_embed(&article));
    }

    #[test]
    fn test_should_embed_with_no_text() {
        let article = make_article(None);
        assert!(!should_embed(&article));
    }

    #[test]
    fn test_should_embed_with_empty_text() {
        let article = make_article(Some(String::new()));
        assert!(!should_embed(&article));
    }
}
