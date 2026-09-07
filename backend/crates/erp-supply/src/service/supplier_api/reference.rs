//! 引用绑定本域校验与 CAS，外部注册表调用由流程层执行。
use super::{context::ensure_version, SupplierApiService};
use crate::entity::supplier_api::*;
use crate::ports::supplier_reference_registry::ResolvedSupplierReference;
use crate::repository::SupplierApiExt;
use crate::{Error, Result};
use persistence_core::Executor;
impl SupplierApiService {
    /// 事务外引用解析之前执行原版本与启用保护。
    pub async fn load_reference_target(
        &self,
        id: &str,
        expected_version: u64,
        executor: &mut dyn Executor,
    ) -> Result<SupplierApiConnection> {
        let connection = self.load_connection(id, executor).await?;
        ensure_version(connection.base.version, expected_version)?;
        if connection.stable.status == SupplierApiConnectionStatus::Active {
            return Err(Error::BusinessLogicError(
                "连接启用期间不能变更配置，请先停用连接".to_string(),
            ));
        }
        Ok(connection)
    }
    /// 结果事务重新读取并校验后应用内部引用；不会使用预检快照直接写入。
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
            return Err(Error::BusinessLogicError(
                "连接启用期间不能变更配置，请先停用连接".to_string(),
            ));
        }
        match action {
            SupplierConnectionAction::UpdateBusinessProfile => {
                connection.update_business_profile(resolved.internal_reference, actor_id)?
            }
            SupplierConnectionAction::BindEndpointReference => {
                connection.bind_endpoint_reference(resolved.internal_reference, actor_id)?
            }
            SupplierConnectionAction::BindCredentialReference => {
                connection.bind_credential_reference(resolved.internal_reference, actor_id)?
            }
            _ => return Err(Error::Internal("引用命令分派错误".to_string())),
        }
        self.db
            .supplier_api_connections()
            .update(&mut connection, executor)
            .await?;
        Ok(connection)
    }
}
