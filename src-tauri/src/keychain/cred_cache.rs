//! 进程内凭据缓存：每条凭据每次启动只触碰一次 OS 钥匙串。
//!
//! 动机：钥匙串 ACL 按 app 身份授权。未点「始终允许」前（或授权身份变化后），
//! **每一次** keychain 读取都可能触发系统弹窗；而 app 每个 IMAP/AI 操作都读凭据，
//! 自动同步下每小时几十次。命中缓存后：每条凭据每进程一次读取，弹窗压力骤减。
//!
//! 取舍：凭据以 `SecretString` 常驻进程内存（与 Thunderbird 等客户端一致）。
//! 写路径（store）与删路径（delete）同步更新缓存，保证缓存不落后于钥匙串。
//! debug 构建走 dev_store（明文文件），不经过本缓存。

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::{Arc, Mutex};

use secrecy::SecretString;

pub(crate) struct CredCache<K: Eq + Hash + Clone> {
    map: Mutex<HashMap<K, Arc<SecretString>>>,
}

impl<K: Eq + Hash + Clone> CredCache<K> {
    pub(crate) fn new() -> Self {
        Self {
            map: Mutex::new(HashMap::new()),
        }
    }

    pub(crate) fn get(&self, key: &K) -> Option<Arc<SecretString>> {
        self.map
            .lock()
            .expect("cred cache lock poisoned")
            .get(key)
            .cloned()
    }

    pub(crate) fn put(&self, key: K, value: SecretString) {
        self.map
            .lock()
            .expect("cred cache lock poisoned")
            .insert(key, Arc::new(value));
    }

    pub(crate) fn remove(&self, key: &K) {
        self.map
            .lock()
            .expect("cred cache lock poisoned")
            .remove(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use secrecy::ExposeSecret;

    #[test]
    fn get_hit_miss_and_overwrite() {
        let cache: CredCache<u32> = CredCache::new();
        assert!(cache.get(&1).is_none(), "空缓存应 miss");
        cache.put(1, SecretString::from("a".to_string()));
        assert_eq!(cache.get(&1).unwrap().expose_secret(), "a");
        cache.put(1, SecretString::from("b".to_string()));
        assert_eq!(
            cache.get(&1).unwrap().expose_secret(),
            "b",
            "覆盖后应取到新值"
        );
    }

    #[test]
    fn remove_clears_entry_only() {
        let cache: CredCache<u32> = CredCache::new();
        cache.put(1, SecretString::from("a".to_string()));
        cache.put(2, SecretString::from("b".to_string()));
        cache.remove(&1);
        assert!(cache.get(&1).is_none(), "删除后应 miss");
        assert!(cache.get(&2).is_some(), "其它条目不受影响");
    }
}
