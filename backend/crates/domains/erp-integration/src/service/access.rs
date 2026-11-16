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

use crate::repository::integration_ops::reader::IntegrationReader;
use crate::{Error, Result};

/// 固定登记的集成授权设施；查询凭证绑定本实例。
#[derive(Clone)]
pub struct IntegrationAccess {
    db: Database,
    pub(super) service: Arc<AuthorizationService>,
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use erp_core::AccountKind;
    use erp_identity::entity::access_control::{ResolvedScope, ScopeClause};
    use erp_identity::testing::authorization;
    use persistence_core::Error as PersistenceError;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;
    use crate::repository::integration_ops::reader::testing::{
        Behavior, MemoryReader, actor, own_scope, service as integration_service,
    };

    #[tokio::test]
    async fn direct_decision_uses_real_difference_identity_version_and_assignment() {
        let reader = Arc::new(MemoryReader::default());
        let service =
            integration_service(vec![integration_ops::DIFFERENCE_DECISION], &[], own_scope(), reader.clone())
                .await;
        service
            .access()
            .require_decide_using(reader.as_ref(), &actor(), "difference", &mut SnapshotExecutor::default())
            .await
            .unwrap();
        assert_eq!(*reader.events.lock().unwrap(), ["difference_facts", "difference_facts"]);
    }

    #[tokio::test]
    async fn direct_decision_rejects_another_handler_even_with_company_scope() {
        let reader = Arc::new(MemoryReader::default());
        reader.difference.lock().unwrap().as_mut().unwrap().owner_user_id = "other".into();
        let scope = ResolvedScope {
            role_clauses: vec![ScopeClause { company: true, ..Default::default() }],
            user_limit: None,
        };
        let service =
            integration_service(vec![integration_ops::DIFFERENCE_DECISION], &[], scope, reader.clone()).await;
        assert!(matches!(
            service
                .access()
                .require_decide_using(
                    reader.as_ref(),
                    &actor(),
                    "difference",
                    &mut SnapshotExecutor::default()
                )
                .await,
            Err(Error::Forbidden(_))
        ));
    }

    #[tokio::test]
    async fn direct_decision_cannot_use_list_detail_or_another_actor_instead_of_decision_qualification() {
        for (grants, user) in [
            (vec![integration_ops::DIFFERENCE_LIST, integration_ops::DIFFERENCE_DETAIL], actor()),
            (
                vec![integration_ops::DIFFERENCE_DECISION],
                AuditActor::new("other".into(), "other".into(), AccountKind::Admin),
            ),
        ] {
            let reader = Arc::new(MemoryReader::default());
            let service = integration_service(grants, &[], own_scope(), reader.clone()).await;
            assert!(matches!(
                service
                    .access()
                    .require_decide_using(
                        reader.as_ref(),
                        &user,
                        "difference",
                        &mut SnapshotExecutor::default()
                    )
                    .await,
                Err(Error::Forbidden(_))
            ));
            assert_eq!(*reader.events.lock().unwrap(), ["difference_facts"], "功能拒绝先于任务 Facts 读取");
        }
    }

    #[tokio::test]
    async fn direct_decision_rejects_missing_root_facts_binding_and_incomplete_facts() {
        let reader = Arc::new(MemoryReader::default());
        let service =
            integration_service(vec![integration_ops::DIFFERENCE_DECISION], &[], own_scope(), reader.clone())
                .await;
        assert!(
            matches!(service.access().require_decide_using(reader.as_ref(), &actor(), "missing", &mut SnapshotExecutor::default()).await, Err(Error::NotFound(message)) if message == "差异记录不存在")
        );
        let mut access = service.access();
        access.service = authorization(
            actor(),
            vec![integration_ops::DIFFERENCE_DECISION],
            own_scope(),
            BTreeMap::new(),
            BTreeMap::new(),
        )
        .unwrap();
        assert!(
            matches!(access.require_decide_using(reader.as_ref(), &actor(), "difference", &mut SnapshotExecutor::default()).await, Err(Error::ValidationError(message)) if message == "缺少任务 Facts 绑定")
        );
        let reader = Arc::new(MemoryReader::with_behavior(Behavior::MissingFacts));
        let service =
            integration_service(vec![integration_ops::DIFFERENCE_DECISION], &[], own_scope(), reader.clone())
                .await;
        assert!(matches!(
            service
                .access()
                .require_decide_using(
                    reader.as_ref(),
                    &actor(),
                    "difference",
                    &mut SnapshotExecutor::default()
                )
                .await,
            Err(Error::NotFound(_))
        ));
        assert_eq!(*reader.events.lock().unwrap(), ["difference_facts", "difference_facts"]);
    }

