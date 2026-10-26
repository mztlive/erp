//! 技术配置自动解析；任务登记事务内重验连接身份和版本。
use erp_supply::entity::supplier_api::SupplierApiConnection;
use erp_supply::ports::supplier_reference_registry::{SupplierDeploymentConfig, SupplierReferenceTarget};
use persistence_core::NoTransaction;

use super::SupplierApiGovernanceProcess;
use crate::{Error, Result};

pub(super) struct Deployment {
    target: SupplierReferenceTarget,
    config: SupplierDeploymentConfig,
}
impl SupplierApiGovernanceProcess {
    /// 只读解析当前部署，不发送外部请求。
    /// # 参数
    /// `id` 为已授权连接。
    /// # 返回
    /// 当前连接与部署配置的绑定。
    /// # 错误
    /// 连接不存在或配置未登记时拒绝。
    pub(super) async fn deployment(&self, id: &str) -> Result<Deployment> {
        let connection = self.domain().load_connection(id, &mut NoTransaction).await?;
        let target = SupplierReferenceTarget::from(&connection);
        let config = self
            .reference_registry
            .deployment_config(&target)
            .map_err(|error| Error::BusinessLogicError(format!("{}: {}", error.code, error.summary)))?;
        Ok(Deployment { target, config })
    }
}
impl Deployment {
    /// 确认启用使用已通过测试的当前配置，不把旧证据用于新部署。
    /// # 参数
    /// `connection` 为事务内重读的连接。
    /// # 返回
    /// 配置身份一致时通过。
    /// # 错误
    /// 引用或连接身份变化时拒绝。
    pub(super) fn ensure_current(&self, connection: &SupplierApiConnection) -> Result<()> {
        if SupplierReferenceTarget::from(connection) != self.target
            || connection.endpoint_reference != self.config.endpoint
            || connection.credential_reference.as_ref() != Some(&self.config.credential)
        {
            return Err(Error::ConflictError("接口配置尚未验证或已变化，请先测试连接".into()));
        }
        Ok(())
    }
    /// 提交前核对连接身份，禁止将预检配置绑定给已变化的连接。
    /// # 参数
    /// `connection` 为当前事务连接，`actor` 为操作人。
    /// # 返回
    /// 配置引用发生变化时为 true。
    /// # 错误
    /// 连接身份变化或实体配置更新失败时拒绝。
    pub(super) fn apply(&self, connection: &mut SupplierApiConnection, actor: &str) -> Result<bool> {
        if SupplierReferenceTarget::from(&*connection) != self.target {
            return Err(Error::ConflictError("连接身份已变化，请刷新后重试".into()));
        }
        Ok(connection.apply_deployment(&self.config, actor)?)
    }
}
