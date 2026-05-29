use chrono::Utc;
use uuid::Uuid;
use vox_rag::db::schema::Article;
use vox_rag::pipeline::profile::{
    build_article_profile_text, normalize_for_embedding, should_embed,
};

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

fn make_article_minimal(cleaned_text: Option<String>) -> Article {
    Article {
        id: Uuid::new_v4(),
        source_name: "Minimal Source".to_string(),
        source_url: None,
        canonical_url: None,
        title: "Minimal Title".to_string(),
        description: None,
        author: None,
        language: None,
        country: None,
        published_at: None,
        fetched_at: None,
        raw_text: None,
        cleaned_text,
        title_hash: None,
        content_hash: None,
        simhash: None,
        status: None,
        created_at: None,
        updated_at: None,
    }
}

#[test]
fn profile_text_truncates_long_cleaned_text() {
    let long_text = "word ".repeat(2000);
    let article = make_article(Some(long_text));

    let profile = build_article_profile_text(&article);

    let lead_index = profile.find("Article lead:\n").unwrap();
    let lead_text = &profile[lead_index..];
    let lead_content = lead_text.strip_prefix("Article lead:\n").unwrap();

    assert!(lead_content.len() <= 1500);
    assert_eq!(lead_content.len(), 1500);
}

#[test]
fn profile_text_handles_exact_boundary_cleaned_text() {
    let exact_text = "x".repeat(1500);
    let article = make_article(Some(exact_text.clone()));

    let profile = build_article_profile_text(&article);

    assert!(profile.contains("Article lead:"));
    assert!(profile.contains(&exact_text));
}

#[test]
fn profile_text_with_special_characters_in_title() {
    let article = Article {
        title: r#"Breaking: "Earthquake" in Japan — 7.5 Magnitude!"#.to_string(),
        ..make_article(Some("Some cleaned text content here.".to_string()))
    };

    let profile = build_article_profile_text(&article);
    assert!(profile.contains(r#"Breaking: "Earthquake" in Japan — 7.5 Magnitude!"#));
}

#[test]
fn profile_text_with_all_optional_fields_none() {
    let article = make_article_minimal(Some("Enough words ".repeat(100).to_string()));

    let profile = build_article_profile_text(&article);

    assert!(profile.contains("Title: Minimal Title"));
    assert!(profile.contains("Source: Minimal Source"));
    assert!(profile.contains("Language: unknown"));
    assert!(profile.contains("Description: "));
}

#[test]
fn normalize_strips_html_in_description() {
    let input = r#"Description: <div class="content">Breaking <b>news</b> alert</div>"#;
    let result = normalize_for_embedding(input);

    assert!(!result.contains("<div"));
    assert!(!result.contains("<b>"));
    assert!(!result.contains("</b>"));
    assert!(!result.contains("</div>"));
    assert!(result.contains("Breaking"));
    assert!(result.contains("news"));
    assert!(result.contains("alert"));
}

#[test]
fn normalize_strips_script_and_style_tags() {
    let input = r#"<script>alert('xss')</script>Content<style>.red{color:red}</style>More"#;
    let result = normalize_for_embedding(input);

    assert!(!result.contains("<script"));
    assert!(!result.contains("<style"));
    assert!(!result.contains("</script"));
    assert!(!result.contains("</style"));
    assert!(result.contains("Content"));
    assert!(result.contains("More"));
}

#[test]
fn normalize_collapses_multiple_whitespace() {
    let input = "Hello    world\t\t\ttest\n\n\nnewline\r\ncarriage   return";
    let result = normalize_for_embedding(input);

    let parts: Vec<&str> = result.split_whitespace().collect();
    assert_eq!(
        parts,
        vec!["Hello", "world", "test", "newline", "carriage", "return"]
    );
}

#[test]
fn normalize_handles_unicode_content() {
    let input = "Café résumé naïve forêt garçon — français – allemand";
    let result = normalize_for_embedding(input);

    assert!(result.contains("Café"));
    assert!(result.contains("résumé"));
    assert!(result.contains("naïve"));
    assert!(result.contains("forêt"));
    assert!(result.contains("garçon"));
    assert!(result.contains("français"));
    assert!(result.contains("allemand"));
}

#[test]
fn normalize_handles_non_latin_scripts() {
    let input = "こんにちは世界 مرحبا بالعالم Привет мир שלום עולם";
    let result = normalize_for_embedding(input);

    assert!(result.contains("こんにちは世界"));
    assert!(result.contains("مرحبا"));
    assert!(result.contains("Привет"));
    assert!(result.contains("שלום"));
}

#[test]
fn normalize_handles_curly_quotes_and_dashes() {
    let input = "\u{201C}hello\u{201D} \u{2018}world\u{2019} \u{2013}test\u{2014}done";
    let result = normalize_for_embedding(input);

    assert!(result.contains("\"hello\""));
    assert!(result.contains("'world'"));
    assert!(result.contains("-test-done"));
}

#[test]
fn normalize_trims_leading_trailing_whitespace() {
    let result = normalize_for_embedding("   hello world   ");

    assert_eq!(result, "hello world");
}

#[test]
fn normalize_empty_string_returns_empty() {
    let result = normalize_for_embedding("");

    assert_eq!(result, "");
}

#[test]
fn normalize_whitespace_only_returns_empty() {
    let result = normalize_for_embedding("   \n\t  ");

    assert_eq!(result, "");
}

#[test]
fn should_embed_with_exactly_80_words() {
    let words: Vec<&str> = vec!["ab"; 80];
    let article = make_article(Some(words.join(" ")));
    assert!(should_embed(&article));
}

#[test]
fn should_embed_with_single_character_words_not_counted() {
    let words = "a b c d e f g h i j k l m n o p q r s t u v w x y z";
    let article = make_article(Some(words.to_string()));
    assert!(!should_embed(&article));
}

#[test]
fn should_embed_with_mixed_short_and_long_words() {
    let short = "a b c d e ";
    let long_words: Vec<&str> = vec!["ab"; 80];
    let text = format!("{}{}", short, long_words.join(" "));
    let article = make_article(Some(text));
    assert!(should_embed(&article));
}

#[test]
fn should_embed_with_just_below_threshold() {
    let words: Vec<&str> = vec!["ab"; 79];
    let article = make_article(Some(words.join(" ")));
    assert!(!should_embed(&article));
}

#[test]
fn profile_text_with_unicode_cleaned_text() {
    let article = Article {
        title: "Café et croissant".to_string(),
        description: Some("Un délicieux petit-déjeuner français".to_string()),
        language: Some("fr".to_string()),
        ..make_article(Some(
            "Le café est servi avec un croissant chaud. "
                .repeat(80)
                .to_string(),
        ))
    };

    let profile = build_article_profile_text(&article);

    assert!(profile.contains("Café et croissant"));
    assert!(profile.contains("Un délicieux petit-déjeuner français"));
    assert!(profile.contains("Language: fr"));
    assert!(profile.contains("Le café est servi"));
}
