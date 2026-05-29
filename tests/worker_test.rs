use vox_rag::workers::embedding_worker::compute_input_hash;

#[test]
fn compute_input_hash_is_deterministic() {
    let hash1 = compute_input_hash("hello world");
    let hash2 = compute_input_hash("hello world");

    assert_eq!(hash1, hash2);
}

#[test]
fn compute_input_hash_different_inputs_produce_different_hashes() {
    let hash1 = compute_input_hash("hello world");
    let hash2 = compute_input_hash("hello world!");

    assert_ne!(hash1, hash2);
}

#[test]
fn compute_input_hash_empty_string() {
    let hash = compute_input_hash("");

    assert!(!hash.is_empty());
    assert_eq!(hash.len(), 64);
}

#[test]
fn compute_input_hash_is_hex_encoded() {
    let hash = compute_input_hash("test");

    assert_eq!(hash.len(), 64);
    assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    assert!(hash.chars().all(|c| c.is_lowercase() || c.is_ascii_digit()));
}

#[test]
fn compute_input_hash_same_length() {
    let short = compute_input_hash("a");
    let long = compute_input_hash("this is a much longer text with many many words");

    assert_eq!(short.len(), 64);
    assert_eq!(long.len(), 64);
}

#[test]
fn compute_input_hash_similar_inputs_different_outputs() {
    let hash1 = compute_input_hash("test");
    let hash2 = compute_input_hash("Test");
    let hash3 = compute_input_hash("test ");

    assert_ne!(hash1, hash2);
    assert_ne!(hash1, hash3);
    assert_ne!(hash2, hash3);
}
