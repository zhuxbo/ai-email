pub mod api;
pub mod config;
pub mod db;
pub mod error;
pub mod mail;
pub mod mcp;
pub mod model;
pub mod operations;
pub mod sync;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{Mutex, OwnedMutexGuard};
pub struct Service {
    pub config: config::Config,
    pub db: db::Database,
    pub transport: Arc<dyn mail::MailTransport>,
    account_locks: HashMap<String, Arc<Mutex<()>>>,
}
impl Service {
    pub async fn new(
        config: config::Config,
        transport: Arc<dyn mail::MailTransport>,
    ) -> anyhow::Result<Arc<Self>> {
        config.validate()?;
        let db = db::Database::open(&config.database_path).await?;
        db.retain_configured(&config.accounts).await?;
        db.cleanup(model::now()).await?;
        let account_locks = config
            .accounts
            .iter()
            .map(|a| (a.id.clone(), Arc::new(Mutex::new(()))))
            .collect();
        Ok(Arc::new(Self {
            config,
            db,
            transport,
            account_locks,
        }))
    }
    pub async fn account_lock(&self, id: &str) -> OwnedMutexGuard<()> {
        self.account_locks
            .get(id)
            .cloned()
            .unwrap_or_else(|| Arc::new(Mutex::new(())))
            .lock_owned()
            .await
    }
    pub async fn accounts(&self, can_write: bool) -> error::Result<serde_json::Value> {
        let existing = self.db.statuses().await?;
        let sync: Vec<_> = self
            .config
            .accounts
            .iter()
            .map(|a| {
                existing
                    .iter()
                    .find(|s| s.account_id == a.id)
                    .cloned()
                    .unwrap_or(model::SyncStatus {
                        account_id: a.id.clone(),
                        status: "pending".into(),
                        last_synced_at: None,
                        error: None,
                    })
            })
            .collect();
        Ok(
            serde_json::json!({"can_write":can_write,"accounts":self.config.accounts.iter().map(|a| serde_json::json!({"id":a.id,"email":a.email,"display_name":a.display_name,"folders":a.folders})).collect::<Vec<_>>(),"sync":sync}),
        )
    }
}
