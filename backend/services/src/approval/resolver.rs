//! 编译期责任解析器；未知解析器和不完整组织范围一律失败关闭。

use std::collections::HashSet;

use database::{AccessControlExt, Executor, MongoCasbinAdapter};
use entities::{
    access_control::{DataScopeSubjectType, DataScopeType},
    AccountKind,
};
use mongodb::Database;

use crate::{errors::Result, iam};

use super::registry::{OPERATIONS_POOL_RESOLVER, SALES_MANAGER_RESOLVER};

const SALES_LEADER_ROLE: &str = "role-sales-leader";
const OPERATIONS_ROLE: &str = "role-operations";
const ROLE_PREFIX: &str = "role:";

/// 解析器成功形成的责任结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedAssignment {
    /// 必须与冻结步骤所需角色一致。
    pub owner_role: String,
    /// 必须可由角色或用户数据范围唯一证明。
    pub owner_organization_id: String,
    /// DIRECT 唯一用户；POOL 必须为空。
    pub owner_user_id: Option<String>,
}

/// 解析失败时写入审批实例与当前步骤的稳定阻塞码。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignmentBlocker {
    /// 解析器未在编译期注册。
    ResolverNotRegistered,
    /// 责任角色不存在或已停用。
    RoleUnavailable,
    /// 责任组织没有被当前数据范围明确覆盖。
    OrganizationScopeUnproven,
    /// DIRECT 步骤无法证明恰好一个有效候选人。
    DirectAssigneeNotUnique,
}

impl AssignmentBlocker {
    /// 返回可持久化且可安全展示的结构化阻塞码。
    pub fn code(self) -> &'static str {
        match self {
            Self::ResolverNotRegistered => "APPROVAL_RESOLVER_NOT_REGISTERED",
            Self::RoleUnavailable => "APPROVAL_OWNER_ROLE_UNAVAILABLE",
            Self::OrganizationScopeUnproven => "APPROVAL_ORGANIZATION_SCOPE_UNPROVEN",
            Self::DirectAssigneeNotUnique => "APPROVAL_DIRECT_ASSIGNEE_NOT_UNIQUE",
        }
    }
}

/// 编译期处理人解析注册表。
#[derive(Debug, Clone)]
pub struct ApprovalAssigneeResolver {
    db: Database,
}

