use anyhow::Result;
use clap::Parser;
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use tracing_subscriber::EnvFilter;

use vox_rag::api::{self, AppState};
use vox_rag::config::Config;
use vox_rag::embedding::{EmbeddingProvider, MockEmbeddingProvider, OctenEmbeddingProvider};
use vox_rag::llm::deepseek::DeepSeekProvider;
use vox_rag::llm::mock::MockLlmProvider;
use vox_rag::llm::provider::LlmProvider;
use vox_rag::pipeline::similarity::Thresholds;
use vox_rag::topic::decision::TopicThresholds;
use vox_rag::workers::embedding_worker::EmbeddingWorker;
use vox_rag::workers::topic_worker::TopicWorker;

#[derive(Parser, Debug)]
#[command(
    name = "vox-rag",
    about = "VoxPod RAG - Article embedding and semantic search"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Parser, Debug)]
enum Commands {
    Api,
    Worker {
        #[arg(long, default_value = "5")]
        poll_interval: u64,
        #[arg(long, default_value = "10")]
        batch_size: i64,
    },
    TopicWorker {
        #[arg(long, default_value = "5")]
        poll_interval: u64,
        #[arg(long, default_value = "10")]
        batch_size: i64,
    },
    All {
        #[arg(long, default_value = "5")]
        poll_interval: u64,
        #[arg(long, default_value = "10")]
        batch_size: i64,
    },
    TestEmbed {
        #[arg(long)]
        text: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    if let Commands::TestEmbed { text } = cli.command {
        let provider = MockEmbeddingProvider::new(4096);
        let embedding = provider.embed_text(&text).await?;
        println!("Embedding dimension: {}", embedding.len());
        println!("First 10 values: {:?}", &embedding[..10]);
        return Ok(());
    }

    let config = Config::from_env()?;

    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&config.database_url)
        .await?;

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .map_err(|e| anyhow::anyhow!("Migration failed: {}", e))?;

    let provider: Arc<dyn EmbeddingProvider> = Arc::new(OctenEmbeddingProvider::new(
        config.octen_api_key.clone(),
        config.octen_api_base_url.clone(),
        config.embedding_model.clone(),
        config.embedding_dimension,
        config.embedding_timeout_ms,
    )?);

    let thresholds = Thresholds {
        near_duplicate: config.similarity_near_duplicate_threshold,
        same_story: config.similarity_same_story_threshold,
        context: config.similarity_context_threshold,
        weak: config.similarity_weak_threshold,
    };

    let topic_thresholds = TopicThresholds {
        near_duplicate: config.similarity_near_duplicate_threshold,
        same_story: config.similarity_same_story_threshold,
        topic_strong_match: config.topic_strong_match_threshold,
        topic_ambiguous_match: config.topic_ambiguous_match_threshold,
        topic_weak_match: config.topic_weak_match_threshold,
        context: config.similarity_context_threshold,
    };

    let llm_provider: Option<Arc<dyn LlmProvider>> = if config.use_llm_for_topics {
        if config.llm_api_key.is_empty() {
            tracing::warn!("USE_LLM_FOR_TOPICS is true but LLM_API_KEY is empty, using mock LLM");
            Some(Arc::new(MockLlmProvider::new("same_story_update")))
        } else {
            Some(Arc::new(DeepSeekProvider::new(
                config.llm_api_key.clone(),
                config.llm_model.clone(),
                config.llm_base_url.clone(),
                config.llm_timeout_ms,
            )?))
        }
    } else {
        None
    };

    match cli.command {
        Commands::Api => {
            run_api(
                pool,
                provider,
                thresholds,
                topic_thresholds,
                config.topic_markdown_max_words,
                config.port,
            )
            .await
        }
        Commands::Worker {
            poll_interval,
            batch_size,
        } => {
            let worker = EmbeddingWorker::new(pool, provider, poll_interval, batch_size);
            worker.run().await;
            Ok(())
        }
        Commands::TopicWorker {
            poll_interval,
            batch_size,
        } => {
            let worker = TopicWorker::new(
                pool,
                provider,
                llm_provider,
                poll_interval,
                batch_size,
                topic_thresholds,
                config.topic_markdown_max_words,
                config.topic_active_days,
                config.topic_archive_days,
            );
            worker.run().await;
            Ok(())
        }
        Commands::All {
            poll_interval,
            batch_size,
        } => {
            let p = pool.clone();
            let pr = provider.clone();
            let _worker_handle = tokio::spawn(async move {
                let worker = EmbeddingWorker::new(p, pr, poll_interval, batch_size);
                worker.run().await;
            });

            let p2 = pool.clone();
            let pr2 = provider.clone();
            let llm2 = llm_provider.clone();
            let topic_thresholds_clone = topic_thresholds.clone();
            let _topic_worker_handle = tokio::spawn(async move {
                let worker = TopicWorker::new(
                    p2,
                    pr2,
                    llm2,
                    poll_interval,
                    batch_size,
                    topic_thresholds_clone,
                    config.topic_markdown_max_words,
                    config.topic_active_days,
                    config.topic_archive_days,
                );
                worker.run().await;
            });

            run_api(
                pool,
                provider,
                thresholds,
                topic_thresholds,
                config.topic_markdown_max_words,
                config.port,
            )
            .await
        }
        Commands::TestEmbed { .. } => unreachable!(),
    }
}

async fn run_api(
    pool: sqlx::PgPool,
    provider: Arc<dyn EmbeddingProvider>,
    thresholds: Thresholds,
    topic_thresholds: TopicThresholds,
    topic_max_words: usize,
    port: u16,
) -> Result<()> {
    let state = Arc::new(AppState {
        pool,
        provider,
        thresholds,
        topic_thresholds,
        topic_max_words,
    });

    let app = api::create_router(state);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
    tracing::info!("API server listening on port {}", port);
    axum::serve(listener, app).await?;

    Ok(())
}
