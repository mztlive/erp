//! 引用绑定本域校验与 CAS，外部注册表调用由流程层执行。
use persistence_core::Executor;

use super::SupplierApiService;
use super::context::ensure_version;
use crate::entity::supplier_api::*;
use crate::ports::supplier_reference_registry::ResolvedSupplierReference;
use crate::repository::SupplierApiExt;
use crate::{Error, Result};
impl SupplierApiService {
    /// 事务外引用解析之前执行原版本与启用保护。
    ///
    /// # 参数
    /// * `id` - 供应商连接主键。
    /// * `expected_version` - 调用方持有的连接版本。
    /// * `executor` - 调用方事务的执行器。
    ///
    /// # 返回
    /// 返回通过版本与停用保护的连接。
    ///
    /// # 错误
    /// 连接不存在或版本不一致时返回对应错误；连接处于启用状态时返回 `BusinessLogicError`。
    pub async fn load_reference_target(
        &self,
        id: &str,
        expected_version: u64,
        executor: &mut dyn Executor,
    ) -> Result<SupplierApiConnection> {
        let connection = self.load_connection(id, executor).await?;
        ensure_version(connection.base.version, expected_version)?;
        if connection.stable.status == SupplierApiConnectionStatus::Active {
            return Err(Error::BusinessLogicError("连接启用期间不能变更配置，请先停用连接".to_string()));
        }
        Ok(connection)
    }
    /// 结果事务重新读取并校验后应用内部引用；不会使用预检快照直接写入。
    ///
    /// # 参数
    /// * `id` - 供应商连接主键。
    /// * `action` - 业务资料、端点或凭证引用绑定动作。
    /// * `expected_version` - 调用方持有的连接版本。
    /// * `resolved` - 已解析的内部引用。
    /// * `actor_id` - 操作人。
    /// * `executor` - 调用方事务的执行器。
    ///
    /// # 返回
    /// 返回已写回的连接。
    ///
    /// # 错误
    /// 连接不存在或版本不一致时返回对应错误；连接处于启用状态时返回 `BusinessLogicError`。动作不是三种引用绑定之一时返回 `Internal`。实体更新或仓储写回失败时返回对应错误。
    pub async fn apply_reference(
        &self,
        id: &str,
        action: SupplierConnectionAction,
        expected_version: u64,
        resolved: ResolvedSupplierReference,
        actor_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<SupplierApiConnection> {
        let mut connection = self
            .db
            .supplier_api()
            .connection(&SupplierApiConnectionId::new(id), executor)
            .await?
            .ok_or_else(|| Error::NotFound("连接不存在".to_string()))?;
        ensure_version(connection.base.version, expected_version)?;
        if connection.stable.status == SupplierApiConnectionStatus::Active {
            return Err(Error::BusinessLogicError("连接启用期间不能变更配置，请先停用连接".to_string()));
        }
        match action {
            SupplierConnectionAction::UpdateBusinessProfile => {
                connection.update_business_profile(resolved.internal_reference, actor_id)?
            },
            SupplierConnectionAction::BindEndpointReference => {
                connection.bind_endpoint_reference(resolved.internal_reference, actor_id)?
            },
            SupplierConnectionAction::BindCredentialReference => {
                connection.bind_credential_reference(resolved.internal_reference, actor_id)?
            },
            _ => return Err(Error::Internal("引用命令分派错误".to_string())),
        }
        self.db.supplier_api_connections().update(&mut connection, executor).await?;
        Ok(connection)
    }
}
