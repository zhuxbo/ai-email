use crate::ai::classify::{classify_message_ids, CLASSIFY_PROMPT_VERSION};
use crate::db::{app_meta, messages, Pool};

const RECLASSIFY_CAP: i64 = 1000;
const MAX_ATTEMPTS: i64 = 3;
const KEY_VERSION: &str = "classify_prompt_version";
const KEY_ATTEMPTS: &str = "reclassify_attempts";

/// 启动后调用一次：若存储的 prompt 版本号落后于当前版本，对存量已分类邮件重跑分类。
///
/// 成功 → 写版本号；失败 → 累加 attempts，最多重试 `MAX_ATTEMPTS` 次后不再尝试。
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

    let attempts: i64 = app_meta::get(pool, KEY_ATTEMPTS)
        .await
        .ok()
        .flatten()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
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
        let _ = app_meta::set(pool, KEY_VERSION, &CLASSIFY_PROMPT_VERSION.to_string()).await;
        return;
    }

    match classify_message_ids(pool, &ids).await {
        Ok(_) => {
            let _ = app_meta::set(pool, KEY_VERSION, &CLASSIFY_PROMPT_VERSION.to_string()).await;
        }
        Err(e) => {
            tracing::warn!(error = %e, "reclassify 失败，记尝试次数");
            let _ = app_meta::set(pool, KEY_ATTEMPTS, &(attempts + 1).to_string()).await;
        }
    }
}
