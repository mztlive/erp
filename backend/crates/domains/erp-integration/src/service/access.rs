//! 集成读取和对象授权委托给 identity，不解释权限条款。
use std::sync::Arc;

use application_core::AuditActor;
use erp_identity::CompiledAccess;
use persistence_core::Executor;

use crate::ports::IntegrationDataScopePort;
use crate::{Error, Result};

/// 集成授权端口的业务用例入口。
#[derive(Clone)]
pub struct IntegrationAccess {
    scope: Arc<dyn IntegrationDataScopePort>,
}
impl IntegrationAccess {
    /// 绑定固定授权端口。
    /// # 参数
    /// scope 为组合根登记的端口。
    /// # 返回
    /// 不缓存授权的业务入口。
    /// # 错误
    /// 不执行 I/O。
    pub fn new(scope: Arc<dyn IntegrationDataScopePort>) -> Self {
        Self { scope }
    }

    /// 取得本次读取的完整授权凭证。
    /// # 参数
    /// actor、resource、action 为真实身份与入口，executor 为原执行器。
    /// # 返回
    /// identity 编译的完整授权计划。
    /// # 错误
    /// 未装配、无权或版本变化时拒绝。
    pub async fn resolve(
        &self,
        actor: &AuditActor,
        resource: &str,
        action: &str,
        executor: &mut dyn Executor,
    ) -> Result<CompiledAccess> {
        self.scope.resolve(actor, resource, action, executor).await
    }

    /// 在交付结果前后复验本次读取凭证。
    /// # 参数
    /// access 为凭证，executor 为原执行器。
    /// # 返回
    /// 仍有效时成功。
    /// # 错误
    /// 未装配或授权变化时拒绝。
    pub async fn validate(&self, access: &CompiledAccess, executor: &mut dyn Executor) -> Result<()> {
        self.scope.validate(access, executor).await
    }

    /// 按实际记录身份重验完整对象资格。
    /// # 参数
    /// actor、resource、action 指定入口，id 为实际记录，executor 为原执行器。
    /// # 返回
    /// identity 完成对象授权后成功。
    /// # 错误
    /// 对象缺失、无权或版本变化时拒绝。
    pub async fn require_handler(
        &self,
        actor: &AuditActor,
        resource: &str,
        action: &str,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.scope.require(actor, resource, action, id, executor).await
    }
    /// 查询处理人在解析时点的唯一主属组织。
    ///
    /// # 参数
    /// * `user_id` - 拟写入的处理人
    /// * `at` - 与授权相同的解析时点
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 存在唯一主属组织时返回其 ID。
    ///
    /// # 错误
    /// 处理人 ID 为 `me` 时返回 `ValidationError`；未装配、组织关系非法或缺少有效内部组织时拒绝。
    pub async fn require_handler_org(
        &self,
        user_id: &str,
        at: erp_core::common::time::Instant,
        executor: &mut dyn Executor,
    ) -> Result<String> {
        if user_id.trim().eq_ignore_ascii_case("me") {
            return Err(Error::ValidationError("处理人不得使用 me 作为人员 ID".into()));
        }
        self.scope
            .own_org(user_id, at, executor)
            .await?
            .filter(|org| !org.eq_ignore_ascii_case("company"))
            .ok_or_else(|| Error::ValidationError("处理人缺少有效内部组织".into()))
    }

    /// 展开请求组织及其可选下级。
    ///
    /// # 参数
    /// * `org_unit_ids` - 请求中的组织 ID
    /// * `include_descendants` - 是否包含有效下级
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回启用节点的组织 ID 集合。
    ///
    /// # 错误
    /// 未知组织或未装配时拒绝。
    pub async fn expand_org_units(
        &self,
        org_unit_ids: &[String],
        include_descendants: bool,
        executor: &mut dyn Executor,
    ) -> Result<std::collections::BTreeSet<String>> {
        self.scope.expand_org_units(org_unit_ids, include_descendants, executor).await
    }
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use persistence_core::NoTransaction;

    use super::*;
    use crate::ports::FailClosedIntegrationDataScopePort;

    #[tokio::test]
    async fn unwired_record_authorization_fails_closed() {
        let access = IntegrationAccess::new(FailClosedIntegrationDataScopePort::shared());
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        assert!(matches!(
            access
                .require_handler(&actor, "integration_error_task", "detail", "record", &mut NoTransaction)
                .await,
            Err(Error::Internal(_))
        ));
    }
}
