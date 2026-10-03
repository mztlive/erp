//! 供应商导入：加密源内容、原子登记任务、统一 worker 逐行执行。
mod execute;
mod failures;
mod submit;

use std::sync::Arc;

use erp_identity::SharedRbacService;
use erp_party::SensitiveDataCodec;
use mongodb::Database;
use storage::S3Storage;

/// 供应商资料后台导入流程。
pub struct SupplierImportProcess {
    db: Database,
    rbac: SharedRbacService,
    storage: S3Storage,
    codec: Arc<SensitiveDataCodec>,
}

/// 稳定的后台任务业务类型。
pub const SUPPLIER_IMPORT_DOMAIN: &str = "SUPPLIER_IMPORT";

impl SupplierImportProcess {
    /// 绑定数据库、实时授权源、对象存储和敏感数据编解码器。
    ///
    /// # 参数
    /// * `db` - 业务数据库
    /// * `rbac` - 根资料写入重验权限和组织归属的共享授权源
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
        rbac: SharedRbacService,
        storage: S3Storage,
        codec: Arc<SensitiveDataCodec>,
    ) -> Self {
        Self { db, rbac, storage, codec }
    }
}
