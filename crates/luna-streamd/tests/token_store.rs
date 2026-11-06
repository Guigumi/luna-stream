use luna_streamd::token_store::TokenStore;
use std::os::unix::fs::PermissionsExt;
use tempfile::tempdir;

#[test]
fn roundtrip() {
    let dir = tempdir().unwrap();
    let store = TokenStore::new(dir.path().to_path_buf());
    assert_eq!(store.load(), None);

    store.save("abc_token_123").unwrap();
    assert_eq!(store.load(), Some("abc_token_123".to_string()));
}

#[test]
fn missing_is_none() {
    let dir = tempdir().unwrap();
    let store = TokenStore::new(dir.path().to_path_buf());
    assert_eq!(store.load(), None);
}

#[test]
fn file_mode_0600() {
    let dir = tempdir().unwrap();
    let store = TokenStore::new(dir.path().to_path_buf());
    store.save("secret").unwrap();

    let file_path = dir.path().join("screencast.token");
    let metadata = std::fs::metadata(&file_path).unwrap();
    let mode = metadata.permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
}

#[test]
fn clear_removes() {
    let dir = tempdir().unwrap();
    let store = TokenStore::new(dir.path().to_path_buf());
    store.save("to_clear").unwrap();
    assert!(store.load().is_some());

    store.clear().unwrap();
    assert_eq!(store.load(), None);
}
