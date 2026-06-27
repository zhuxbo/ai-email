use crate::db::Pool;
use crate::error::AppResult;

pub async fn get(pool: &Pool, key: &str) -> AppResult<Option<String>> {
    let r: Option<(String,)> = sqlx::query_as("SELECT value FROM app_meta WHERE key=?1")
        .bind(key)
        .fetch_optional(pool)
        .await?;
    Ok(r.map(|x| x.0))
}

pub async fn set(pool: &Pool, key: &str, value: &str) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO app_meta(key,value) VALUES(?1,?2) \
         ON CONFLICT(key) DO UPDATE SET value=excluded.value",
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn app_meta_roundtrip() {
        let pool = crate::db::test_pool().await;
        assert_eq!(get(&pool, "k").await.unwrap(), None);
        set(&pool, "k", "1").await.unwrap();
        assert_eq!(get(&pool, "k").await.unwrap().as_deref(), Some("1"));
        set(&pool, "k", "2").await.unwrap();
        assert_eq!(get(&pool, "k").await.unwrap().as_deref(), Some("2"));
    }
}
