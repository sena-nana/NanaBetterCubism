const KEYRING_SERVICE: &str = "com.senanana.nanabettercubism";
const KEYRING_ACCOUNT: &str = "cubism-editor-plugin-token";

pub(super) fn load_token() -> String {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
        .and_then(|entry| entry.get_password())
        .unwrap_or_default()
}

pub(super) fn save_token(token: &str) {
    if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT) {
        let _ = entry.set_password(token);
    }
}
