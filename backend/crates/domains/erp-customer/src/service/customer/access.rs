//! 客户用例仅执行统一授权查询，不解释角色范围或记录授权。
use std::collections::BTreeSet;
use std::sync::Arc;

use application_core::AuditActor;
use erp_core::common::time::{BusinessDate, Instant};
use erp_identity::entity::authorization::{FactValue, ObjectFacts};
use erp_identity::{
    AccessTarget, AuthorizationService, Error as IdentityError, OrganizationDirectoryService,
};
use erp_permissions::{Permission, customer, internal};
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use crate::authorization::CustomerResolvedScope;
use crate::entity::customer::AssignmentRole;
use crate::repository::CustomerExt;
use crate::repository::prelude::*;
use crate::repository::scope::CustomerReadScope;
use crate::{Error, Result};

/// 客户查询与命令共用的统一授权接入。
#[derive(Clone)]
pub struct CustomerAccess {
    db: Database,
    pub(crate) service: Arc<AuthorizationService>,
}
impl CustomerAccess {
    /// 绑定客户仓储、固定资源登记与身份设施。
    /// # 参数
    /// db 为客户数据库，service 为应用共享授权服务。
    /// # 返回
    /// 不缓存跨请求授权的客户访问器。
    /// # 错误
    /// 构造不执行 I/O；完整绑定由共享服务构造时检查。
    pub fn new(db: Database, service: Arc<AuthorizationService>) -> Self {
        Self { db, service }
    }

    /// 在创建前按真实负责人及其组织校验候选客户。
    /// # 参数
    /// actor 为真实操作人，id 为拟创建客户，executor 为原写入事务。
    /// # 返回
    /// 创建范围覆盖候选归属时成功。
    /// # 错误
    /// 负责人组织缺失、无权限或范围不匹配时拒绝。
    pub async fn require_candidate(
        &self,
        actor: &AuditActor,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let (organization, valid_until) =
            self.organization().own_org_with_validity(actor.id(), Instant::now(), executor).await?;
        self.authorize_creation(actor, id, organization, valid_until, executor).await
    }

    /// 将真实创建人及其组织事实构造成创建候选，不接受请求提交的负责人或授权事实。
    async fn authorize_creation(
        &self,
        actor: &AuditActor,
        id: &str,
        organization: Option<String>,
        valid_until: Option<Instant>,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let organization =
            organization.ok_or_else(|| Error::ValidationError("请先维护负责人的有效主属组织".into()))?;
        let candidate = ObjectFacts {
            resource: "customer".into(),
            id: id.into(),
            owner_ids: FactValue::Known([actor.id().to_owned()].into()),
            collaborators: FactValue::Known(Default::default()),
            org_unit_ids: FactValue::Known([organization].into()),
            valid_until,
            ..Default::default()
        };

        Ok(self
            .service
            .require(actor, &customer::CREATE, AccessTarget::Candidate(candidate), executor)
            .await?)
    }

    /// 同时检查原客户范围与新增负责人的真实归属。
    /// # 参数
    /// actor 为当前账号，id 为客户，owner 为已验证的目标负责人，executor 为原写事务。
    /// # 返回
    /// 原记录与新负责人归属均在范围内时成功。
    /// # 错误
    /// 目标组织缺失、任一范围不匹配或身份取证失败时拒绝。
    pub(crate) async fn require_owner_change(
        &self,
        actor: &AuditActor,
        id: &str,
        owner: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.require_owner_set_change(actor, id, &[owner.to_owned()].into(), executor).await
    }

    /// 在原事务同时校验原客户及变更后完整负责人的归属。
    /// # 参数
    /// actor、id 为当前入口及客户，owners 为生效区间内的真实完整目标负责人，executor 为原写事务。
    /// # 返回
    /// 原客户及目标归属均满足更新范围时成功。
    /// # 错误
    /// 目标组织缺失、原记录或目标归属越权时拒绝。
    pub(crate) async fn require_owner_set_change(
        &self,
        actor: &AuditActor,
        id: &str,
        owners: &BTreeSet<String>,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let candidate = self.ownership_candidate("", owners, executor).await?;
        Ok(self
            .service
            .require(
                actor,
                &customer::UPDATE,
                AccessTarget::OwnershipChange { id: id.into(), candidate },
                executor,
            )
            .await?)
    }

