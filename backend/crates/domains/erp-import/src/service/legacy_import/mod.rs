//! 旧导入批次与行管理查询在同一快照校验功能资格、读取本域及后台任务事实。

use std::sync::Arc;

use erp_identity::AuthorizationService;
use mongodb::Database;

use crate::ports::BulkJobFactsPort;

mod query;

/// 旧数据导入查询服务。
///
/// 提供导入批次与导入行的只读查询；查询入口持有读取快照，后台任务只经事实端口读取。
#[derive(Clone)]
pub struct LegacyImportService {
    db: Database,
    bulk_jobs: Arc<dyn BulkJobFactsPort>,
    authorization: Arc<AuthorizationService>,
}

impl LegacyImportService {
    /// 装配历史导入查询所需数据库、后台任务事实端口和应用共享授权服务，供在原快照内验权及补齐关联。
    pub fn new(
        db: Database,
        bulk_jobs: Arc<dyn BulkJobFactsPort>,
        authorization: Arc<AuthorizationService>,
    ) -> Self {
        Self { db, bulk_jobs, authorization }
    }
}
