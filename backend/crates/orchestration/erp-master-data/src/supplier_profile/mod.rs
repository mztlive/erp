//! 供应商资料根级命令。
//!
//! 本 Process 协调 Party、Supplier、当前事实与独立资质，在一个
//! MongoDB 事务中提交全部写入，并把幂等结果与业务数据一并落库。

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

use erp_identity::SharedRbacService;
use erp_party::SensitiveDataCodec;
use erp_supplier::{
    SupplierAccess, SupplierExt, SupplierProfileCommand, SupplierProfileMutationView, command_view,
};
use mongodb::Database;
use persistence_core::NoTransaction;

use crate::Result;

/// 完整供应商资料的跨域写入与事务编排。
pub struct SupplierProfileProcess {
    db: Database,
    sensitive_data: Arc<SensitiveDataCodec>,
    authorization: SupplierProfileAuthorization,
}

/// 同一启动期装配的身份资格与供应商范围服务，不包含具体跨域 adapter。
#[derive(Clone)]
struct SupplierProfileAuthorization {
    rbac: SharedRbacService,
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
        rbac: SharedRbacService,
        access: SupplierAccess,
    ) -> Self {
        Self { db, sensitive_data, authorization: SupplierProfileAuthorization { rbac, access } }
    }

    /// 按幂等键查询已成功的根级命令结果。
    ///
    /// # 参数
    /// * `idempotency_key` - 客户端根资料命令幂等键。
    ///
    /// # 返回
    /// 已成功命令的视图；没有记录时为 `None`。
    ///
    /// # 错误
    /// 查询失败时返回仓储错误。
    pub async fn command_result(&self, idempotency_key: &str) -> Result<Option<SupplierProfileMutationView>> {
        Ok(self.command_record(idempotency_key).await?.map(command_view))
    }

    /// 加载幂等命令实体，供请求一致性与并发恢复校验使用。
    ///
    /// # 参数
    /// * `idempotency_key` - 客户端根资料命令幂等键
    ///
    /// # 返回
    /// 返回已成功命令；不存在时返回 `None`。
    ///
    /// # 错误
    /// 仓储查询或反序列化失败时返回错误。
    async fn command_record(&self, idempotency_key: &str) -> Result<Option<SupplierProfileCommand>> {
        Ok(self.db.supplier().profile_command(idempotency_key, &mut NoTransaction).await?)
    }
}
