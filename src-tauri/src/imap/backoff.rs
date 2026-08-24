//! 同步失败冷却退避。
//!
//! 背景：QQ IMAP 登录被节流时每次同步都失败，若自动同步仍按原间隔（5 分钟）无退避
//! 地重试，只会持续撞限流、恶性循环。本模块记录每账户连续同步失败次数：
//!
//! - 0/1 次失败不冷却（单次失败常是偶发抖动）。
//! - 连续 ≥2 次进入冷却：2 次 → 15 分钟，≥3 次 → 30 分钟（封顶）。冷却期内**自动**
//!   同步直接跳过（不再发起 IMAP 登录），手动同步（force）不受限。
//! - 任一次同步成功即清零。
//!
//! 状态在内存（重启即清零）——退避是针对「本会话内服务端限流」的短期防抖，不落库。

use std::collections::HashMap;
use std::sync::Mutex as StdMutex;
use std::time::{Duration, Instant};

use uuid::Uuid;

/// 冷却时长基数：连续 2 次失败后的冷却时长。
const BASE_COOLDOWN: Duration = Duration::from_secs(15 * 60);
/// 冷却上限：连续 ≥3 次失败后的冷却时长。
const MAX_COOLDOWN: Duration = Duration::from_secs(30 * 60);
/// 触发冷却所需的最小连续失败次数。
const MIN_FAILURES_TO_COOLDOWN: u32 = 2;

#[derive(Debug, Clone)]
struct FailureEntry {
    consecutive_failures: u32,
    last_failure_at: Instant,
}

/// 纯函数：连续失败次数 → 冷却时长。测试锁定阶梯值。
pub fn cooldown_after(consecutive_failures: u32) -> Duration {
    if consecutive_failures < MIN_FAILURES_TO_COOLDOWN {
        return Duration::ZERO;
    }
    if consecutive_failures > MIN_FAILURES_TO_COOLDOWN {
        MAX_COOLDOWN
    } else {
        BASE_COOLDOWN
    }
}

#[derive(Default)]
pub struct SyncBackoff {
    entries: StdMutex<HashMap<Uuid, FailureEntry>>,
}

impl SyncBackoff {
    pub fn new() -> Self {
        Self::default()
    }

    /// 自动同步前的守卫：若账户处于冷却期，返回剩余冷却时长（调用方跳过本次同步）。
    pub fn remaining(&self, account_id: Uuid, now: Instant) -> Duration {
        let entries = self.entries.lock().expect("sync backoff lock poisoned");
        let Some(entry) = entries.get(&account_id) else {
            return Duration::ZERO;
        };
        let total = cooldown_after(entry.consecutive_failures);
        let elapsed = now.saturating_duration_since(entry.last_failure_at);
        total.saturating_sub(elapsed)
    }

    pub fn record_success(&self, account_id: Uuid) {
        let mut entries = self.entries.lock().expect("sync backoff lock poisoned");
        if entries.remove(&account_id).is_some() {
            tracing::info!(account_id = %account_id, "同步成功，退出失败冷却");
        }
    }

    pub fn record_failure(&self, account_id: Uuid, now: Instant) {
        let mut entries = self.entries.lock().expect("sync backoff lock poisoned");
        let entry = entries.entry(account_id).or_insert(FailureEntry {
            consecutive_failures: 0,
            last_failure_at: now,
        });
        entry.consecutive_failures += 1;
        entry.last_failure_at = now;
        tracing::warn!(
            account_id = %account_id,
            consecutive_failures = entry.consecutive_failures,
            cooldown_secs = cooldown_after(entry.consecutive_failures).as_secs(),
            "同步失败，计入冷却退避"
        );
    }

    /// 测试辅助：读取某账户连续失败次数。
    #[cfg(test)]
    pub(crate) fn failures_of(&self, account_id: Uuid) -> u32 {
        self.entries
            .lock()
            .expect("sync backoff lock poisoned")
            .get(&account_id)
            .map(|e| e.consecutive_failures)
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cooldown_ladder() {
        assert_eq!(cooldown_after(0), Duration::ZERO);
        assert_eq!(
            cooldown_after(1),
            Duration::ZERO,
            "单次失败是偶发抖动，不冷却"
        );
        assert_eq!(cooldown_after(2), BASE_COOLDOWN);
        assert_eq!(cooldown_after(3), MAX_COOLDOWN);
        assert_eq!(cooldown_after(10), MAX_COOLDOWN, "冷却封顶 30 分钟");
    }

    #[test]
    fn remaining_decays_and_expires() {
        let backoff = SyncBackoff::new();
        let id = Uuid::new_v4();
        let t0 = Instant::now();

        backoff.record_failure(id, t0);
        backoff.record_failure(id, t0);
        assert_eq!(
            backoff.remaining(id, t0),
            BASE_COOLDOWN,
            "刚失败完应剩满额冷却"
        );

        let half = t0 + BASE_COOLDOWN / 2;
        assert_eq!(backoff.remaining(id, half), BASE_COOLDOWN / 2);

        let after = t0 + BASE_COOLDOWN + Duration::from_secs(1);
        assert_eq!(
            backoff.remaining(id, after),
            Duration::ZERO,
            "冷却期过后不再拦截"
        );
    }

    #[test]
    fn success_resets_counter() {
        let backoff = SyncBackoff::new();
        let id = Uuid::new_v4();
        let t0 = Instant::now();
        backoff.record_failure(id, t0);
        backoff.record_failure(id, t0);
        assert_eq!(backoff.failures_of(id), 2);
        assert_eq!(backoff.remaining(id, t0), BASE_COOLDOWN);

        backoff.record_success(id);
        assert_eq!(backoff.failures_of(id), 0);
        assert_eq!(
            backoff.remaining(id, t0),
            Duration::ZERO,
            "成功后立即退出冷却"
        );
    }

    #[test]
    fn single_failure_does_not_cool_down() {
        let backoff = SyncBackoff::new();
        let id = Uuid::new_v4();
        let t0 = Instant::now();
        backoff.record_failure(id, t0);
        assert_eq!(
            backoff.remaining(id, t0),
            Duration::ZERO,
            "1 次失败不应拦截自动同步"
        );
    }
}