impl ApprovalAssigneeResolver {
    /// 创建仅依赖当前数据库事实的解析器。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 在调用方事务内解析冻结步骤的责任人或责任池。
    ///
    /// DIRECT 只在角色、组织范围和唯一有效账号全部可证明时成功；POOL 只形成
    /// 角色与组织责任，不猜测个人。岗位分离仍由所属业务域强类型动作重验。
    ///
    /// # 错误
    /// 数据库读取失败返回服务错误；不能安全解析时返回结构化阻塞码。
    pub async fn resolve(
        &self,
        resolver_key: &str,
        owner_organization_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<std::result::Result<ResolvedAssignment, AssignmentBlocker>> {
        self.resolve_excluding(resolver_key, owner_organization_id, &[], executor)
            .await
    }

    /// 在解析 DIRECT 责任人前排除违反岗位分离的用户。
    ///
    /// 排除发生在唯一性判断之前；因此“提交人 + 唯一其他销售领导”会稳定指派给
    /// 其他销售领导，而不会因原始候选数量为二错误阻塞。POOL 仍只解析责任池，
    /// 形成个人责任后的操作人岗位分离由审批运行时提交决定时重验。
    ///
    /// # 错误
    /// 数据库读取失败返回服务错误；不能安全解析时返回结构化阻塞码。
    pub async fn resolve_excluding(
        &self,
        resolver_key: &str,
        owner_organization_id: &str,
        excluded_user_ids: &[&str],
        executor: &mut dyn Executor,
    ) -> Result<std::result::Result<ResolvedAssignment, AssignmentBlocker>> {
        match resolver_key {
            SALES_MANAGER_RESOLVER => {
                self.resolve_unique_user(
                    SALES_LEADER_ROLE,
                    owner_organization_id,
                    excluded_user_ids,
                    executor,
                )
                .await
            }
            OPERATIONS_POOL_RESOLVER => {
                self.resolve_pool(OPERATIONS_ROLE, owner_organization_id, executor)
                    .await
            }
            _ => Ok(Err(AssignmentBlocker::ResolverNotRegistered)),
        }
    }

    /// 校验用户当前是否有资格承接指定角色和组织的任务。
    ///
    /// 校验账号启用状态、角色启用状态、当前 Casbin 角色绑定，以及角色/用户数据
    /// 范围。用户没有单独范围时沿用角色范围；存在用户范围时必须显式覆盖组织。
    /// 业务对象参与权和岗位分离由对应强类型领域命令继续重验。
    ///
    /// # 错误
    /// 角色 ID 非法或底层仓储读取失败时返回服务错误。
    pub async fn user_is_eligible_for_assignment(
        &self,
        user_id: &str,
        role_id: &str,
        organization_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        if !self.role_is_enabled(role_id, executor).await?
            || !self
                .role_covers_organization(role_id, organization_id, executor)
                .await?
            || !self
                .user_covers_organization(user_id, organization_id, executor)
                .await?
        {
            return Ok(false);
        }
        entities::RoleId::parse(role_id)?;
        let Some(account) = self
            .db
            .accounts()
            .find_by_id(user_id, executor)
            .await?
            .filter(|account| account.is_kind(AccountKind::Admin) && account.can_login())
        else {
            return Ok(false);
        };
        Ok(self
            .active_role_ids(account.kind, user_id, executor)
            .await?
            .iter()
            .any(|active_role_id| active_role_id == role_id))
    }

    async fn resolve_unique_user(
        &self,
        role_id: &str,
        organization_id: &str,
        excluded_user_ids: &[&str],
        executor: &mut dyn Executor,
    ) -> Result<std::result::Result<ResolvedAssignment, AssignmentBlocker>> {
        if !self.role_is_enabled(role_id, executor).await? {
            return Ok(Err(AssignmentBlocker::RoleUnavailable));
        }
        if !self
            .role_covers_organization(role_id, organization_id, executor)
            .await?
        {
            return Ok(Err(AssignmentBlocker::OrganizationScopeUnproven));
        }
        let candidates = self
            .active_users_for_role(role_id, organization_id, executor)
            .await?;
        let candidates = eligible_candidates_excluding(candidates, excluded_user_ids);
        let [owner_user_id] = candidates.as_slice() else {
            return Ok(Err(AssignmentBlocker::DirectAssigneeNotUnique));
        };
        Ok(Ok(ResolvedAssignment {
            owner_role: role_id.to_string(),
            owner_organization_id: organization_id.to_string(),
            owner_user_id: Some(owner_user_id.clone()),
        }))
    }

    async fn resolve_pool(
        &self,
        role_id: &str,
        organization_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<std::result::Result<ResolvedAssignment, AssignmentBlocker>> {
        if !self.role_is_enabled(role_id, executor).await? {
            return Ok(Err(AssignmentBlocker::RoleUnavailable));
        }
        if !self
            .role_covers_organization(role_id, organization_id, executor)
            .await?
        {
            return Ok(Err(AssignmentBlocker::OrganizationScopeUnproven));
        }
        Ok(Ok(ResolvedAssignment {
            owner_role: role_id.to_string(),
            owner_organization_id: organization_id.to_string(),
            owner_user_id: None,
        }))
    }

    async fn role_is_enabled(&self, role_id: &str, executor: &mut dyn Executor) -> Result<bool> {
        let role_ids = vec![role_id.to_string()];
        Ok(self.db.roles().enabled_roles(&role_ids, executor).await?.len() == 1)
    }

    async fn role_covers_organization(
        &self,
        role_id: &str,
        organization_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let scopes = self
            .db
            .data_scopes()
            .list_by_subject(DataScopeSubjectType::Role, role_id, executor)
            .await?;
        Ok(scopes.iter().any(|scope| match scope.scope_type {
            DataScopeType::Company => true,
            DataScopeType::Organization | DataScopeType::Team => {
                scope.scope_targets.iter().any(|target| target == organization_id)
            }
            DataScopeType::SelfOwned | DataScopeType::Collaborative => false,
        }))
    }

    async fn active_users_for_role(
        &self,
        role_id: &str,
        organization_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        entities::RoleId::parse(role_id)?;
        let role_key = format!("{ROLE_PREFIX}{role_id}");
        let subjects = MongoCasbinAdapter::new(self.db.clone())
            .role_subjects(&role_key, executor)
            .await?;
        let mut users = Vec::new();
        for user_id in admin_user_ids(subjects) {
            if self
                .user_scope_and_account_are_eligible(&user_id, organization_id, executor)
                .await?
            {
                users.push(user_id);
            }
        }
        users.sort();
        users.dedup();
        Ok(users)
    }

    async fn active_role_ids(
        &self,
        account_kind: AccountKind,
        user_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        let subject = iam::subject(account_kind, user_id);
        let role_ids = casbin_role_ids(
            MongoCasbinAdapter::new(self.db.clone())
                .subject_roles(&subject, executor)
                .await?,
        );
        if role_ids.is_empty() {
            return Ok(role_ids);
        }
        let enabled = self
            .db
            .roles()
            .enabled_roles(&role_ids, executor)
            .await?
            .into_iter()
            .map(|role| role.base.id)
            .collect::<HashSet<_>>();
        Ok(role_ids
            .into_iter()
            .filter(|role_id| enabled.contains(role_id))
            .collect())
    }

    async fn user_scope_and_account_are_eligible(
        &self,
        user_id: &str,
        organization_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        if !self
            .user_covers_organization(user_id, organization_id, executor)
            .await?
        {
            return Ok(false);
        }
        Ok(self
            .db
            .accounts()
            .find_by_id(user_id, executor)
            .await?
            .is_some_and(|account| account.is_kind(AccountKind::Admin) && account.can_login()))
    }

    async fn user_covers_organization(
        &self,
        user_id: &str,
        organization_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let scopes = self
            .db
            .data_scopes()
            .list_by_subject(DataScopeSubjectType::User, user_id, executor)
            .await?;
        if scopes.is_empty() {
            return Ok(true);
        }
        Ok(scopes.iter().any(|scope| match scope.scope_type {
            DataScopeType::Company => true,
            DataScopeType::Organization | DataScopeType::Team => {
                scope.scope_targets.iter().any(|target| target == organization_id)
            }
            DataScopeType::SelfOwned | DataScopeType::Collaborative => false,
        }))
    }
}

fn casbin_role_ids(role_keys: Vec<String>) -> Vec<String> {
    let mut role_ids = role_keys
        .into_iter()
        .filter_map(|role_key| role_key.strip_prefix(ROLE_PREFIX).map(str::to_string))
        .collect::<Vec<_>>();
    role_ids.sort();
    role_ids.dedup();
    role_ids
}

fn admin_user_ids(subjects: Vec<String>) -> Vec<String> {
    let prefix = iam::subject(AccountKind::Admin, "");
    let mut user_ids = subjects
        .into_iter()
        .filter_map(|subject| subject.strip_prefix(&prefix).map(str::to_string))
        .filter(|user_id| !user_id.is_empty())
        .collect::<Vec<_>>();
    user_ids.sort();
    user_ids.dedup();
    user_ids
}

fn eligible_candidates_excluding(mut candidates: Vec<String>, excluded_user_ids: &[&str]) -> Vec<String> {
    candidates.retain(|candidate| !excluded_user_ids.iter().any(|excluded| candidate == excluded));
    candidates
}

#[cfg(test)]
mod tests {
    use super::{admin_user_ids, casbin_role_ids, eligible_candidates_excluding, AssignmentBlocker};

    #[test]
    fn blocker_codes_are_structured_and_stable() {
        assert_eq!(
            AssignmentBlocker::DirectAssigneeNotUnique.code(),
            "APPROVAL_DIRECT_ASSIGNEE_NOT_UNIQUE"
        );
        assert_eq!(
            AssignmentBlocker::ResolverNotRegistered.code(),
            "APPROVAL_RESOLVER_NOT_REGISTERED"
        );
    }

    #[test]
    fn separation_exclusion_runs_before_direct_uniqueness() {
        let candidates = vec!["submitter".to_string(), "leader".to_string()];
        let filtered = eligible_candidates_excluding(candidates, &["submitter"]);
        assert_eq!(filtered, vec!["leader".to_string()]);
    }

    #[test]
    fn casbin_role_bindings_form_active_role_ids() {
        let role_ids = casbin_role_ids(vec![
            "role:role-sales-leader".to_string(),
            "not-a-role".to_string(),
            "role:role-operations".to_string(),
            "role:role-sales-leader".to_string(),
        ]);

        assert_eq!(role_ids, ["role-operations", "role-sales-leader"]);
    }

    #[test]
    fn direct_candidates_come_only_from_admin_casbin_subjects() {
        let user_ids = admin_user_ids(vec![
            "user:admin:leader-2".to_string(),
            "role:role-sales-leader".to_string(),
            "user:admin:".to_string(),
            "user:admin:leader-1".to_string(),
            "user:admin:leader-2".to_string(),
        ]);

        assert_eq!(user_ids, ["leader-1", "leader-2"]);
    }
}