    /// 从原事务中的真实负责人关系构造候选，并保留组织关系的最近变化边界。
    async fn ownership_candidate(
        &self,
        id: &str,
        owners: &BTreeSet<String>,
        executor: &mut dyn Executor,
    ) -> Result<ObjectFacts> {
        let at = Instant::now();
        let mut organizations = BTreeSet::new();
        let mut valid_until: Option<Instant> = None;
        for owner in owners {
            let (org, boundary) = self.organization().own_org_with_validity(owner, at, executor).await?;
            organizations
                .insert(org.ok_or_else(|| Error::ValidationError("请先维护负责人的有效主属组织".into()))?);
            if let Some(boundary) = boundary {
                valid_until = Some(valid_until.map_or(boundary, |current| current.min(boundary)));
            }
        }

        Ok(ObjectFacts {
            resource: "customer".into(),
            id: id.into(),
            owner_ids: FactValue::Known(owners.clone()),
            collaborators: FactValue::Known(Default::default()),
            org_unit_ids: FactValue::Known(organizations),
            valid_until,
            ..Default::default()
        })
    }

    /// 返回身份域组织目录，用于业务筛选与责任事实。
    /// # 参数
    /// 无。
    /// # 返回
    /// 复用当前数据库的组织目录。
    /// # 错误
    /// 不执行 I/O。
    pub fn organization(&self) -> OrganizationDirectoryService {
        OrganizationDirectoryService::new(self.db.clone())
    }

    /// 复验本次查询凭证的身份、范围与有效期。
    /// # 参数
    /// access 为本访问器生成的上下文，executor 为原执行器。
    /// # 返回
    /// 凭证仍有效时成功。
    /// # 错误
    /// 身份、映射、会话或有效期变化时拒绝。
    pub async fn validate(&self, access: &CustomerResolvedScope, executor: &mut dyn Executor) -> Result<()> {
        Ok(self.service.validate(&access.compiled, executor).await?)
    }

    /// 在固定客户目录编译本阶段完整查询条件。
    pub(super) async fn query_context(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        executor: &mut dyn Executor,
    ) -> Result<CustomerResolvedScope> {
        if permission.resource() != "customer" {
            return Err(Error::ValidationError("客户访问器不能使用其他资源权限".into()));
        }
        let compiled = self.service.query(actor, permission, executor).await?;
        Ok(CustomerResolvedScope {
            user_id: actor.id().into(),
            scope_version: compiled.version(),
            as_of: compiled.as_of(),
            compiled: Arc::new(compiled),
        })
    }

