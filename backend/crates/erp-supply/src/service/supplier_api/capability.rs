//! 事务内能力资格、变更和连接版本推进。
use persistence_core::Executor;

use super::SupplierApiService;
use super::command::{apply_validated_changes, map_capability_change_rejection};
use super::context::ensure_version;
use crate::entity::supplier_api::*;
use crate::repository::SupplierApiExt;
use crate::{Error, Result};
impl SupplierApiService {
    /// 执行原能力变更全部本域读取/校验/批量写入，不开启事务。
    ///
    /// # 参数
    /// * `id` - 供应商连接主键。
    /// * `expected_version` - 调用方持有的连接版本。
    /// * `change_set` - 待分类并应用的能力变更集。
    /// * `actor_id` - 记录能力配置的操作人。
    /// * `executor` - 调用方事务的执行器。
    ///
    /// # 返回
    /// 返回已推进版本的连接。
    ///
    /// # 错误
    /// 连接不存在时返回 `NotFound`；版本不一致时返回 `ConflictError`；连接处于启用状态时返回 `BusinessLogicError`。变更集分类、实时版本、采购确认覆盖、实体构造或仓储写入失败时返回对应错误。
    pub async fn apply_capability_changes(
        &self,
        id: &str,
        expected_version: u64,
        change_set: CapabilityChangeSet,
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
            return Err(Error::BusinessLogicError("连接启用期间不能修改能力，请先停用连接".to_string()));
        }
        let capabilities = self
            .db
            .supplier_api()
            .connection_capabilities(&SupplierApiConnectionId::new(id), executor)
            .await?;
        let confirmations = self
            .db
            .supplier_api()
            .business_confirmations(&SupplierApiConnectionId::new(id), executor)
            .await?;
        let classified = change_set.classify(&capabilities).map_err(map_capability_change_rejection)?;
        let (mut updates, creates) = apply_validated_changes(id, &classified, &confirmations, &capabilities)?;
        self.db.supplier_api().persist_capability_changes(&mut updates, &creates, executor).await?;
        connection.record_capability_configuration(actor_id)?;
        self.db.supplier_api_connections().update(&mut connection, executor).await?;
        Ok(connection)
    }
}
