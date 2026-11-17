//! 供应商资料根级命令。
//!
//! 本 Process 协调 Party、Supplier、当前事实与独立资质，在一个
//! MongoDB 事务中提交全部写入，并把幂等结果与业务数据一并落库。

mod attachment_upload;
mod commands;
mod create;
mod handover;
mod party_change;
mod sensitive;
#[cfg(test)]
mod tests;
mod update;
mod validation;

pub mod import;

use std::sync::Arc;

use erp_identity::AuthorizationService;
use erp_party::SensitiveDataCodec;
use erp_supplier::{SupplierAccess, SupplierProfileMutationView};
use mongodb::Database;

/// 完整供应商资料的跨域写入与事务编排。
#[derive(Clone)]
pub struct SupplierProfileProcess {
    db: Database,
    sensitive_data: Arc<SensitiveDataCodec>,
    authorization: SupplierProfileAuthorization,
}

/// 同一启动期装配的身份资格与供应商范围服务，不包含具体跨域 adapter。
#[derive(Clone)]
struct SupplierProfileAuthorization {
    rbac: Arc<AuthorizationService>,
    access: SupplierAccess,
}
/// 携带文件资产的供应商根命令执行结果。
pub struct SupplierProfileWithAssetsResult {
    /// 稳定业务结果。
    pub view: SupplierProfileMutationView,
    /// 本次上传对象是否已随业务事务登记；幂等重放时为 `false`。
    pub assets_committed: bool,
}

impl SupplierProfileProcess {
    /// 创建具备完整授权能力的供应商资料服务。
    ///
    /// # 参数
    /// db 为业务数据库，sensitive_data 为敏感数据编解码器；rbac 与 access 为同次装配的身份及供应商授权能力。
    /// # 返回
    /// 在业务事务中执行授权重验的资料服务。
    /// # 错误
    /// 构造不执行 I/O。
    pub fn new(
        db: Database,
        sensitive_data: Arc<SensitiveDataCodec>,
        rbac: Arc<AuthorizationService>,
        access: SupplierAccess,
    ) -> Self {
        Self { db, sensitive_data, authorization: SupplierProfileAuthorization { rbac, access } }
    }
}
