//! 供应商资料根级命令。
//!
//! 页面只调用本服务维护 Party、Supplier、当前事实与独立资质；服务在一个
//! MongoDB 事务中提交全部写入，并把幂等结果与业务数据一并落库。

mod create;
mod sensitive;
#[cfg(test)]
mod tests;
mod update;
mod validation;

use std::sync::Arc;

use erp_supplier::SupplierExt;
use erp_supplier::SupplierProfileCommand;
use mongodb::Database;
use persistence_core::NoTransaction;

use crate::Result;
use erp_party::SensitiveDataCodec;
use erp_supplier::{command_view, SupplierProfileMutationView};

mod party_change;

/// 完整供应商资料的根级写服务。
pub struct SupplierProfileService {
    db: Database,
    sensitive_data: Arc<SensitiveDataCodec>,
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
        Self { db, sensitive_data }
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
        Ok(self
            .db
            .supplier()
            .profile_command(idempotency_key, &mut NoTransaction)
            .await?)
    }
}
