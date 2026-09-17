//! 供应商资料根级命令。
//!
//! 页面只调用本服务维护 Party、Supplier、当前事实与独立资质；服务在一个
//! MongoDB 事务中提交全部写入，并把幂等结果与业务数据一并落库。

mod create;
mod handover;
mod sensitive;
#[cfg(test)]
mod tests;
mod update;
mod validation;

pub mod import;

use std::sync::Arc;

use erp_identity::SharedRbacService;
use erp_party::SensitiveDataCodec;
use erp_supplier::{SupplierExt, SupplierProfileCommand, SupplierProfileMutationView, command_view};
use mongodb::Database;
use persistence_core::NoTransaction;

use crate::{Error, Result};

mod party_change;

/// 完整供应商资料的根级写服务。
pub struct SupplierProfileService {
    db: Database,
    sensitive_data: Arc<SensitiveDataCodec>,
    rbac: Option<SharedRbacService>,
}

/// 携带文件资产的供应商根命令执行结果。
pub struct SupplierProfileWithAssetsResult {
    /// 稳定业务结果。
    pub view: SupplierProfileMutationView,
    /// 本次上传对象是否已随业务事务登记；幂等重放时为 `false`。
    pub assets_committed: bool,
}

impl SupplierProfileService {
    /// 创建根级供应商资料服务。
    pub fn new(db: Database, sensitive_data: Arc<SensitiveDataCodec>) -> Self {
        Self { db, sensitive_data, rbac: None }
    }

    /// 注入当前 RBAC 快照，供资料写入在事务内重验 DataScope。
    ///
    /// # 参数
    /// * `rbac` - 共享 RBAC 服务
    ///
    /// # 返回
    /// 返回可在同一写入事务证明供应商范围的资料服务。
    ///
    /// # 错误
    /// 无。
    pub fn with_rbac(mut self, rbac: SharedRbacService) -> Self {
        self.rbac = Some(rbac);
        self
    }

    /// 取得资料写入所需的授权源。
    pub(super) fn require_rbac(&self) -> Result<&SharedRbacService> {
        self.rbac.as_ref().ok_or_else(|| Error::Internal("供应商资料写入需要授权源".into()))
    }

    /// 按幂等键查询已成功的根级命令结果。
    ///
    /// # Errors
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
