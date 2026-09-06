//! 域 D22 `legacy_import` 查询编排。
//!
//! 批次/行状态查询只读本域仓储；后台任务身份经消费方 Port 读取。
//! 跨域写入（应用、执行、确认完成）由 `erp-processes::import_apply` 持有。

use std::sync::Arc;

use mongodb::Database;

use crate::ports::BulkJobFactsPort;

mod query;

/// 旧数据导入查询服务。
///
/// 提供导入批次与导入行的只读查询；跨域命令不在本服务内开启事务。
pub struct LegacyImportService {
    db: Database,
    bulk_jobs: Arc<dyn BulkJobFactsPort>,
}

impl LegacyImportService {
    /// 创建旧数据导入查询服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    /// * `bulk_jobs` - 后台任务身份事实端口
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(db: Database, bulk_jobs: Arc<dyn BulkJobFactsPort>) -> Self {
        Self { db, bulk_jobs }
    }
}
