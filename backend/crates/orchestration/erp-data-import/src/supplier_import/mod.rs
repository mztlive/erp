//! 供应商导入：加密源内容、原子登记任务、统一 worker 逐行执行。
mod execute;
mod failures;
mod submit;

use std::sync::Arc;

use erp_master_data::SupplierProfileProcess;
use erp_party::SensitiveDataCodec;
use erp_supplier::dto::import_job::SupplierImportJobRequest;
use mongodb::Database;
use storage::S3Storage;

use crate::background::import_source::ImportSourceCache;

/// 供应商资料后台导入流程。
pub struct SupplierImportProcess {
    db: Database,
    profile: Arc<SupplierProfileProcess>,
    storage: S3Storage,
    codec: Arc<SensitiveDataCodec>,
    sources: ImportSourceCache<SupplierImportJobRequest>,
}

/// 稳定的后台任务业务类型。
pub const SUPPLIER_IMPORT_DOMAIN: &str = "SUPPLIER_IMPORT";

impl SupplierImportProcess {
    /// 绑定数据库、实时授权源、对象存储和敏感数据编解码器。
    ///
    /// # 参数
    /// * `db` - 业务数据库
    /// * `profile` - 已注入实时授权和供应商数据范围的根资料流程
    /// * `storage` - 加密导入源的对象存储
    /// * `codec` - 敏感数据编解码器
    ///
    /// # 返回
    /// 返回供 HTTP 和统一 worker 共用的流程；不执行 I/O。
    ///
    /// # 错误
    /// 无。
    pub fn new(
        db: Database,
        profile: Arc<SupplierProfileProcess>,
        storage: S3Storage,
        codec: Arc<SensitiveDataCodec>,
    ) -> Self {
        // 每份供应商输入至多 10 MiB，保留一轮四个任务的完整输入。
        Self { db, profile, storage, codec, sources: ImportSourceCache::new(40 * 1024 * 1024) }
    }
}
