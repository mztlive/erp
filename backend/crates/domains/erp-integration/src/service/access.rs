//! 集成授权、查询编译与业务组织事实入口。
use std::collections::BTreeSet;
use std::sync::Arc;

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_identity::{
    AccessTarget, AuthorizationService, CompiledAccess, OrganizationDirectoryService, Permission,
};
use erp_permissions::integration_ops;
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};

/// 固定登记的集成授权设施；查询凭证绑定本实例。
#[derive(Clone)]
pub struct IntegrationAccess {
    db: Database,
    pub(super) service: Arc<AuthorizationService>,
}

impl IntegrationAccess {
    /// 绑定共享身份设施和本域固定目录。
    ///
    /// 不缓存授权结果的访问器。固定目录非法时启动失败；构造不执行 I/O。
    ///
    /// db、rbac 为组合根共享身份设施。
    pub fn new(db: Database, service: Arc<AuthorizationService>) -> Self {
        Self { service, db }
    }

    /// 为当前列表动作编译集成域的完整授权查询条件，供仓储和交付复验共用。
    /// 只接受服务端登记的权限，使用原执行器；入口未登记、无权或授权事实不可用时拒绝。
    pub async fn resolve(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        executor: &mut dyn Executor,
    ) -> Result<CompiledAccess> {
        Ok(self.service.query(actor, permission, executor).await?)
    }

    /// 交付集成列表前，用原读取执行器复验本次查询凭证；账号、权限、映射或事实版本变化时不交付结果。
    pub async fn validate(&self, access: &CompiledAccess, executor: &mut dyn Executor) -> Result<()> {
        Ok(self.service.validate(access, executor).await?)
    }

    /// 在原执行器上检查集成处理入口：新建错误任务或差异只检查功能资格，已有记录的操作按实际对象做完整授权。
    /// 对象缺失、无权或授权事实不可用时拒绝。
    pub async fn require_handler(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let target = if [integration_ops::ERROR_TASK_CREATE, integration_ops::DIFFERENCE_CREATE]
            .contains(permission)
        {
            AccessTarget::Operation
        } else {
            AccessTarget::record(id)?
        };
        Ok(self.service.require(actor, permission, target, executor).await?)
    }
    /// 无正式任务的差异进入直接对账前，在写事务内检查实际差异、当前处理人归属和固定决定资格。
    /// 任一条件不满足即拒绝；决定的乐观锁版本由命令执行阶段核对。
    pub async fn require_decide(
        &self,
        actor: &AuditActor,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        Ok(self
            .service
            .require(actor, &integration_ops::DIFFERENCE_DECISION, AccessTarget::record(id)?, executor)
            .await?)
    }

    /// 查询处理人在解析时点的唯一主属组织。
    ///
    /// 存在唯一主属组织时返回其 ID。处理人 ID 为 `me` 时返回 `ValidationError`；组织关系非法或缺少有效内部组织时拒绝。
    ///
    /// 沿用调用方的执行器，不另开事务。
    pub async fn require_handler_org(
        &self,
        user_id: &str,
        at: Instant,
        executor: &mut dyn Executor,
    ) -> Result<String> {
        if user_id.trim().eq_ignore_ascii_case("me") {
            return Err(Error::ValidationError("处理人不得使用 me 作为人员 ID".into()));
        }
        OrganizationDirectoryService::new(self.db.clone())
            .own_org(user_id, at, executor)
            .await?
            .filter(|org| !org.eq_ignore_ascii_case("company"))
            .ok_or_else(|| Error::ValidationError("处理人缺少有效内部组织".into()))
    }

    /// 展开请求组织及其可选下级。
    ///
    /// 返回启用节点的组织 ID 集合。未知组织时拒绝。
    ///
    /// 沿用调用方的执行器，不另开事务。
    pub async fn expand_org_units(
        &self,
        org_unit_ids: &[String],
        include_descendants: bool,
        executor: &mut dyn Executor,
    ) -> Result<BTreeSet<String>> {
        Ok(OrganizationDirectoryService::new(self.db.clone())
            .expand_org_units(org_unit_ids, include_descendants, executor)
            .await?)
    }
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use erp_identity::testing::unavailable_authorization;
    use persistence_core::ReadExecutor;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;

    #[tokio::test]
    async fn difference_decision_requires_identity_before_business_reads() {
        let client =
            mongodb::Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1").await.unwrap();
        let access =
            IntegrationAccess::new(client.database("difference_authorization"), unavailable_authorization());
        let actor = AuditActor::new("user".into(), "login".into(), AccountKind::Admin);

        assert!(matches!(
            access.require_decide(&actor, "difference", &mut SnapshotExecutor::default()).await,
            Err(Error::Internal(reason)) if reason == "测试授权设施不可用"
        ));
        assert!(matches!(
            access.require_decide(&actor, "difference", &mut ReadExecutor::default()).await,
            Err(Error::ValidationError(_))
        ));
    }
}
