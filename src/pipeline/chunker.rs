#[derive(Debug, Clone)]
pub struct ChunkConfig {
    pub min_chunk_size: usize,
    pub max_chunk_size: usize,
    pub overlap: usize,
}

impl Default for ChunkConfig {
    fn default() -> Self {
        Self {
            min_chunk_size: 700,
            max_chunk_size: 1200,
            overlap: 150,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TextChunk {
    pub chunk_index: usize,
    pub text: String,
    pub token_count: usize,
}

pub fn chunk_text(text: &str, config: &ChunkConfig) -> Vec<TextChunk> {
    if text.trim().is_empty() {
        return vec![];
    }

    let words: Vec<&str> = text.split_whitespace().collect();
    let total_words = words.len();

    if total_words <= config.max_chunk_size {
        return vec![TextChunk {
            chunk_index: 0,
            text: text.to_string(),
            token_count: total_words,
        }];
    }

    let mut chunks = Vec::new();
    let step = config.max_chunk_size.saturating_sub(config.overlap);
    let mut start = 0;
    let mut index = 0;

    while start < total_words {
        let end = (start + config.max_chunk_size).min(total_words);
        let chunk_words = &words[start..end];
        let chunk_text = chunk_words.join(" ");

        chunks.push(TextChunk {
            chunk_index: index,
            text: chunk_text,
            token_count: chunk_words.len(),
        });

        if end >= total_words {
            break;
        }

        start += step;
        index += 1;
    }

    chunks
}
