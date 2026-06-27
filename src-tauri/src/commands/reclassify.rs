use crate::ai::classify::{classify_message_ids, CLASSIFY_PROMPT_VERSION};
use crate::db::{app_meta, messages, Pool};

const RECLASSIFY_CAP: i64 = 1000;
const MAX_ATTEMPTS: i64 = 3;
const KEY_VERSION: &str = "classify_prompt_version";
const KEY_ATTEMPTS: &str = "reclassify_attempts";
const KEY_ATTEMPTS_VERSION: &str = "reclassify_attempts_version";

/// 「针对当前目标版本」的有效失败次数：存储的失败次数若属于更旧的 prompt 版本则视为 0，
/// 使每次 bump 版本都获得全新的重试预算（同版本内仍正常累加退避）。
fn effective_attempts(stored_attempts: i64, attempts_version: i64, current: i64) -> i64 {
    if attempts_version == current {
        stored_attempts
    } else {
        0
    }
}

/// 启动后调用一次：若存储的 prompt 版本号落后于当前版本，对存量已分类邮件重跑分类。
///
/// 成功 → 写版本号；失败 → 累加 attempts，最多重试 `MAX_ATTEMPTS` 次后不再尝试。
/// attempts 按 prompt 版本计：bump 版本即重置失败预算（见 `effective_attempts`），
/// 避免上一版本耗尽的失败次数永久阻断新版本重跑。
/// 设计为幂等：重复调用只在版本号未推进时执行。
pub async fn run_once_if_needed(pool: &Pool) {
    let stored: i64 = app_meta::get(pool, KEY_VERSION)
        .await
        .ok()
        .flatten()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    if stored >= CLASSIFY_PROMPT_VERSION {
        return;
    }

    let stored_attempts: i64 = app_meta::get(pool, KEY_ATTEMPTS)
        .await
        .ok()
        .flatten()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let attempts_version: i64 = app_meta::get(pool, KEY_ATTEMPTS_VERSION)
        .await
        .ok()
        .flatten()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let attempts = effective_attempts(stored_attempts, attempts_version, CLASSIFY_PROMPT_VERSION);
    if attempts >= MAX_ATTEMPTS {
        return;
    }

    let ids = match messages::reclassify_candidates(pool, RECLASSIFY_CAP).await {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "reclassify 取候选失败");
            return;
        }
    };

    if ids.is_empty() {
        if let Err(e) = app_meta::set(pool, KEY_VERSION, &CLASSIFY_PROMPT_VERSION.to_string()).await
        {
            tracing::warn!(error = %e, "reclassify: 写 app_meta 版本号失败（无候选）");
        }
        return;
    }

    match classify_message_ids(pool, &ids).await {
        Ok(_) => {
            if let Err(e) =
                app_meta::set(pool, KEY_VERSION, &CLASSIFY_PROMPT_VERSION.to_string()).await
            {
                tracing::warn!(error = %e, "reclassify: 写 app_meta 版本号失败");
            }
        }
        Err(e) => {
            tracing::warn!(error = %e, "reclassify 失败，记尝试次数");
            if let Err(e) = app_meta::set(pool, KEY_ATTEMPTS, &(attempts + 1).to_string()).await {
                tracing::warn!(error = %e, "reclassify: 写 app_meta 尝试次数失败");
            }
            if let Err(e) = app_meta::set(
                pool,
                KEY_ATTEMPTS_VERSION,
                &CLASSIFY_PROMPT_VERSION.to_string(),
            )
            .await
            {
                tracing::warn!(error = %e, "reclassify: 写 app_meta 尝试版本失败");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_attempts_same_version_keeps_count() {
        assert_eq!(effective_attempts(2, 3, 3), 2);
    }

    #[test]
    fn effective_attempts_old_version_resets_to_zero() {
        // 失败次数属于版本 2、目标已是版本 3 → 重置预算
        assert_eq!(effective_attempts(3, 2, 3), 0);
    }

    #[test]
    fn effective_attempts_unset_version_is_zero() {
        // attempts_version 缺省 0、不等于真实目标版本 → 0
        assert_eq!(effective_attempts(5, 0, 3), 0);
    }

    /// 无候选邮件时，新版本应直接把版本号推进到当前值（不触达 AI）。
    #[tokio::test]
    async fn new_version_empty_candidates_advances_version() {
        let pool = crate::db::test_pool().await;
        run_once_if_needed(&pool).await;
        let v = app_meta::get(&pool, KEY_VERSION)
            .await
            .unwrap()
            .and_then(|s| s.parse::<i64>().ok());
        assert_eq!(v, Some(CLASSIFY_PROMPT_VERSION));
    }

    /// 旧版本耗尽的失败次数不应阻断新版本重跑（版本化退避：bump 即重置预算）。
    #[tokio::test]
    async fn exhausted_attempts_from_old_version_does_not_block() {
        let pool = crate::db::test_pool().await;
        app_meta::set(&pool, KEY_ATTEMPTS, &MAX_ATTEMPTS.to_string())
            .await
            .unwrap();
        app_meta::set(
            &pool,
            KEY_ATTEMPTS_VERSION,
            &(CLASSIFY_PROMPT_VERSION - 1).to_string(),
        )
        .await
        .unwrap();
        run_once_if_needed(&pool).await;
        let v = app_meta::get(&pool, KEY_VERSION)
            .await
            .unwrap()
            .and_then(|s| s.parse::<i64>().ok());
        assert_eq!(
            v,
            Some(CLASSIFY_PROMPT_VERSION),
            "旧版本耗尽的失败次数不应阻断新版本重跑"
        );
    }

    /// 当前版本失败次数达上限 → 早退，版本号不推进（证明同版本退避仍生效）。
    #[tokio::test]
    async fn exhausted_attempts_current_version_blocks() {
        let pool = crate::db::test_pool().await;
        app_meta::set(&pool, KEY_ATTEMPTS, &MAX_ATTEMPTS.to_string())
            .await
            .unwrap();
        app_meta::set(
            &pool,
            KEY_ATTEMPTS_VERSION,
            &CLASSIFY_PROMPT_VERSION.to_string(),
        )
        .await
        .unwrap();
        run_once_if_needed(&pool).await;
        let v = app_meta::get(&pool, KEY_VERSION).await.unwrap();
        assert!(v.is_none(), "当前版本失败次数达上限应早退、不推进版本号");
    }
}
