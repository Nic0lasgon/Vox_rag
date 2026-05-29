use vox_rag::pipeline::chunker::{ChunkConfig, chunk_text};

fn generate_words(count: usize) -> String {
    (1..=count)
        .map(|i| format!("word{i}"))
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn empty_text_returns_empty_vec() {
    let config = ChunkConfig::default();
    let result = chunk_text("", &config);
    assert!(result.is_empty());
}

#[test]
fn whitespace_only_returns_empty_vec() {
    let config = ChunkConfig::default();
    let result = chunk_text("   \n\t  ", &config);
    assert!(result.is_empty());
}

#[test]
fn short_text_returns_single_chunk() {
    let config = ChunkConfig::default();
    let text = generate_words(100);
    let result = chunk_text(&text, &config);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].chunk_index, 0);
    assert_eq!(result[0].token_count, 100);
}

#[test]
fn exactly_at_boundary_returns_single_chunk() {
    let config = ChunkConfig::default();
    let text = generate_words(1200);
    let result = chunk_text(&text, &config);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].token_count, 1200);
}

#[test]
fn one_over_boundary_returns_two_chunks() {
    let config = ChunkConfig::default();
    let text = generate_words(1201);
    let result = chunk_text(&text, &config);
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].chunk_index, 0);
    assert_eq!(result[1].chunk_index, 1);
}

#[test]
fn long_text_produces_multiple_chunks() {
    let config = ChunkConfig::default();
    let text = generate_words(3000);
    let result = chunk_text(&text, &config);
    assert!(result.len() > 1);
    for chunk in &result {
        assert!(chunk.token_count <= config.max_chunk_size);
        assert!(!chunk.text.is_empty());
    }
}

#[test]
fn chunk_sizes_within_valid_range() {
    let config = ChunkConfig::default();
    let text = generate_words(5000);
    let result = chunk_text(&text, &config);
    for chunk in &result {
        assert!(chunk.token_count >= 1);
        assert!(chunk.token_count <= config.max_chunk_size);
    }
}

#[test]
fn overlap_exists_between_consecutive_chunks() {
    let config = ChunkConfig::default();
    let text = generate_words(3000);
    let result = chunk_text(&text, &config);
    assert!(result.len() >= 2);

    let first_words: Vec<&str> = result[0].text.split_whitespace().collect();
    let second_words: Vec<&str> = result[1].text.split_whitespace().collect();

    let overlap_start = first_words.len() - config.overlap;
    let expected_overlap = &first_words[overlap_start..];
    let actual_overlap = &second_words[..config.overlap.min(second_words.len())];

    assert_eq!(expected_overlap, actual_overlap);
}

#[test]
fn chunk_indices_are_sequential() {
    let config = ChunkConfig::default();
    let text = generate_words(5000);
    let result = chunk_text(&text, &config);
    for (i, chunk) in result.iter().enumerate() {
        assert_eq!(chunk.chunk_index, i);
    }
}

#[test]
fn content_preserved_first_word_matches() {
    let config = ChunkConfig::default();
    let text = generate_words(2000);
    let result = chunk_text(&text, &config);
    assert!(!result.is_empty());
    assert!(result[0].text.starts_with("word1"));
}

#[test]
fn all_chunks_have_non_empty_text() {
    let config = ChunkConfig::default();
    let text = generate_words(4000);
    let result = chunk_text(&text, &config);
    for chunk in &result {
        assert!(!chunk.text.trim().is_empty());
    }
}

#[test]
fn custom_config_values_work() {
    let config = ChunkConfig {
        min_chunk_size: 100,
        max_chunk_size: 200,
        overlap: 50,
    };
    let text = generate_words(1000);
    let result = chunk_text(&text, &config);
    assert!(result.len() > 1);
    for chunk in &result {
        assert!(chunk.token_count <= 200);
    }

    let first_words: Vec<&str> = result[0].text.split_whitespace().collect();
    let second_words: Vec<&str> = result[1].text.split_whitespace().collect();
    let overlap_start = first_words.len() - 50;
    let expected_overlap = &first_words[overlap_start..];
    let actual_overlap = &second_words[..50];
    assert_eq!(expected_overlap, actual_overlap);
}

#[test]
fn small_custom_config_single_chunk() {
    let config = ChunkConfig {
        min_chunk_size: 10,
        max_chunk_size: 50,
        overlap: 5,
    };
    let text = generate_words(30);
    let result = chunk_text(&text, &config);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].token_count, 30);
}

#[test]
fn total_content_coverage() {
    let config = ChunkConfig {
        min_chunk_size: 100,
        max_chunk_size: 300,
        overlap: 50,
    };
    let text = generate_words(1000);
    let result = chunk_text(&text, &config);
    let mut reconstructed = String::new();
    for (i, chunk) in result.iter().enumerate() {
        if i == 0 {
            reconstructed.push_str(&chunk.text);
        } else {
            let new_words: Vec<&str> = chunk.text.split_whitespace().collect();
            let non_overlap = &new_words[50..];
            reconstructed.push(' ');
            reconstructed.push_str(&non_overlap.join(" "));
        }
    }
    let original_words: Vec<&str> = text.split_whitespace().collect();
    let reconstructed_words: Vec<&str> = reconstructed.split_whitespace().collect();
    assert_eq!(original_words.len(), reconstructed_words.len());
    assert_eq!(original_words, reconstructed_words);
}
