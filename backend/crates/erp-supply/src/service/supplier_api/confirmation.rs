//! 追加业务能力确认；连接版本先于确认事实和审计写入。
use super::{
    context::{digest, ensure_version},
    SupplierApiService,
};
use crate::dto::supplier_api::ConfirmBusinessCapabilityRequirementCommand;
use crate::entity::supplier_api::*;
use crate::repository::SupplierApiExt;
use crate::{Error, Result};
use erp_core::common::time::Instant;
use persistence_core::Executor;
/// 已推进连接版本的本域确认准备结果。
pub struct PreparedBusinessConfirmation {
    pub connection: SupplierApiConnection,
    pub capability: SupplierApiCapability,
    pub confirmation: BusinessCapabilityConfirmation,
}
/// 确认命令的本域输入；审计身份只保留稳定操作人 ID。
pub struct BusinessConfirmationInput<'a> {
    pub id: String,
    pub command: ConfirmBusinessCapabilityRequirementCommand,
    pub operation_id: String,
    pub idempotency_hash: String,
    pub fingerprint: String,
    pub actor_id: &'a str,
}
impl SupplierApiService {
    /// 复用原事务读取和首个连接 CAS；调用方随后构造审计并持久化确认。
    pub async fn prepare_business_confirmation(
        &self,
        input: BusinessConfirmationInput<'_>,
        executor: &mut dyn Executor,
    ) -> Result<PreparedBusinessConfirmation> {
        let BusinessConfirmationInput {
            id,
            command,
            operation_id,
            idempotency_hash,
            fingerprint,
            actor_id,
        } = input;
        let mut connection = self
            .db
            .supplier_api()
            .connection(&SupplierApiConnectionId::new(&id), executor)
            .await?
            .ok_or_else(|| Error::NotFound("连接不存在".to_string()))?;
        ensure_version(connection.base.version, command.expected_connection_version)?;
        let capability = self
            .db
            .supplier_api()
            .connection_capability(
                &SupplierApiConnectionId::new(&id),
                command.capability_code,
                executor,
            )
            .await?
            .ok_or_else(|| Error::NotFound("连接能力不存在".to_string()))?;
        ensure_version(capability.base.version, command.expected_capability_version)?;
        let confirmation = BusinessCapabilityConfirmation::new(
            format!("w20-confirm-{}", digest(&[&id, &operation_id])),
            BusinessCapabilityConfirmationData {
                connection_id: SupplierApiConnectionId::new(id.clone()),
                capability_id: SupplierApiCapabilityId::new(capability.base.id.clone()),
                capability_code: command.capability_code,
                requirement: command.requirement,
                applicability_reference: command.applicability_reference,
                evidence_references: command.evidence_references,
                reason_code: command.reason_code,
                connection_version: connection.base.version,
                capability_version: capability.base.version,
                operation_id: operation_id.clone(),
                idempotency_key_hash: idempotency_hash,
                request_fingerprint: fingerprint.clone(),
                confirmed_by: actor_id.to_string(),
                confirmed_at: Instant::now(),
            },
        )?;
        connection.touch_business_confirmation(actor_id);
        self.db
            .supplier_api_connections()
            .update(&mut connection, executor)
            .await?;
        Ok(PreparedBusinessConfirmation {
            connection,
            capability,
            confirmation,
        })
    }
    /// 写入不可变确认，保留与审计的外层交错位置。
    pub async fn persist_business_confirmation(
        &self,
        confirmation: &BusinessCapabilityConfirmation,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.db
            .supplier_api_business_confirmations()
            .create(confirmation, executor)
            .await?;
        Ok(())
    }
}