    /// 准备查询凭证和业务归属筛选，不预装全量客户主键。
    /// # 参数
    /// actor、permission 为当前入口，executor 为原事务。
    /// # 返回
    /// 当前查询凭证与「我的客户」筛选事实。
    /// # 错误
    /// 身份、授权或业务归属读取失败时拒绝。
    pub async fn query(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        executor: &mut dyn Executor,
    ) -> Result<(CustomerResolvedScope, CustomerReadScope)> {
        let context = self.query_context(actor, permission, executor).await?;

        let assignments = self
            .db
            .customer_assignments()
            .find_active_assignments_for_user(actor.id(), business_date(context.as_of)?, executor)
            .await?;
        let owned = assignments
            .into_iter()
            .filter(|a| a.assignment_role == AssignmentRole::Owner)
            .map(|a| a.customer_id.to_string())
            .collect();
        Ok((context, CustomerReadScope { owned_customer_ids: owned }))
    }
    /// 在原业务事务使用 identity 检查完整客户及全部负责人事实。
    /// # 参数
    /// actor、permission、customer_id 为真实入口和目标，executor 为原事务。
    /// # 返回
    /// 完整对象授权通过时成功，不生成查询凭证。
    /// # 错误
    /// 缺失、无权、版本变化或接入失败时拒绝。
    pub async fn require_with(
        &self,
        actor: AuditActor,
        permission: &Permission,
        customer_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if permission.resource() != "customer" {
            return Err(Error::ValidationError("客户访问器不能使用其他资源权限".into()));
        }
        self.service.require(&actor, permission, AccessTarget::record(customer_id)?, executor).await.map_err(
            |error| match error {
                IdentityError::NotFound(_) | IdentityError::Forbidden(_) => deny_object(permission),
                error => error.into(),
            },
        )
    }
    /// 通过固定客户选择权限检查当前业务资格。
    /// # 参数
    /// actor、customer_id 为真实账号和客户，executor 为原事务。
    /// # 返回
    /// 当前客户资格成立时成功。
    /// # 错误
    /// 仅有历史资格、客户缺失或无权时拒绝。
    pub async fn require_current(
        &self,
        actor: &AuditActor,
        customer_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        Ok(self
            .service
            .require(actor, &internal::CUSTOMER_SELECT, AccessTarget::record(customer_id)?, executor)
            .await?)
    }
    /// 在独立事务检查以当前账号为首条负责人的客户创建候选。
    /// # 参数
    /// actor 为当前账号。
    /// # 返回
    /// 当前创建候选同时通过功能资格和归属范围时成功。
    /// # 错误
    /// 身份、负责人组织、创建范围或候选有效期不满足时拒绝。
    pub async fn ensure_create(&self, actor: &AuditActor) -> Result<()> {
        let this = self.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { this.require_create(&actor, executor).await })
            })
            .await
    }
    /// 在原事务检查服务端创建候选；持久化前仍须带实际客户 ID 复验。
    /// # 参数
    /// actor 为固定首条负责人的当前账号，executor 为原写事务；此时尚未分配客户 ID。
    /// # 返回
    /// 创建功能资格及真实负责人归属范围均通过时成功。
    /// # 错误
    /// 账号、负责人组织、创建范围、有效期或快照边界无效时拒绝。
    pub async fn require_create(&self, actor: &AuditActor, executor: &mut dyn Executor) -> Result<()> {
        self.require_candidate(actor, "", executor).await
    }
}
pub(crate) fn business_date(at: Instant) -> Result<BusinessDate> {
    let date = at.as_utc() + chrono::Duration::hours(8);
    Ok(date.format("%Y-%m-%d").to_string().parse()?)
}

/// 拒绝不可见对象且不泄露存在性差异。
///
/// # 参数
/// * `permission` - 对应的公共客户权限常量。
///
/// # 返回
/// 读取返回 NotFound；写入返回 Forbidden。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 不可读详情不得暴露客户是否存在。
pub(super) fn deny_object(permission: &Permission) -> Error {
    if permission == &customer::LIST || permission == &customer::DETAIL {
        Error::NotFound("客户不存在或无权查看".into())
    } else {
        Error::Forbidden("当前用户不在该客户的授权范围内".into())
    }
}

/// 拒绝超过查询上限的展开结果。
///
/// 列表与组织筛选共用同一上限（`scope` 复用本函数，避免各自硬编码 10000）。
///
/// # 参数
/// * `count` - 已展开数量
/// * `message` - 超限提示
///
/// # 返回
/// 未超限时返回 `Ok`。
///
/// # 错误
/// 超过 10000 时返回校验错误。
///
/// # 关键业务约束
/// 不得静默截断授权或筛选身份集合。
pub(super) fn ensure_limit(count: usize, message: &str) -> Result<()> {
    if count > QUERY_LIMIT {
        return Err(Error::ValidationError(format!("{message}，请收窄授权范围")));
    }
    Ok(())
}

