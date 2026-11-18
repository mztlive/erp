//! 导入查询 adapter：把消费方端口接到支持域批量任务。

use std::sync::Arc;

use async_trait::async_trait;
use erp_identity::AuthorizationService;
use erp_import::{BulkJobFactsPort, LegacyImportService};
use erp_support::BulkJobExt;
use erp_support::repository::prelude::*;
use mongodb::Database;
use persistence_core::Executor;

/// 为导入批次视图读取后台任务身份的 Mongo adapter。
#[derive(Clone)]
pub struct MongoImportBulkJobs {
    db: Database,
}

impl MongoImportBulkJobs {
    /// 绑定后台任务集合所在数据库，构造时不读取。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 包装为导入域可注入的批量任务事实 Port。
    /// 返回共享的批量任务事实 Port。
    pub fn shared(db: Database) -> Arc<dyn BulkJobFactsPort> {
        Arc::new(Self::new(db))
    }
}

#[async_trait]
impl BulkJobFactsPort for MongoImportBulkJobs {
    async fn background_job_id_by_request_id(
        &self,
        request_id: &str,
        executor: &mut dyn Executor,
    ) -> erp_import::Result<Option<String>> {
        Ok(self
            .db
            .background_jobs()
            .find_by_request_id(request_id, executor)
            .await
            .map_err(erp_import::Error::from)?
            .map(|job| job.base.id))
    }
}

/// 装配带批量任务身份 adapter 的导入查询服务。
/// 返回已注入批量任务事实 Port 的导入查询服务。
pub fn legacy_import_service(db: Database, authorization: Arc<AuthorizationService>) -> LegacyImportService {
    LegacyImportService::new(db.clone(), MongoImportBulkJobs::shared(db), authorization)
}
