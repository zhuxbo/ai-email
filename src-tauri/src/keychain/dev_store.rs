//! Debug-only 文件态凭据存储：dev 构建旁路 OS 钥匙串，避免每次重建后重复系统授权
//! （ad-hoc 签名 cdhash 每次变 → 「始终允许」存不住）。release 不编译本模块。
//! 文件为明文 JSON（debug 专用、在 app 数据目录、不入库），与「API key dev 用 .env 明文」同档。

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, PoisonError};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

/// 读-改-写串行化（dev 低频，足够）。
static FILE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Default, Serialize, Deserialize)]
struct DevCreds {
    #[serde(default)]
    mail: HashMap<String, String>,
    #[serde(default)]
    ai: HashMap<String, String>,
}

#[derive(Clone, Copy)]
pub enum Namespace {
    Mail,
    Ai,
}

fn load(path: &Path) -> AppResult<DevCreds> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|_| AppError::Keychain("dev cred parse failed".into())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(DevCreds::default()),
        Err(_) => Err(AppError::Keychain("dev cred read failed".into())),
    }
}

fn save(path: &Path, creds: &DevCreds) -> AppResult<()> {
    let bytes = serde_json::to_vec_pretty(creds)
        .map_err(|_| AppError::Keychain("dev cred serialize failed".into()))?;
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(path, bytes).map_err(|_| AppError::Keychain("dev cred write failed".into()))
}

fn bucket(creds: &mut DevCreds, ns: Namespace) -> &mut HashMap<String, String> {
    match ns {
        Namespace::Mail => &mut creds.mail,
        Namespace::Ai => &mut creds.ai,
    }
}

pub fn read(path: &Path, ns: Namespace, key: &str) -> AppResult<Option<String>> {
    let _g = FILE_LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    let mut creds = load(path)?;
    Ok(bucket(&mut creds, ns).get(key).cloned())
}

pub fn write(path: &Path, ns: Namespace, key: &str, secret: &str) -> AppResult<()> {
    let _g = FILE_LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    let mut creds = load(path)?;
    bucket(&mut creds, ns).insert(key.to_string(), secret.to_string());
    save(path, &creds)
}

pub fn delete(path: &Path, ns: Namespace, key: &str) -> AppResult<()> {
    let _g = FILE_LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    let mut creds = load(path)?;
    bucket(&mut creds, ns).remove(key);
    save(path, &creds)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn write_then_read_roundtrip() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("c.json");
        write(&p, Namespace::Mail, "k1", "secret1").unwrap();
        assert_eq!(
            read(&p, Namespace::Mail, "k1").unwrap().as_deref(),
            Some("secret1")
        );
    }

    #[test]
    fn read_missing_returns_none() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("c.json");
        assert_eq!(read(&p, Namespace::Mail, "nope").unwrap(), None);
    }

    #[test]
    fn delete_is_idempotent() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("c.json");
        delete(&p, Namespace::Ai, "ghost").unwrap(); // 删不存在键 Ok
        write(&p, Namespace::Ai, "k", "v").unwrap();
        delete(&p, Namespace::Ai, "k").unwrap();
        assert_eq!(read(&p, Namespace::Ai, "k").unwrap(), None);
    }

    #[test]
    fn mail_and_ai_same_key_no_collision() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("c.json");
        write(&p, Namespace::Mail, "same", "mail-val").unwrap();
        write(&p, Namespace::Ai, "same", "ai-val").unwrap();
        // 变异验证：bucket() 把 Mail/Ai 写反 → 此两断言 FAIL
        assert_eq!(
            read(&p, Namespace::Mail, "same").unwrap().as_deref(),
            Some("mail-val")
        );
        assert_eq!(
            read(&p, Namespace::Ai, "same").unwrap().as_deref(),
            Some("ai-val")
        );
    }

    #[test]
    fn concurrent_writes_all_persist() {
        use std::sync::Arc;
        use std::thread;
        let dir = tempdir().unwrap();
        let p = Arc::new(dir.path().join("c.json"));
        let handles: Vec<_> = (0..8)
            .map(|i| {
                let p = Arc::clone(&p);
                thread::spawn(move || {
                    write(&p, Namespace::Mail, &format!("k{i}"), &format!("v{i}")).unwrap();
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
        // 变异验证：去掉 FILE_LOCK → RMW 互相覆盖丢键 → 此断言 FAIL
        for i in 0..8 {
            assert_eq!(
                read(&p, Namespace::Mail, &format!("k{i}"))
                    .unwrap()
                    .as_deref(),
                Some(format!("v{i}").as_str())
            );
        }
    }
}
