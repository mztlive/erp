//! 供应商导入：加密源内容、原子登记任务、统一 worker 逐行执行。
mod execute;
mod failures;
mod submit;

use erp_party::SensitiveDataCodec;
use mongodb::Database;
use std::sync::Arc;
use storage::S3Storage;

/// 供应商资料后台导入流程。
pub struct SupplierImportProcess {
    db: Database,
    storage: S3Storage,
    codec: Arc<SensitiveDataCodec>,
}

/// 稳定的后台任务业务类型。
pub const SUPPLIER_IMPORT_DOMAIN: &str = "SUPPLIER_IMPORT";

impl SupplierImportProcess {
    /// 绑定数据库、对象存储和敏感数据编解码器。
    ///
    /// 返回供 HTTP 和统一 worker 共用的流程；不执行 I/O。
    pub fn new(db: Database, storage: S3Storage, codec: Arc<SensitiveDataCodec>) -> Self {
        Self { db, storage, codec }
    }
}
