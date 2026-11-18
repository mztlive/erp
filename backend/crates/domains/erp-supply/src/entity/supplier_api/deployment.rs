//! 部署配置由服务器解析，业务用户不选择或提交技术引用。
use super::SupplierApiConnection;
use crate::Result;
use crate::ports::supplier_reference_registry::SupplierDeploymentConfig;

impl SupplierApiConnection {
    /// 更新当前部署引用；配置变化使旧健康证据失效。
    /// 引用实际变化时返回 true；相同配置不改变版本或健康状态。
    /// 引用非法或技术版本溢出时拒绝。
    pub fn apply_deployment(&mut self, config: &SupplierDeploymentConfig, actor: &str) -> Result<bool> {
        if self.technical_references_ready()
            && self.endpoint_reference == config.endpoint
            && self.credential_reference.as_ref() == Some(&config.credential)
        {
            return Ok(false);
        }

        self.bind_endpoint_reference(config.endpoint.clone(), actor)?;
        self.bind_credential_reference(config.credential.clone(), actor)?;
        Ok(true)
    }
}
