//! 开启自动下单前自动检查当前配置，不创建供应商订单。
use application_core::AuditActor;
use erp_identity::AccessTarget;
use erp_permissions::internal;
use erp_supply::entity::supplier_api::{
    SupplierApiCapabilityCode, SupplierApiConnection, SupplierHealthCheckType,
};
use erp_supply::ports::supplier_reference_registry::SupplierReferenceTarget;
use persistence_core::{Executor, Transactional};

use super::SupplierApiGovernanceProcess;
use super::deployment::Deployment;
use crate::{Error, Result};

impl SupplierApiGovernanceProcess {
    /// 在事务外验证目标版本、协议能力与当前配置的鉴权。
    /// # 参数
    /// `id` 为连接，`version` 为请求版本，`actor` 为已授权操作人。
    /// # 返回
    /// 通过检查的部署配置；提交事务再次校验连接版本和身份。
    /// # 错误
    /// 版本冲突、缺少下单能力、配置或鉴权失败时拒绝。
    pub(super) async fn checked_deployment(
        &self,
        id: &str,
        version: u64,
        actor: &AuditActor,
    ) -> Result<Deployment> {
        let (process, id, operator) = (self.clone(), id.to_owned(), actor.clone());
        let (connection, deployment) = self
            .db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { process.activation_target(&id, version, &operator, executor).await })
            })
            .await?;

        let gateway =
            self.gateway.as_ref().ok_or_else(|| Error::BusinessLogicError("供应商连接检查未配置".into()))?;
        gateway
            .health_check(&connection, SupplierHealthCheckType::Authentication)
            .await
            .map_err(|error| Error::BusinessLogicError(format!("连接检查失败：{}", error.summary)))?;
        Ok(deployment)
    }

    async fn activation_target(
        &self,
        id: &str,
        version: u64,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<(SupplierApiConnection, Deployment)> {
        self.authorization
            .require(actor, &internal::CONNECTION_ENABLE, AccessTarget::Operation, executor)
            .await?;

        let mut connection = self.domain().load_connection(id, executor).await?;
        if connection.base.version != version {
            return Err(Error::ConflictError("连接已变化，请刷新后重试".into()));
        }

        let capabilities = self
            .reference_registry
            .capabilities(&SupplierReferenceTarget::from(&connection))
            .map_err(|error| Error::BusinessLogicError(error.summary))?;
        if !capabilities.contains(&SupplierApiCapabilityCode::Order) {
            return Err(Error::BusinessLogicError("该供应商接口不支持自动下单".into()));
        }

        let deployment = self.deployment(&connection)?;
        deployment.apply(&mut connection, actor.id())?;
        Ok((connection, deployment))
    }
}