    #[tokio::test]
    async fn direct_decision_preserves_exact_version_conflict_and_facts_storage_error() {
        for behavior in [Behavior::ChangedVersion, Behavior::FactsFailure] {
            let reader = Arc::new(MemoryReader::with_behavior(behavior));
            let service = integration_service(
                vec![integration_ops::DIFFERENCE_DECISION],
                &[],
                own_scope(),
                reader.clone(),
            )
            .await;
            let error = service
                .access()
                .require_decide_using(
                    reader.as_ref(),
                    &actor(),
                    "difference",
                    &mut SnapshotExecutor::default(),
                )
                .await
                .unwrap_err();
            if behavior == Behavior::ChangedVersion {
                assert!(matches!(error, Error::ConflictError(message) if message == "任务版本已改变"));
            } else {
                assert_eq!(
                    error.to_string(),
                    Error::from(PersistenceError::EntityMetadataOutOfRange("测试 Facts 读取失败"))
                        .to_string()
                );
            }
            assert_eq!(*reader.events.lock().unwrap(), ["difference_facts", "difference_facts"]);
        }
    }
}
impl IntegrationAccess {
    /// 绑定共享身份设施和本域固定目录。
    /// # 参数
    /// db、rbac 为组合根共享身份设施。
    /// # 返回
    /// 不缓存授权结果的访问器。
    /// # 错误
    /// 固定目录非法时启动失败；构造不执行 I/O。
    pub fn new(db: Database, service: Arc<AuthorizationService>) -> Self {
        Self { service, db }
    }

    /// 查询并编译固定资源的完整授权条件。
    /// # 参数
    /// actor、permission 指定服务端入口，executor 为原执行器。
    /// # 返回
    /// identity 生成的不可伪造查询凭证。
    /// # 错误
    /// 未登记入口、无权、映射或事实失败时拒绝。
    pub async fn resolve(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        executor: &mut dyn Executor,
    ) -> Result<CompiledAccess> {
        Ok(self.service.query(actor, permission, executor).await?)
    }

    /// 复验同一服务生成的读取凭证。
    /// # 参数
    /// access、executor 为本次凭证及原执行器。
    /// # 返回
    /// 仍有效时成功。
    /// # 错误
    /// 身份、权限、映射或版本变化时拒绝。
    pub async fn validate(&self, access: &CompiledAccess, executor: &mut dyn Executor) -> Result<()> {
        Ok(self.service.validate(access, executor).await?)
    }

    /// 检查真实差异记录的当前处理人和精确版本。
    /// # 参数
    /// actor、id 为真实账号和差异ID，executor 为决定写事务。
    /// # 返回
    /// 实际任务版本、指派及固定资格均通过时成功。
    /// # 错误
    /// 差异缺失、版本或指派不符时拒绝。
    pub async fn require_decide(
        &self,
        actor: &AuditActor,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.require_decide_using(&self.db, actor, id, executor).await
    }

    async fn require_decide_using(
        &self,
        reader: &dyn IntegrationReader,
        actor: &AuditActor,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let row =
            reader.difference(id, executor).await?.ok_or_else(|| Error::NotFound("差异记录不存在".into()))?;
        Ok(self
            .service
            .require(
                actor,
                &integration_ops::DIFFERENCE_DECISION,
                AccessTarget::Task { id: row.base.id, version: row.base.version.to_string() },
                executor,
            )
            .await?)
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
    /// 处理人 ID 为 `me` 时返回 `ValidationError`；组织关系非法或缺少有效内部组织时拒绝。
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
    /// # 参数
    /// * `org_unit_ids` - 请求中的组织 ID
    /// * `include_descendants` - 是否包含有效下级
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回启用节点的组织 ID 集合。
    ///
    /// # 错误
    /// 未知组织时拒绝。
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
