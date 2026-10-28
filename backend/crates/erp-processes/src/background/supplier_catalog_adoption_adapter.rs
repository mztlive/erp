//! 从持久化确认集合领取选品任务，页面关闭不影响执行。

use std::collections::HashSet;
use std::sync::Arc;

use erp_supply::catalog_adoption::runnable_batches;
use mongodb::Database;
use persistence_core::NoTransaction;
use tokio::sync::{Mutex, Semaphore};

use super::BackgroundTaskAdapter;
use crate::Result;
use crate::supplier_catalog_adoption::SupplierCatalogAdoptionProcess;

/// 复用统一后台执行器，最多同时运行两个入库批次。
pub struct SupplierCatalogAdoptionTaskAdapter {
    db: Database,
    process: Arc<SupplierCatalogAdoptionProcess>,
    cursor: Mutex<String>,
    permits: Arc<Semaphore>,
    active: Arc<Mutex<HashSet<String>>>,
}

impl SupplierCatalogAdoptionTaskAdapter {
    /// 注入已装配权限、来源注册表与素材客户端的采纳流程。
    /// # 参数
    /// `db` 为当前数据库，`process` 为应用装配完成的流程。
    /// # 返回
    /// 可登记到统一后台执行器的持久化任务适配器。
    /// # 错误
    /// 不返回错误。
    pub fn new(db: Database, process: SupplierCatalogAdoptionProcess) -> Self {
        Self {
            db,
            process: Arc::new(process),
            cursor: Mutex::new(String::new()),
            permits: Arc::new(Semaphore::new(2)),
            active: Arc::new(Mutex::new(HashSet::new())),
        }
    }
}

#[async_trait::async_trait]
impl BackgroundTaskAdapter for SupplierCatalogAdoptionTaskAdapter {
    fn name(&self) -> &'static str {
        "supplier_catalog_adoption"
    }

    async fn run_due(&self) -> Result<usize> {
        let mut cursor = self.cursor.lock().await;
        let batches = runnable_batches(&self.db, &cursor, &mut NoTransaction).await?;
        if batches.is_empty() {
            cursor.clear();
            return Ok(0);
        }
        let mut claimed = 0;
        for batch in batches {
            *cursor = batch.base.id.clone();
            if self.active.lock().await.contains(&batch.base.id) {
                continue;
            }
            let Ok(permit) = Arc::clone(&self.permits).try_acquire_owned() else { break };
            self.active.lock().await.insert(batch.base.id.clone());
            let active = Arc::clone(&self.active);
            let process = Arc::clone(&self.process);
            tokio::spawn(async move {
                let _permit = permit;
                if let Err(error) = process.process_batch(&batch.base.id).await {
                    tracing::warn!(batch_id=%batch.base.id,account=%batch.actor_id,error=%error,"supplier catalog adoption interrupted");
                }
                active.lock().await.remove(&batch.base.id);
            });
            claimed += 1;
        }
        Ok(claimed)
    }
}
