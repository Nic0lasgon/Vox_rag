use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;
use vox_rag::db::{article_queries, embedding_queries, job_queries, schema::Article};

fn make_test_article(
    id: Uuid,
    source_name: &str,
    title: &str,
    cleaned_text: Option<&str>,
    canonical_url: Option<&str>,
) -> Article {
    Article {
        id,
        source_name: source_name.to_string(),
        source_url: Some("https://example.com".to_string()),
        canonical_url: canonical_url.map(|s| s.to_string()),
        title: title.to_string(),
        description: Some("A test article".to_string()),
        author: Some("Test Author".to_string()),
        language: Some("en".to_string()),
        country: Some("US".to_string()),
        published_at: Some(Utc::now()),
        fetched_at: Some(Utc::now()),
        raw_text: Some("raw content".to_string()),
        cleaned_text: cleaned_text.map(|s| s.to_string()),
        title_hash: Some("hash123".to_string()),
        content_hash: Some("content456".to_string()),
        simhash: Some("sim789".to_string()),
        status: Some("active".to_string()),
        created_at: None,
        updated_at: None,
    }
}

fn make_test_article_with_words(
    id: Uuid,
    source_name: &str,
    title: &str,
    word_count: usize,
    canonical_url: Option<&str>,
) -> Article {
    let words: Vec<&str> = vec!["word"; word_count];
    let cleaned_text = words.join(" ");
    make_test_article(id, source_name, title, Some(&cleaned_text), canonical_url)
}

mod article_queries_tests {
    use super::*;

