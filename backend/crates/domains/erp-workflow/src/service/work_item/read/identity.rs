//! 任务授权直接使用 identity 的公开范围解析与 RBAC 服务。
use std::collections::BTreeSet;

use application_core::AuditActor;
use erp_identity::access_control::ScopedObject;
use erp_identity::service::access_control::resolve::{AuthorizedDataScope, DataScopeService};
use persistence_core::Executor;

use super::WorkItemReadService;
use crate::{Error, Result};

impl WorkItemReadService {
    /// 解析任务读取或管理动作的负责人范围。
    /// # 参数
    /// `actor` 为真实账号，`action` 为 list/detail/manage，`executor` 为原事务。
    /// # 返回
    /// 公司范围返回 None；空集合不授予读取资格。
    /// # 错误
    /// 非法动作、身份失效、权限拒绝或范围过大时返回错误。
    pub async fn task_scope_owners(
        &self,
        actor: &AuditActor,
        action: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<Vec<String>>> {
        if !matches!(action, "list" | "detail" | "manage") {
            return Err(Error::ValidationError("未登记的任务权限动作".into()));
        }
        let scope = DataScopeService::new(self.db.clone(), self.rbac.clone())
            .resolve(actor, "work_item", action, executor)
            .await?;
        compile_task_owners(&scope)
    }
    /// 读取当前身份授权版本，用于分页和首末复验。
    /// # 参数
    /// `actor` 为当前账号，`executor` 沿用调用事务。
    /// # 返回
    /// 返回 identity 生成的授权版本。
    /// # 错误
    /// 身份失效或读取失败时传播错误。
    pub async fn queue_scope_version(
        &self,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<String> {
        Ok(DataScopeService::new(self.db.clone(), self.rbac.clone())
            .authorization_version(actor, executor)
            .await?)
    }
}
/// 将 identity 已授权范围转换为当前有效的任务负责人集合。
/// # 参数
/// `scope` 为当前事务解析的身份授权范围。
/// # 返回
/// 公司范围返回 None，否则返回去重的具体负责人。
/// # 错误
/// 负责人超过 20000 人时拒绝。
pub fn compile_task_owners(scope: &AuthorizedDataScope) -> Result<Option<Vec<String>>> {
    let owners = owners(scope);
    if owners.as_ref().is_some_and(|ids| ids.len() > 20_000) {
        return Err(Error::ValidationError("任务管理范围超过 20000 人，请缩小配置范围".into()));
    }
    Ok(owners)
}

/// 按公共授权范围与当前有效内部组织关系编译具体任务负责人。
fn owners(context: &AuthorizedDataScope) -> Option<Vec<String>> {
    let object = |user: &str, org: Option<&str>| {
        context.scope.allows(
            &ScopedObject {
                owned: user == context.user_id,
                collaborating: false,
                historical_read_participant: false,
                org_unit_id: org,
                settlement_party_id: None,
                warehouse_id: None,
            },
            false,
        )
    };
    // 无归属、非本人仍获准，只可能由角色 Company 与个人上限共同证明。
    if object("", None) {
        return None;
    }
    let mut users = BTreeSet::new();
    if object(&context.user_id, None) {
        users.insert(context.user_id.clone());
    }
    for membership in &context.organizations.memberships {
        if !membership.base.is_deleted()
            && membership.validity.contains(context.as_of)
            && object(&membership.user_id, Some(&membership.org_unit_id))
        {
            users.insert(membership.user_id.clone());
        }
    }
    Some(users.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use entity_core::BaseModel;
    use erp_core::common::time::Instant;
    use erp_identity::access_control::{ResolvedScope, ScopeClause};
    use erp_identity::entity::organization::{OrgMembership, OrgValidity};

    use super::*;

    fn context() -> AuthorizedDataScope {
        let membership = |id: &str, user: &str, org: &str, end: Option<i64>| OrgMembership {
            base: BaseModel::new(id.into()),
            user_id: user.into(),
            org_unit_id: org.into(),
            validity: OrgValidity {
                valid_from: Instant::from_unix_secs(1),
                valid_to: end.map(Instant::from_unix_secs),
            },
            changed_by: "admin".into(),
            reason: "fixture".into(),
        };
        let mut context = AuthorizedDataScope {
            user_id: "manager".into(),
            resource: "work_item".into(),
            action: "manage".into(),
            role_scopes: Default::default(),
            scope: ResolvedScope {
                role_clauses: vec![ScopeClause {
                    org_unit_ids: BTreeSet::from(["dept-a".into()]),
                    ..Default::default()
                }],
                user_limit: None,
            },
            organizations: Default::default(),
            policy_version: 1,
            scope_version: "v1".into(),
            as_of: Instant::from_unix_secs(10),
        };
        context.organizations.memberships = vec![
            membership("a", "worker-a", "dept-a", None),
            membership("b", "worker-b", "dept-b", None),
            membership("old", "expired", "dept-a", Some(10)),
        ];
        context
    }

    #[test]
    fn task_manager_uses_current_membership_not_responsibility_organization() {
        let mut scope = context();
        assert_eq!(owners(&scope), Some(vec!["worker-a".into()]));
        scope.scope.user_limit = Some(ScopeClause { self_owned: true, ..Default::default() });
        assert_eq!(owners(&scope), Some(vec![]));
        scope.scope.role_clauses = vec![ScopeClause { company: true, ..Default::default() }];
        assert_eq!(owners(&scope), Some(vec!["manager".into()]));
        scope.scope.user_limit = None;
        assert_eq!(owners(&scope), None);
        scope.scope.role_clauses.clear();
        assert_eq!(owners(&scope), Some(vec![]));
    }

    /// 管理范围上限在批量和原读取路径共用，边界人数允许且超限保持原验证错误。
    #[test]
    fn managed_owner_limit_preserves_exact_boundary() {
        let mut scope = context();
        let template = scope.organizations.memberships[0].clone();
        scope.organizations.memberships = (0..20_000)
            .map(|index| {
                let mut membership = template.clone();
                membership.user_id = format!("worker-{index}");
                membership
            })
            .collect();
        assert_eq!(compile_task_owners(&scope).unwrap().unwrap().len(), 20_000);
        let mut extra = template;
        extra.user_id = "extra-worker".into();
        scope.organizations.memberships.push(extra);
        assert!(
            matches!(compile_task_owners(&scope), Err(Error::ValidationError(message)) if message == "任务管理范围超过 20000 人，请缩小配置范围")
        );
    }
}
