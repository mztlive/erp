use application_core::AuditActor;
use erp_core::AccountKind;
use erp_identity::{PortalActor, PortalIdentityService};
use erp_supplier_catalog::supplier_portal::PortalBatchInput;
use persistence_core::{Executor, ReadExecutor};
use serde_json::Value;

use super::BusinessTransfers;
use super::imports::{decode, required};
use crate::{Error, Result};
impl BusinessTransfers {
    pub(super) async fn portal_recover(&self, input: &Value, actor: &AuditActor) -> Result<Value> {
        let portal = self.portal_actor(input, actor).await?;
        portal.require_write()?;
        let mut request: PortalBatchInput = decode(&input["request"])?;
        request.validate_only = true;
        request.recovery_only = true;
        let result = self.services.portal.clone().batch_receipts(&portal, request).await?;
        serde_json::to_value(result).map_err(json_error)
    }

    async fn portal_actor(&self, input: &Value, actor: &AuditActor) -> Result<PortalActor> {
        self.portal_actor_with(input, actor, &mut ReadExecutor::default()).await
    }

    // 门户会话按原提交事务取证，不把事务外身份检查当作登记资格。
    //
    // input 含服务器冻结的会话版本，actor 为原身份，executor 为当前业务执行器。
    //
    // 当前有效的门户身份。
    //
    // 非门户身份、会话版本无效或取证失败时拒绝。
    pub(super) async fn portal_actor_with(
        &self,
        input: &Value,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<PortalActor> {
        if actor.kind() != AccountKind::Supplier {
            return Err(Error::Forbidden("此任务仅供供应商门户使用".into()));
        }
        let account_version = input["session"]["account_version"]
            .as_u64()
            .ok_or_else(|| Error::Forbidden("门户身份无效".into()))?;
        let binding_version = input["session"]["binding_version"]
            .as_u64()
            .ok_or_else(|| Error::Forbidden("门户身份无效".into()))?;
        PortalIdentityService::new(self.db.clone())
            .validate_session(
                actor.id(),
                required(&input["session"], "account")?,
                account_version,
                binding_version,
                executor,
            )
            .await
            .map_err(Into::into)
    }
    pub(super) async fn portal_authorize(&self, input: &Value, actor: &AuditActor) -> Result<()> {
        let portal = self.portal_actor(input, actor).await?;
        portal.require_write()?;
        let mut request: PortalBatchInput = decode(&input["request"])?;
        request.validate_only = true;
        self.services.portal.clone().batch(&portal, request).await?;
        Ok(())
    }
    pub(super) async fn portal_batch(&self, input: &Value, actor: &AuditActor) -> Result<Value> {
        let portal = self.portal_actor(input, actor).await?;
        let result = self.services.portal.clone().batch(&portal, decode(&input["request"])?).await?;
        serde_json::to_value(result).map_err(json_error)
    }
}

fn json_error(_: serde_json::Error) -> Error {
    Error::Internal("任务结果无法编码".into())
}