    #[sqlx::test(migrations = "./migrations")]
    async fn insert_and_retrieve_article(pool: PgPool) {
        let id = Uuid::new_v4();
        let article = make_test_article_with_words(
            id,
            "Test Source",
            "Breaking News: Something Happened",
            100,
            Some("https://example.com/unique-article-1"),
        );

        let inserted = article_queries::insert_article(&pool, &article)
            .await
            .expect("Failed to insert article");

        assert_eq!(inserted.id, id);
        assert_eq!(inserted.title, "Breaking News: Something Happened");
        assert_eq!(inserted.source_name, "Test Source");
        assert!(inserted.created_at.is_some());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn insert_article_upsert_on_canonical_url(pool: PgPool) {
        let id1 = Uuid::new_v4();
        let url = "https://example.com/upsert-article";

        let article1 =
            make_test_article_with_words(id1, "Source 1", "Original Title", 100, Some(url));

        let _inserted1 = article_queries::insert_article(&pool, &article1)
            .await
            .expect("Failed first insert");

        let id2 = Uuid::new_v4();
        let article2 =
            make_test_article_with_words(id2, "Source 2", "Updated Title", 100, Some(url));

        let inserted2 = article_queries::insert_article(&pool, &article2)
            .await
            .expect("Failed second insert");

        assert_eq!(inserted2.id, id2);
        assert_eq!(inserted2.title, "Updated Title");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn get_article_by_id_found(pool: PgPool) {
        let id = Uuid::new_v4();
        let article = make_test_article_with_words(
            id,
            "Find Me",
            "Findable Article",
            100,
            Some("https://example.com/findable"),
        );

        article_queries::insert_article(&pool, &article)
            .await
            .expect("Failed to insert");

        let found = article_queries::get_article_by_id(&pool, id)
            .await
            .expect("Query failed");

        assert!(found.is_some());
        let found = found.unwrap();
        assert_eq!(found.id, id);
        assert_eq!(found.title, "Findable Article");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn get_article_by_id_not_found(pool: PgPool) {
        let nonexistent = Uuid::new_v4();
        let found = article_queries::get_article_by_id(&pool, nonexistent)
            .await
            .expect("Query failed");

        assert!(found.is_none());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn get_articles_without_embedding_returns_only_without_profiles(pool: PgPool) {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        let article1 = make_test_article_with_words(
            id1,
            "Source A",
            "Article with profile",
            100,
            Some("https://example.com/with-profile"),
        );
        let article2 = make_test_article_with_words(
            id2,
            "Source B",
            "Article without profile",
            100,
            Some("https://example.com/without-profile"),
        );

        article_queries::insert_article(&pool, &article1)
            .await
            .unwrap();
        article_queries::insert_article(&pool, &article2)
            .await
            .unwrap();

        let embedding: Vec<f32> = vec![0.0_f32; 4096];
        embedding_queries::upsert_article_profile(
            &pool,
            id1,
            "Profile text for article 1",
            "test-model",
            4096,
            embedding,
        )
        .await
        .unwrap();

        let without = article_queries::get_articles_without_embedding(&pool, 10)
            .await
            .unwrap();

        assert!(!without.is_empty());
        let ids: Vec<Uuid> = without.iter().map(|a| a.id).collect();
        assert!(
            !ids.contains(&id1),
            "Should not include article with profile"
        );
        assert!(ids.contains(&id2), "Should include article without profile");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn get_articles_without_embedding_excludes_empty_cleaned_text(pool: PgPool) {
        let id = Uuid::new_v4();
        let article = make_test_article(
            id,
            "Source",
            "No cleaned text",
            None,
            Some("https://example.com/no-clean"),
        );

        article_queries::insert_article(&pool, &article)
            .await
            .unwrap();

        let without = article_queries::get_articles_without_embedding(&pool, 10)
            .await
            .unwrap();

        let ids: Vec<Uuid> = without.iter().map(|a| a.id).collect();
        assert!(
            !ids.contains(&id),
            "Should exclude articles without cleaned_text"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn update_article_cleaned_text_success(pool: PgPool) {
        let id = Uuid::new_v4();
        let article = make_test_article_with_words(
            id,
            "Source",
            "Update me",
            100,
            Some("https://example.com/update-text"),
        );

        article_queries::insert_article(&pool, &article)
            .await
            .unwrap();

        article_queries::update_article_cleaned_text(&pool, id, "New cleaned content here")
            .await
            .expect("Update failed");

        let updated = article_queries::get_article_by_id(&pool, id)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(updated.cleaned_text.unwrap(), "New cleaned content here");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn update_article_cleaned_text_nonexistent_errors(pool: PgPool) {
        let result =
            article_queries::update_article_cleaned_text(&pool, Uuid::new_v4(), "new text").await;

        assert!(result.is_err());
    }
}

mod embedding_queries_tests {
    use super::*;

    #[sqlx::test(migrations = "./migrations")]
    async fn upsert_article_profile_insert_new(pool: PgPool) {
        let id = Uuid::new_v4();
        let article = make_test_article_with_words(
            id,
            "Source",
            "Article for profile",
            100,
            Some("https://example.com/profile-insert"),
        );

        article_queries::insert_article(&pool, &article)
            .await
            .unwrap();

        let embedding: Vec<f32> = (0..4096).map(|i| i as f32 / 4096.0).collect();

        embedding_queries::upsert_article_profile(
            &pool,
            id,
            "Profile text here",
            "test-model",
            4096,
            embedding,
        )
        .await
        .unwrap();

        let profile = embedding_queries::get_article_profile(&pool, id)
            .await
            .unwrap()
            .expect("Profile should exist");

        assert_eq!(profile.article_id, id);
        assert_eq!(profile.profile_text, "Profile text here");
        assert_eq!(profile.embedding_model, Some("test-model".to_string()));
        assert_eq!(profile.embedding_dimension, Some(4096));
        assert!(profile.embedding.is_some());
        assert!(profile.embedded_at.is_some());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn upsert_article_profile_update_existing(pool: PgPool) {
        let id = Uuid::new_v4();
        let article = make_test_article_with_words(
            id,
            "Source",
            "Article for profile update",
            100,
            Some("https://example.com/profile-upsert"),
        );

        article_queries::insert_article(&pool, &article)
            .await
            .unwrap();

        let embedding1: Vec<f32> = vec![0.1_f32; 4096];
        embedding_queries::upsert_article_profile(
            &pool,
            id,
            "First profile text",
            "model-v1",
            4096,
            embedding1,
        )
        .await
        .unwrap();

        let embedding2: Vec<f32> = vec![0.9_f32; 4096];
        embedding_queries::upsert_article_profile(
            &pool,
            id,
            "Updated profile text",
            "model-v2",
            4096,
            embedding2,
        )
        .await
        .unwrap();

        let profile = embedding_queries::get_article_profile(&pool, id)
            .await
            .unwrap()
            .expect("Profile should exist");

        assert_eq!(profile.profile_text, "Updated profile text");
        assert_eq!(profile.embedding_model, Some("model-v2".to_string()));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn get_article_profile_not_found(pool: PgPool) {
        let profile = embedding_queries::get_article_profile(&pool, Uuid::new_v4())
            .await
            .unwrap();

        assert!(profile.is_none());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn find_similar_articles_finds_similar(pool: PgPool) {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        let article1 = make_test_article_with_words(
            id1,
            "Source A",
            "First Article",
            100,
            Some("https://example.com/similar-a"),
        );
        let article2 = make_test_article_with_words(
            id2,
            "Source B",
            "Second Article",
            100,
            Some("https://example.com/similar-b"),
        );

        article_queries::insert_article(&pool, &article1)
            .await
            .unwrap();
        article_queries::insert_article(&pool, &article2)
            .await
            .unwrap();

        let embedding1: Vec<f32> = vec![0.8_f32; 4096];
        embedding_queries::upsert_article_profile(
            &pool,
            id1,
            "Profile text 1",
            "test-model",
            4096,
            embedding1,
        )
        .await
        .unwrap();

        let embedding2: Vec<f32> = vec![0.8001_f32; 4096];
        embedding_queries::upsert_article_profile(
            &pool,
            id2,
            "Profile text 2",
            "test-model",
            4096,
            embedding2,
        )
        .await
        .unwrap();

        let similar =
            embedding_queries::find_similar_articles(&pool, vec![0.8_f32; 4096], 10, &[id1], 0.0)
                .await
                .unwrap();

        let similar_ids: Vec<Uuid> = similar.iter().map(|s| s.article_id).collect();
        assert!(similar_ids.contains(&id2));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn find_similar_excludes_excluded_ids(pool: PgPool) {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        let article1 = make_test_article_with_words(
            id1,
            "Source A",
            "Excluded Article",
            100,
            Some("https://example.com/excluded"),
        );
        let article2 = make_test_article_with_words(
            id2,
            "Source B",
            "Target Article",
            100,
            Some("https://example.com/target"),
        );

        article_queries::insert_article(&pool, &article1)
            .await
            .unwrap();
        article_queries::insert_article(&pool, &article2)
            .await
            .unwrap();

        let embedding: Vec<f32> = vec![0.5_f32; 4096];
        embedding_queries::upsert_article_profile(
            &pool,
            id1,
            "Profile 1",
            "test-model",
            4096,
            embedding.clone(),
        )
        .await
        .unwrap();
        embedding_queries::upsert_article_profile(
            &pool,
            id2,
            "Profile 2",
            "test-model",
            4096,
            embedding,
        )
        .await
        .unwrap();

        let similar = embedding_queries::find_similar_articles(
            &pool,
            vec![0.5_f32; 4096],
            10,
            &[id1, id2],
            0.0,
        )
        .await
        .unwrap();

        let similar_ids: Vec<Uuid> = similar.iter().map(|s| s.article_id).collect();
        assert!(!similar_ids.contains(&id1));
        assert!(!similar_ids.contains(&id2));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn find_similar_respects_min_score(pool: PgPool) {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        let article1 = make_test_article_with_words(
            id1,
            "Source A",
            "Query Article",
            100,
            Some("https://example.com/query-art"),
        );
        let article2 = make_test_article_with_words(
            id2,
            "Source B",
            "Distant Article",
            100,
            Some("https://example.com/distant"),
        );

        article_queries::insert_article(&pool, &article1)
            .await
            .unwrap();
        article_queries::insert_article(&pool, &article2)
            .await
            .unwrap();

        embedding_queries::upsert_article_profile(
            &pool,
            id1,
            "Profile 1",
            "test-model",
            4096,
            vec![1.0_f32; 4096],
        )
        .await
        .unwrap();

        embedding_queries::upsert_article_profile(
            &pool,
            id2,
            "Profile 2",
            "test-model",
            4096,
            vec![-1.0_f32; 4096],
        )
        .await
        .unwrap();

        let similar =
            embedding_queries::find_similar_articles(&pool, vec![1.0_f32; 4096], 10, &[id1], 0.99)
                .await
                .unwrap();

        let similar_ids: Vec<Uuid> = similar.iter().map(|s| s.article_id).collect();
        assert!(
            !similar_ids.contains(&id2),
            "Distant article should be filtered by min_score"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn find_historical_by_embedding_returns_older_articles(pool: PgPool) {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        let mut article1 = make_test_article_with_words(
            id1,
            "Source A",
            "Recent Article",
            100,
            Some("https://example.com/recent"),
        );
        article1.published_at = Some(Utc::now());

        let mut article2 = make_test_article_with_words(
            id2,
            "Source B",
            "Old Article",
            100,
            Some("https://example.com/old-one"),
        );
        article2.published_at = Some(Utc::now() - chrono::Duration::days(60));

        article_queries::insert_article(&pool, &article1)
            .await
            .unwrap();
        article_queries::insert_article(&pool, &article2)
            .await
            .unwrap();

        let embedding: Vec<f32> = vec![0.5_f32; 4096];
        embedding_queries::upsert_article_profile(
            &pool,
            id1,
            "Profile 1",
            "test-model",
            4096,
            embedding.clone(),
        )
        .await
        .unwrap();
        embedding_queries::upsert_article_profile(
            &pool,
            id2,
            "Profile 2",
            "test-model",
            4096,
            embedding,
        )
        .await
        .unwrap();

        let results = embedding_queries::find_historical_by_embedding(
            &pool,
            vec![0.5_f32; 4096],
            30,
            10,
            0.0,
        )
        .await
        .unwrap();

        let result_ids: Vec<Uuid> = results.iter().map(|r| r.article_id).collect();
        assert!(
            result_ids.contains(&id2),
            "Old article should appear in historical results"
        );
    }
}

mod job_queries_tests {
    use super::*;

    #[sqlx::test(migrations = "./migrations")]
    async fn create_embedding_job_with_correct_defaults(pool: PgPool) {
        let target_id = Uuid::new_v4();

        let job = job_queries::create_embedding_job(
            &pool,
            "article_profile",
            target_id,
            "test-model",
            "abc123hash",
        )
        .await
        .unwrap();

        assert_eq!(job.target_type, "article_profile");
        assert_eq!(job.target_id, target_id);
        assert_eq!(job.model, "test-model");
        assert_eq!(job.input_hash, "abc123hash");
        assert_eq!(job.status, "pending");
        assert_eq!(job.attempts, Some(0));
        assert!(job.last_error.is_none());
        assert!(job.created_at.is_some());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn pick_pending_jobs_returns_pending(pool: PgPool) {
        let id = Uuid::new_v4();

        job_queries::create_embedding_job(&pool, "article_profile", id, "test-model", "hash1")
            .await
            .unwrap();

        let jobs = job_queries::pick_pending_jobs(&pool, 10).await.unwrap();
        assert!(!jobs.is_empty());
        let pending_ids: Vec<Uuid> = jobs.iter().map(|j| j.target_id).collect();
        assert!(pending_ids.contains(&id));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn pick_pending_jobs_skips_completed(pool: PgPool) {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        let job1 = job_queries::create_embedding_job(
            &pool,
            "article_profile",
            id1,
            "test-model",
            "hash-a",
        )
        .await
        .unwrap();

        job_queries::create_embedding_job(&pool, "article_profile", id2, "test-model", "hash-b")
            .await
            .unwrap();

        job_queries::mark_job_done(&pool, job1.id).await.unwrap();

        let jobs = job_queries::pick_pending_jobs(&pool, 10).await.unwrap();
        let job_ids: Vec<Uuid> = jobs.iter().map(|j| j.id).collect();
        assert!(
            !job_ids.contains(&job1.id),
            "Completed job should not be picked"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn mark_job_running_updates_status(pool: PgPool) {
        let job = job_queries::create_embedding_job(
            &pool,
            "article_profile",
            Uuid::new_v4(),
            "test-model",
            "hash",
        )
        .await
        .unwrap();

        job_queries::mark_job_running(&pool, job.id, "worker-1")
            .await
            .unwrap();

        let jobs = job_queries::pick_pending_jobs(&pool, 10).await.unwrap();
        let running_ids: Vec<Uuid> = jobs.iter().map(|j| j.id).collect();
        assert!(
            !running_ids.contains(&job.id),
            "Running job should not appear as pending"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn mark_job_done_sets_completed_at(pool: PgPool) {
        let job = job_queries::create_embedding_job(
            &pool,
            "article_profile",
            Uuid::new_v4(),
            "test-model",
            "hash-done",
        )
        .await
        .unwrap();

        job_queries::mark_job_done(&pool, job.id).await.unwrap();

        let jobs = job_queries::pick_pending_jobs(&pool, 10).await.unwrap();
        let pending_ids: Vec<Uuid> = jobs.iter().map(|j| j.id).collect();
        assert!(
            !pending_ids.contains(&job.id),
            "Done job should not be pending"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn mark_job_failed_sets_error(pool: PgPool) {
        let job = job_queries::create_embedding_job(
            &pool,
            "article_profile",
            Uuid::new_v4(),
            "test-model",
            "hash-fail",
        )
        .await
        .unwrap();

        job_queries::mark_job_failed(&pool, job.id, "Something went wrong")
            .await
            .unwrap();

        let jobs = job_queries::pick_pending_jobs(&pool, 10).await.unwrap();
        let pending_ids: Vec<Uuid> = jobs.iter().map(|j| j.id).collect();
        assert!(
            !pending_ids.contains(&job.id),
            "Failed job should not be pending"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn reschedule_job_resets_to_pending_with_delay(pool: PgPool) {
        let job = job_queries::create_embedding_job(
            &pool,
            "article_profile",
            Uuid::new_v4(),
            "test-model",
            "hash-resched",
        )
        .await
        .unwrap();

        job_queries::mark_job_running(&pool, job.id, "worker-1")
            .await
            .unwrap();

        job_queries::reschedule_job(&pool, job.id, 300)
            .await
            .unwrap();

        let jobs = job_queries::pick_pending_jobs(&pool, 10).await.unwrap();
        let rescheduled_ids: Vec<Uuid> = jobs.iter().map(|j| j.id).collect();
        assert!(
            !rescheduled_ids.contains(&job.id),
            "Rescheduled job with future delay should not be immediately picked"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn reschedule_job_with_zero_delay_is_immediately_pickable(pool: PgPool) {
        let job = job_queries::create_embedding_job(
            &pool,
            "article_profile",
            Uuid::new_v4(),
            "test-model",
            "hash-zero",
        )
        .await
        .unwrap();

        job_queries::mark_job_running(&pool, job.id, "worker-1")
            .await
            .unwrap();

        job_queries::reschedule_job(&pool, job.id, 0).await.unwrap();

        let jobs = job_queries::pick_pending_jobs(&pool, 10).await.unwrap();
        let rescheduled_ids: Vec<Uuid> = jobs.iter().map(|j| j.id).collect();
        assert!(
            rescheduled_ids.contains(&job.id),
            "Rescheduled job with zero delay should be pickable"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn is_already_embedded_true_when_done(pool: PgPool) {
        let target_id = Uuid::new_v4();
        let job = job_queries::create_embedding_job(
            &pool,
            "article_profile",
            target_id,
            "test-model",
            "hash-already",
        )
        .await
        .unwrap();

        job_queries::mark_job_done(&pool, job.id).await.unwrap();

        let is_embedded =
            job_queries::is_already_embedded(&pool, "article_profile", target_id, "hash-already")
                .await
                .unwrap();

        assert!(is_embedded);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn is_already_embedded_false_when_pending(pool: PgPool) {
        let target_id = Uuid::new_v4();
        job_queries::create_embedding_job(
            &pool,
            "article_profile",
            target_id,
            "test-model",
            "hash-pending",
        )
        .await
        .unwrap();

        let is_embedded =
            job_queries::is_already_embedded(&pool, "article_profile", target_id, "hash-pending")
                .await
                .unwrap();

        assert!(!is_embedded);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn is_already_embedded_false_with_different_hash(pool: PgPool) {
        let target_id = Uuid::new_v4();
        let job = job_queries::create_embedding_job(
            &pool,
            "article_profile",
            target_id,
            "test-model",
            "hash-original",
        )
        .await
        .unwrap();

        job_queries::mark_job_done(&pool, job.id).await.unwrap();

        let is_embedded =
            job_queries::is_already_embedded(&pool, "article_profile", target_id, "hash-different")
                .await
                .unwrap();

        assert!(!is_embedded);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn pick_pending_jobs_skip_locked_precludes_double_pick(pool: PgPool) {
        let target_ids: Vec<Uuid> = (0..5).map(|_| Uuid::new_v4()).collect();
        for (i, id) in target_ids.iter().enumerate() {
            let job = job_queries::create_embedding_job(
                &pool,
                "article_profile",
                *id,
                "test-model",
                &format!("hash-skip-locked-{}", i),
            )
            .await
            .unwrap();
            job_queries::mark_job_done(&pool, job.id).await.unwrap();
        }

        for id in &target_ids {
            job_queries::create_embedding_job(
                &pool,
                "article_profile",
                *id,
                "test-model",
                &format!("hash-skip-locked-new-{}", id),
            )
            .await
            .unwrap();
        }

        let pool2 = pool.clone();
        let (jobs1, jobs2) = tokio::join!(
            async { job_queries::pick_pending_jobs(&pool, 3).await.unwrap() },
            async { job_queries::pick_pending_jobs(&pool2, 3).await.unwrap() },
        );

        let ids1: std::collections::HashSet<Uuid> = jobs1.iter().map(|j| j.id).collect();
        let ids2: std::collections::HashSet<Uuid> = jobs2.iter().map(|j| j.id).collect();

        let intersection: Vec<_> = ids1.intersection(&ids2).collect();
        assert!(
            intersection.is_empty(),
            "SKIP LOCKED should prevent picking the same job twice"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn pick_pending_jobs_respects_limit(pool: PgPool) {
        for i in 0..10 {
            job_queries::create_embedding_job(
                &pool,
                "article_profile",
                Uuid::new_v4(),
                "test-model",
                &format!("hash-limit-{}", i),
            )
            .await
            .unwrap();
        }

        let jobs = job_queries::pick_pending_jobs(&pool, 3).await.unwrap();
        assert_eq!(jobs.len(), 3);
    }
}
