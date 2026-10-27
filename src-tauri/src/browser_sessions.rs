//! Encrypted cookie-only session vault; pages retain ephemeral storage and never receive native credentials.
use crate::credential_cipher::{CredentialCipher, EncryptedCredential, ALGORITHM};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
use zeroize::{Zeroize, Zeroizing};
const PURPOSE: &[u8] = b"topic-desk-studio:browser-cookies:v1";
const SESSION_LIFETIME: i64 = 30 * 24 * 60 * 60;
const MAX_COOKIES: usize = 2_000;
const MAX_BYTES: usize = 2_000_000;

/// Cookie values exist only in native memory; serialized plaintext buffers are zeroized.
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct SavedCookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    pub secure: bool,
    pub http_only: bool,
    pub same_site: Option<String>,
    pub expires: Option<i64>,
    /// Session cookies without a server expiry are remembered for at most thirty days.
    pub remember_until: i64,
}
impl Drop for SavedCookie {
    fn drop(&mut self) {
        self.value.zeroize();
    }
}
impl SavedCookie {
    fn key(&self) -> (String, String, String) {
        (self.domain.clone(), self.path.clone(), self.name.clone())
    }
    fn valid(&self) -> bool {
        let domain = self.domain.strip_prefix('.').unwrap_or(&self.domain);
        let host_valid = url::Url::parse(&format!("https://{domain}/"))
            .ok()
            .is_some_and(|url| {
                url.host_str() == Some(domain)
                    && url.port().is_none()
                    && url.username().is_empty()
                    && url.password().is_none()
            });
        host_valid
            && !domain.is_empty()
            && self.domain.len() <= 253
            && !self.name.is_empty()
            && self.name.len() <= 256
            && self.value.len() <= 16_384
            && self.path.starts_with('/')
            && self.path.len() <= 1_024
            && !self
                .name
                .chars()
                .any(|c| c.is_control() || matches!(c, ';' | '='))
            && !self.value.chars().any(|c| c.is_control())
            && !self.path.chars().any(|c| c.is_control())
            && self
                .expires
                .is_none_or(|value| time::OffsetDateTime::from_unix_timestamp(value).is_ok())
            && matches!(
                self.same_site.as_deref(),
                None | Some("Strict" | "Lax" | "None")
            )
    }
    fn live(&self, now: i64) -> bool {
        self.expires.unwrap_or(self.remember_until) > now
    }
    fn equivalent(&self, other: &Self) -> bool {
        self.key() == other.key()
            && self.value == other.value
            && self.secure == other.secure
            && self.http_only == other.http_only
            && self.same_site == other.same_site
            && self.expires == other.expires
    }
}
/// Independent database/key deliberately excluded from history backups and topic restores.
pub(crate) struct CookieVault {
    database: Connection,
    key_path: PathBuf,
    jar: Option<Vec<SavedCookie>>,
    baselines: BTreeMap<String, Vec<SavedCookie>>,
}
impl CookieVault {
    /// Initialize ciphertext storage without accessing model credentials or any OS vault.
    pub fn open(directory: &Path) -> Result<Self, String> {
        let database = Connection::open(directory.join("browser-sessions.sqlite"))
            .map_err(|_| "无法打开登录会话存储")?;
        database
            .busy_timeout(std::time::Duration::from_secs(5))
            .map_err(|_| "登录会话存储繁忙")?;
        database.execute_batch("CREATE TABLE IF NOT EXISTS cookie_vault(id INTEGER PRIMARY KEY CHECK(id=1), algorithm TEXT NOT NULL, nonce BLOB NOT NULL, ciphertext BLOB NOT NULL);").map_err(|_| "无法初始化登录会话存储")?;
        Ok(Self {
            database,
            key_path: directory.join("browser-session.key"),
            jar: None,
            baselines: BTreeMap::new(),
        })
    }
    fn load(&mut self, now: i64) -> Result<(), String> {
        if self.jar.is_some() {
            return Ok(());
        }
        let row: Option<(String, Vec<u8>, Vec<u8>)> = self
            .database
            .query_row(
                "SELECT algorithm,nonce,ciphertext FROM cookie_vault WHERE id=1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(|_| "无法读取登录会话")?;
        let cookies = if let Some((algorithm, nonce, ciphertext)) = row {
            if algorithm != ALGORITHM || ciphertext.len() > MAX_BYTES + 32 {
                return Err("登录会话加密格式不受支持，请清除 Cookie 后重新登录".into());
            }
            let cipher = CredentialCipher::load_existing(&self.key_path)
                .map_err(|_| "无法读取登录会话密钥")?
                .ok_or("登录会话密钥丢失，请清除 Cookie 后重新登录")?;
            let plaintext = cipher
                .decrypt_scoped(&EncryptedCredential { nonce, ciphertext }, PURPOSE)
                .map_err(|_| "登录会话完整性校验失败，请清除 Cookie 后重新登录")?;
            let cookies: Vec<SavedCookie> =
                serde_json::from_str(&plaintext).map_err(|_| "登录会话内容无效")?;
            if cookies.len() > MAX_COOKIES || cookies.iter().any(|cookie| !cookie.valid()) {
                return Err("登录会话内容无效".into());
            }
            cookies
                .into_iter()
                .filter(|cookie| cookie.live(now))
                .collect()
        } else {
            vec![]
        };
        self.jar = Some(cookies);
        Ok(())
    }
    /// Authenticate and restore live cookies before the first external request.
    pub fn restore(&mut self, tab: &str, now: i64) -> Result<Vec<SavedCookie>, String> {
        self.load(now)?;
        let cookies = self
            .jar
            .as_ref()
            .unwrap()
            .iter()
            .filter(|cookie| cookie.live(now))
            .cloned()
            .collect::<Vec<_>>();
        self.baselines.insert(tab.into(), cookies.clone());
        Ok(cookies)
    }
    /// Merge per-tab changes rather than whole jars: unchanged background tabs
    /// cannot overwrite refreshed tokens or resurrect cookies removed by logout.
    pub fn capture(
        &mut self,
        tab: &str,
        mut current: Vec<SavedCookie>,
        now: i64,
    ) -> Result<(), String> {
        self.load(now)?;
        if current.len() > MAX_COOKIES || current.iter().any(|cookie| !cookie.valid()) {
            return Err("网站 Cookie 超出安全限制，登录状态未保存".into());
        }
        for cookie in &mut current {
            if cookie.expires.is_none() {
                cookie.remember_until = now.saturating_add(SESSION_LIFETIME);
            }
        }
        let Some(previous) = self.baselines.get(tab) else {
            return Ok(());
        };
        let mut next = self
            .jar
            .as_ref()
            .unwrap()
            .iter()
            .filter(|cookie| cookie.live(now))
            .cloned()
            .collect::<Vec<_>>();
        let mut changed = next.len() != self.jar.as_ref().unwrap().len();
        for old in previous {
            if !current.iter().any(|cookie| cookie.key() == old.key()) {
                let before = next.len();
                next.retain(|cookie| !(cookie.key() == old.key() && cookie.equivalent(old)));
                changed |= before != next.len();
            }
        }
        for cookie in &current {
            if previous.iter().any(|old| old.equivalent(cookie)) {
                continue;
            }
            next.retain(|old| old.key() != cookie.key());
            if cookie.live(now) {
                next.push(cookie.clone());
            }
            changed = true;
        }
        if next.len() > MAX_COOKIES {
            return Err("登录会话数量超出安全限制".into());
        }
        if changed {
            let plaintext =
                Zeroizing::new(serde_json::to_string(&next).map_err(|_| "无法编码登录会话")?);
            if plaintext.len() > MAX_BYTES {
                return Err("登录会话超出安全限制".into());
            }
            let has_payload: bool = self
                .database
                .query_row("SELECT EXISTS(SELECT 1 FROM cookie_vault)", [], |row| {
                    row.get(0)
                })
                .map_err(|_| "无法读取登录会话")?;
            let cipher = if has_payload {
                CredentialCipher::load_existing(&self.key_path)
                    .map_err(|_| "无法读取登录会话密钥")?
                    .ok_or("登录会话密钥丢失，请清除 Cookie 后重新登录")?
            } else {
                CredentialCipher::load_or_create(&self.key_path)
                    .map_err(|_| "无法保存登录会话密钥")?
            };
            let encrypted = cipher
                .encrypt_scoped(&plaintext, PURPOSE)
                .map_err(|_| "登录会话加密失败")?;
            // Commit one authenticated snapshot atomically. Failures retain the last good payload.
            self.database
                .execute(
                    "INSERT OR REPLACE INTO cookie_vault VALUES(1,?1,?2,?3)",
                    params![ALGORITHM, encrypted.nonce, encrypted.ciphertext],
                )
                .map_err(|_| "无法保存登录会话")?;
            self.jar = Some(next);
        }
        for cookie in &mut current {
            if let Some(old) = previous.iter().find(|old| old.equivalent(cookie)) {
                cookie.remember_until = old.remember_until;
            }
        }
        self.baselines.insert(tab.into(), current);
        Ok(())
    }
    /// Forget only the closed tab's baseline, retaining the remembered login state.
    pub fn forget_tab(&mut self, tab: &str) {
        self.baselines.remove(tab);
    }
    /// Clearing remains possible when ciphertext/key failures prevent restoration.
    pub fn clear(&mut self) -> Result<(), String> {
        self.database
            .execute("DELETE FROM cookie_vault", [])
            .map_err(|_| "无法清除已保存的登录状态")?;
        self.jar = Some(vec![]);
        self.baselines.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "topic-desk-cookies-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
        fn vault(&self) -> CookieVault {
            CookieVault::open(&self.0).unwrap()
        }
        fn key(&self) -> PathBuf {
            self.0.join("browser-session.key")
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn cookie(value: &str) -> SavedCookie {
        SavedCookie {
            name: "session".into(),
            value: value.into(),
            domain: "example.org".into(),
            path: "/".into(),
            secure: true,
            http_only: true,
            same_site: Some("Lax".into()),
            expires: None,
            remember_until: 0,
        }
    }
    #[test]
    fn survives_tab_close_and_restart_without_plaintext_storage() {
        let fixture = Fixture::new();
        let mut vault = fixture.vault();
        assert!(vault.restore("one", 100).unwrap().is_empty());
        vault
            .capture("one", vec![cookie("test-login-token")], 100)
            .unwrap();
        vault.forget_tab("one");
        let restored = vault.restore("two", 101).unwrap();
        assert_eq!(restored[0].value, "test-login-token");
        assert!(restored[0].secure && restored[0].http_only);
        assert_eq!(restored[0].same_site.as_deref(), Some("Lax"));
        drop(vault);
        assert_eq!(
            fixture.vault().restore("restart", 102).unwrap()[0].value,
            "test-login-token"
        );
        let disk = std::fs::read(fixture.0.join("browser-sessions.sqlite")).unwrap();
        for secret in ["test-login-token", "example.org", "session"] {
            assert!(!disk
                .windows(secret.len())
                .any(|part| part == secret.as_bytes()));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(fixture.key())
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }
    #[test]
    fn server_logout_does_not_resurrect_from_old_tabs() {
        let fixture = Fixture::new();
        let mut vault = fixture.vault();
        vault.restore("one", 100).unwrap();
        vault.capture("one", vec![cookie("first")], 100).unwrap();
        let old = vault.restore("old", 101).unwrap();
        vault.capture("one", vec![], 102).unwrap();
        vault.capture("old", old, 103).unwrap();
        assert!(vault.restore("third", 104).unwrap().is_empty());
        drop(vault);
        assert!(fixture.vault().restore("restart", 105).unwrap().is_empty());
    }
    #[test]
    fn stale_tabs_cannot_overwrite_new_logins_or_delete_newer_tokens() {
        let fixture = Fixture::new();
        let mut vault = fixture.vault();
        vault.restore("one", 100).unwrap();
        vault.capture("one", vec![cookie("first")], 100).unwrap();
        let old = vault.restore("old", 101).unwrap();
        vault.capture("one", vec![cookie("new")], 102).unwrap();
        vault.capture("old", old, 103).unwrap();
        vault.capture("old", vec![], 104).unwrap();
        assert_eq!(vault.restore("three", 105).unwrap()[0].value, "new");
    }
    #[test]
    fn deadlines_and_server_expiry_are_respected_without_extending_idle_sessions() {
        let fixture = Fixture::new();
        let mut vault = fixture.vault();
        vault.restore("one", 100).unwrap();
        vault.capture("one", vec![cookie("value")], 100).unwrap();
        vault.capture("one", vec![cookie("value")], 200).unwrap();
        assert!(vault
            .restore("expired", 100 + SESSION_LIFETIME)
            .unwrap()
            .is_empty());
        let mut persistent = cookie("expiry");
        persistent.expires = Some(401);
        vault.capture("one", vec![persistent], 400).unwrap();
        assert!(vault.restore("expired-server", 401).unwrap().is_empty());
    }
    #[test]
    fn clear_works_without_a_key_and_allows_new_logins() {
        let fixture = Fixture::new();
        let mut vault = fixture.vault();
        vault.restore("one", 100).unwrap();
        vault.capture("one", vec![cookie("value")], 100).unwrap();
        drop(vault);
        std::fs::remove_file(fixture.key()).unwrap();
        let mut vault = fixture.vault();
        assert!(vault.restore("one", 101).is_err());
        vault.clear().unwrap();
        assert!(vault.restore("one", 102).unwrap().is_empty());
        vault.capture("one", vec![cookie("new")], 103).unwrap();
        assert_eq!(
            fixture.vault().restore("restart", 104).unwrap()[0].value,
            "new"
        );
    }
    #[test]
    fn rejects_unsupported_algorithm_nonce_and_modified_ciphertext() {
        for sql in [
            "UPDATE cookie_vault SET algorithm='unsupported'",
            "UPDATE cookie_vault SET nonce=X'00'",
            "UPDATE cookie_vault SET ciphertext=X'0001'",
        ] {
            let fixture = Fixture::new();
            let mut vault = fixture.vault();
            vault.restore("one", 100).unwrap();
            vault.capture("one", vec![cookie("value")], 100).unwrap();
            vault.database.execute(sql, []).unwrap();
            drop(vault);
            let mut vault = fixture.vault();
            assert!(vault.restore("restart", 101).is_err());
            vault.clear().unwrap();
            assert!(vault.restore("restart", 102).unwrap().is_empty());
        }
    }
    #[test]
    fn rejects_invalid_cookie_input_and_keeps_the_last_good_payload() {
        let fixture = Fixture::new();
        let mut vault = fixture.vault();
        vault.restore("one", 100).unwrap();
        vault.capture("one", vec![cookie("good")], 100).unwrap();
        let mut invalid = cookie("v");
        invalid.domain = "example.org/path".into();
        assert!(vault.capture("one", vec![invalid], 101).is_err());
        assert!(vault
            .capture("one", vec![cookie("bad\nvalue")], 101)
            .is_err());
        assert!(vault
            .capture("one", vec![cookie("v"); MAX_COOKIES + 1], 101)
            .is_err());
        assert_eq!(vault.restore("two", 102).unwrap()[0].value, "good");
    }
    #[test]
    fn clearing_discards_old_in_flight_baselines_and_never_restores_them() {
        let fixture = Fixture::new();
        let mut vault = fixture.vault();
        vault.restore("one", 100).unwrap();
        vault.capture("one", vec![cookie("token")], 100).unwrap();
        let stale = vault.restore("old", 101).unwrap();
        vault.clear().unwrap();
        vault.capture("old", stale, 102).unwrap();
        assert!(vault.restore("fresh", 103).unwrap().is_empty());
        drop(vault);
        assert!(fixture.vault().restore("restart", 104).unwrap().is_empty());
    }
    #[test]
    fn a_deleted_key_is_not_recreated_over_existing_ciphertext() {
        let fixture = Fixture::new();
        let mut vault = fixture.vault();
        vault.restore("one", 100).unwrap();
        vault.capture("one", vec![cookie("token")], 100).unwrap();
        std::fs::remove_file(fixture.key()).unwrap();
        assert!(vault.capture("one", vec![cookie("changed")], 101).is_err());
        assert!(!fixture.key().exists());
    }
    #[test]
    fn unique_nonces_and_a_separate_authenticated_purpose_protect_payloads() {
        let fixture = Fixture::new();
        let cipher = CredentialCipher::load_or_create(&fixture.key()).unwrap();
        let first = cipher.encrypt_scoped("test-payload", PURPOSE).unwrap();
        let second = cipher.encrypt_scoped("test-payload", PURPOSE).unwrap();
        assert_ne!(first.nonce, second.nonce);
        assert!(cipher.decrypt(&first).is_err());
        assert_eq!(
            cipher.decrypt_scoped(&first, PURPOSE).unwrap().as_str(),
            "test-payload"
        );
    }
}
