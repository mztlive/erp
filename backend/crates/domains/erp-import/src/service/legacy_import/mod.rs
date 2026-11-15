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
    /// 创建旧数据导入查询服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    /// * `bulk_jobs` - 后台任务身份事实端口
    /// * `authorization` - 应用唯一共享授权服务
    ///
    /// # 返回
    /// 返回服务实例。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(
        db: Database,
        bulk_jobs: Arc<dyn BulkJobFactsPort>,
        authorization: Arc<AuthorizationService>,
    ) -> Self {
        Self { db, bulk_jobs, authorization }
    }
}