/// 授权与筛选集合的查询上限。
pub(super) const QUERY_LIMIT: usize = 10_000;

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use erp_identity::entity::access_control::{ResolvedScope, ScopeClause};
    use erp_identity::testing::fixed_authorization;
    use mongodb::Client;
    use persistence_core::NoTransaction;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;

    fn actor() -> AuditActor {
        AuditActor::new("creator".into(), "creator-login".into(), AccountKind::Admin)
    }

    async fn access(permissions: &[Permission], scope: ResolvedScope) -> CustomerAccess {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
            .await
            .unwrap()
            .database("customer_creation_authorization");
        CustomerAccess::new(db, fixed_authorization(permissions, scope, Vec::new(), Vec::new()).unwrap())
    }

    fn scope(clause: ScopeClause) -> ResolvedScope {
        ResolvedScope { role_clauses: vec![clause], user_limit: None }
    }

    #[tokio::test]
    async fn creation_candidate_uses_the_creator_for_self_and_actual_organization_scope() {
        for clause in [
            ScopeClause { self_owned: true, ..Default::default() },
            ScopeClause { org_unit_ids: ["org-a".into()].into(), ..Default::default() },
        ] {
            let access = access(&[customer::CREATE], scope(clause)).await;
            for id in ["", "new-customer"] {
                access
                    .authorize_creation(
                        &actor(),
                        id,
                        Some("org-a".into()),
                        None,
                        &mut SnapshotExecutor::default(),
                    )
                    .await
                    .unwrap();
            }
        }
    }

    #[tokio::test]
    async fn creation_candidate_requires_both_permission_and_positive_role_scope() {
        for (permissions, resolved) in [
            (Vec::new(), scope(ScopeClause { company: true, ..Default::default() })),
            (vec![customer::CREATE], ResolvedScope { role_clauses: Vec::new(), user_limit: None }),
        ] {
            let access = access(&permissions, resolved).await;
            let error = access
                .authorize_creation(
                    &actor(),
                    "new-customer",
                    Some("org-a".into()),
                    None,
                    &mut SnapshotExecutor::default(),
                )
                .await
                .unwrap_err();
            assert!(matches!(error, Error::Forbidden(_)));
        }
    }

    #[tokio::test]
    async fn creation_candidate_rejects_uncovered_organization_and_user_limit() {
        let organization = ScopeClause { org_unit_ids: ["org-a".into()].into(), ..Default::default() };
        for resolved in [
            scope(organization.clone()),
            ResolvedScope {
                role_clauses: vec![ScopeClause { company: true, ..Default::default() }],
                user_limit: Some(organization),
            },
        ] {
            let access = access(&[customer::CREATE], resolved).await;
            let error = access
                .authorize_creation(
                    &actor(),
                    "new-customer",
                    Some("org-b".into()),
                    None,
                    &mut SnapshotExecutor::default(),
                )
                .await
                .unwrap_err();
            assert!(matches!(error, Error::Forbidden(_)));
        }
    }

    #[tokio::test]
    async fn creation_candidate_requires_owner_organization_current_facts_and_snapshot() {
        let access =
            access(&[customer::CREATE], scope(ScopeClause { company: true, ..Default::default() })).await;

        let missing = access
            .authorize_creation(&actor(), "new-customer", None, None, &mut SnapshotExecutor::default())
            .await
            .unwrap_err();
        assert!(
            matches!(missing, Error::ValidationError(message) if message == "请先维护负责人的有效主属组织")
        );

        let expired = access
            .authorize_creation(
                &actor(),
                "new-customer",
                Some("org-a".into()),
                Some(Instant::from_unix_secs(1)),
                &mut SnapshotExecutor::default(),
            )
            .await
            .unwrap_err();
        assert!(matches!(expired, Error::ConflictError(_)));

        let outside_snapshot = access
            .authorize_creation(&actor(), "new-customer", Some("org-a".into()), None, &mut NoTransaction)
            .await
            .unwrap_err();
        assert!(
            matches!(outside_snapshot, Error::ValidationError(message) if message == "写入授权必须使用快照事务")
        );
    }
}
