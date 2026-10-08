//! 导入查询 adapter：把消费方端口接到支持域批量任务。

use std::sync::Arc;

use async_trait::async_trait;
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
    ///
    /// # 参数
    /// * `db` - 支持域批量任务所在数据库。
    ///
    /// # 返回
    /// 返回未执行 I/O 的 adapter。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 包装为导入域可注入的批量任务事实 Port。
    ///
    /// # 参数
    /// * `db` - 支持域批量任务所在数据库。
    ///
    /// # 返回
    /// 返回共享的批量任务事实 Port。
    ///
    /// # 错误
    /// 不返回错误。
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
///
/// # 参数
/// * `db` - 导入与批量任务所在数据库。
///
/// # 返回
/// 返回已注入批量任务事实 Port 的导入查询服务。
///
/// # 错误
/// 不返回错误。
pub fn legacy_import_service(db: Database) -> LegacyImportService {
    LegacyImportService::new(db.clone(), MongoImportBulkJobs::shared(db))
}

/// 装配拥有跨域事务的导入应用流程。
///
/// # 参数
/// * `db` - 导入应用流程使用的数据库。
///
/// # 返回
/// 返回导入应用流程。
///
/// # 错误
/// 不返回错误。
pub fn import_apply_service(db: Database) -> crate::import_apply::ImportApplyService {
    crate::import_apply::ImportApplyService::new(db)
}
