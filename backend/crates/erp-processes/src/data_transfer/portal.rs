use application_core::AuditActor;
use erp_core::AccountKind;
use erp_identity::{PortalActor, PortalIdentityService};
use persistence_core::NoTransaction;
use serde_json::Value;

use super::BusinessTransfers;
use super::imports::{decode, required};
use crate::supplier_portal::{PortalBatchInput, SupplierPortalProcess};
use crate::{Error, Result};
impl BusinessTransfers {
    pub(super) async fn portal_context_access(&self, input: &Value, actor: &AuditActor) -> Result<()> {
        self.portal_actor(input, actor).await?.require_write()?;
        Ok(())
    }

    pub(super) async fn portal_recover(&self, input: &Value, actor: &AuditActor) -> Result<Value> {
        let portal = self.portal_actor(input, actor).await?;
        portal.require_write()?;
        let mut request: PortalBatchInput = decode(&input["request"])?;
        request.validate_only = true;
        request.recovery_only = true;
        let result = SupplierPortalProcess::new(self.db.clone(), self.rbac.clone())
            .batch_receipts(&portal, request)
            .await?;
        serde_json::to_value(result).map_err(json_error)
    }

    async fn portal_actor(&self, input: &Value, actor: &AuditActor) -> Result<PortalActor> {
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
                &mut NoTransaction,
            )
            .await
            .map_err(Into::into)
    }
    pub(super) async fn portal_authorize(&self, input: &Value, actor: &AuditActor) -> Result<()> {
        let portal = self.portal_actor(input, actor).await?;
        portal.require_write()?;
        let mut request: PortalBatchInput = decode(&input["request"])?;
        request.validate_only = true;
        SupplierPortalProcess::new(self.db.clone(), self.rbac.clone()).batch(&portal, request).await?;
        Ok(())
    }
    pub(super) async fn portal_batch(&self, input: &Value, actor: &AuditActor) -> Result<Value> {
        let portal = self.portal_actor(input, actor).await?;
        let result = SupplierPortalProcess::new(self.db.clone(), self.rbac.clone())
            .batch(&portal, decode(&input["request"])?)
            .await?;
        serde_json::to_value(result).map_err(json_error)
    }
}

fn json_error(_: serde_json::Error) -> Error {
    Error::Internal("任务结果无法编码".into())
}
