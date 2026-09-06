//! 责任队列授权快照、范围过滤与允许动作。

use std::collections::{HashMap, HashSet};

use database::{AccessControlExt, DocumentRegistryExt, MongoCasbinAdapter};
use entities::{
    access_control::{DataScope, DataScopeSubjectType, OrganizationCoverage, ResponsibilityScopeSet},
    work_item::{AvailableWorkItemAccount, WorkItem, WorkItemStatus, WorkItemType},
    Permission, PermissionSet,
};
use mongodb::Database;
use persistence_core::{Executor, NoTransaction};

use crate::errors::{Error, ErrorCode, Result};
use application_core::AuditActor;

use super::close::is_w29_fields_closable;
use super::dto;
use super::facts::{apply_object_display, object_policy, ObjectFact, ObjectFactMap};
use super::{
    ProcessingBlockerView, ProcessingState, WorkItemAllowedAction, WorkItemFilter, WorkItemScope,
    WorkItemService,
};

pub(super) const MANAGE_PERMISSION: &str = "work_item:manage";
pub(super) const REASSIGN_PERMISSION: &str = "work_item:reassign";
pub(super) const CLOSE_PERMISSION: &str = "work_item:close";

pub(super) struct ActorAccess {
    pub(super) actor_id: String,
    pub(super) permissions: Vec<Permission>,
    pub(super) participant_document_ids: HashSet<String>,
    pub(super) organization_ids: Vec<String>,
    pub(super) responsibility_scopes: Vec<(String, Option<String>)>,
    pub(super) can_manage: bool,
}

impl WorkItemService {
    /// 写命令执行前重验对象存在、阅读权限和参与依据。
    pub(super) async fn ensure_object_participation(
        &self,
        actor: &AuditActor,
        item: &WorkItem,
    ) -> Result<()> {
        let access = self.actor_access(actor).await?;
        self.ensure_item_access(item, &access)
            .await
            .map_err(|_| Error::Forbidden("当前账号无权处理该业务对象".to_string()))
    }

    pub(super) async fn ensure_item_access(&self, item: &WorkItem, access: &ActorAccess) -> Result<()> {
        if self
            .authorized_fields_for_items(vec![item.clone()], access)
            .await?
            .is_empty()
        {
            return Err(Error::Forbidden("业务对象不可访问".to_string()));
        }
        Ok(())
    }

    pub(super) async fn actor_access(&self, actor: &AuditActor) -> Result<ActorAccess> {
        self.actor_access_for(actor.kind(), actor.id()).await
    }

    async fn actor_access_for(
        &self,
        account_kind: erp_core::AccountKind,
        actor_id: &str,
    ) -> Result<ActorAccess> {
        let role_ids = self.rbac.role_ids(account_kind, actor_id).await?;
        let permissions = self.rbac.permissions(account_kind, actor_id).await?;
        let participant_document_ids = self
            .db
            .document_participants()
            .document_ids_by_user(actor_id, &mut NoTransaction)
            .await?
            .into_iter()
            .collect();
        let manage_permission = Permission::parse(MANAGE_PERMISSION).expect("固定权限合法");
        let has_manage_permission = permissions
            .iter()
            .any(|permission| permission.covers(&manage_permission));
        let manage_role_ids = self
            .roles_granting_permission(&role_ids, &manage_permission, has_manage_permission)
            .await?;
        let (organization_ids, responsibility_scopes) = self
            .actor_scope_access(actor_id, &role_ids, &manage_role_ids)
            .await?;
        Ok(ActorAccess {
            actor_id: actor_id.to_string(),
            permissions,
            participant_document_ids,
            can_manage: !manage_role_ids.is_empty(),
            organization_ids,
            responsibility_scopes,
        })
    }

    /// 定位实际授予指定权限的角色，使管理数据范围与权限来源关联。
    pub(super) async fn roles_granting_permission(
        &self,
        role_ids: &[String],
        permission: &Permission,
        account_has_permission: bool,
    ) -> Result<Vec<String>> {
        if !account_has_permission {
            return Ok(Vec::new());
        }
        let mut granting_roles = Vec::new();
        for role_id in role_ids {
            if self.rbac.enforce(&format!("role:{role_id}"), permission).await? {
                granting_roles.push(role_id.clone());
            }
        }
        Ok(granting_roles)
    }

    /// 分别保留每个角色的组织授权，避免多角色范围交叉放大。
    async fn actor_scope_access(
        &self,
        actor_id: &str,
        role_ids: &[String],
        manage_role_ids: &[String],
    ) -> Result<(Vec<String>, Vec<(String, Option<String>)>)> {
        let user_subject_ids = vec![actor_id.to_string()];
        let user_scopes = self
            .db
            .data_scopes()
            .list_by_subjects(DataScopeSubjectType::User, &user_subject_ids, &mut NoTransaction)
            .await?;
        let role_scopes = self
            .db
            .data_scopes()
            .list_by_subjects(DataScopeSubjectType::Role, role_ids, &mut NoTransaction)
            .await?
            .into_iter()
            .fold(HashMap::<String, Vec<DataScope>>::new(), |mut grouped, scope| {
                grouped.entry(scope.subject_id.clone()).or_default().push(scope);
                grouped
            });
        let mut responsibility_scopes = Vec::new();
        let mut management_scopes = Vec::new();
        for role_id in role_ids {
            let role_scopes = role_scopes.get(role_id).map(Vec::as_slice).unwrap_or_default();
            let pairs = responsibility_scope_for_role(role_id, role_scopes, &user_scopes);
            responsibility_scopes.extend(pairs.iter().cloned());
            if manage_role_ids.contains(role_id) {
                management_scopes.extend(pairs);
            }
        }
        responsibility_scopes.sort();
        responsibility_scopes.dedup();
        Ok((
            organizations_from_pairs(&management_scopes),
            responsibility_scopes,
        ))
    }

    pub(super) fn scope_filter(
        &self,
        query: &dto::WorkItemListQuery,
        actor: &AuditActor,
        access: &ActorAccess,
    ) -> Result<WorkItemFilter> {
        let mut filter = WorkItemFilter {
            work_item_types: query.work_item_types.clone(),
            statuses: query.statuses.clone(),
            priorities: query.priorities.clone(),
            query: query.query.clone(),
            object_access_shapes: Some(object_access_shapes(access)),
            page: query.page,
            page_size: query.page_size,
            sort_by: Some(query.sort_by.to_string()),
            sort_ascending: query.sort_ascending,
            ..WorkItemFilter::default()
        };
        match query.scope {
            WorkItemScope::Mine => filter.owner_user_id = Some(actor.id().to_string()),
            WorkItemScope::Managed => {
                ensure_managed_access(access)?;
                filter.owner_organization_ids = organization_filter(access);
            }
            WorkItemScope::History => {
                filter.history_actor_id = Some(actor.id().to_string());
                if access.can_manage && !access.organization_ids.is_empty() {
                    filter.history_managed_organization_ids = Some(organization_filter(access));
                }
            }
        }
        Ok(filter)
    }

    pub(super) async fn view_access(
        &self,
        item: &dto::WorkItemFields,
        scope: WorkItemScope,
        actor: &AuditActor,
        access: &ActorAccess,
    ) -> Result<ViewAccess> {
        if let Some(blocker) = self.processing_blocker(None::<&str>).await? {
            return Ok(ViewAccess::blocked(blocker));
        }
        if scope == WorkItemScope::History || item.status != WorkItemStatus::Open {
            return Ok(ViewAccess::ready(Vec::new()));
        }
        let actions = allowed_actions(item, scope, actor.id(), access);
        Ok(ViewAccess::ready(actions))
    }

    async fn processing_blocker(&self, _step_id: Option<&str>) -> Result<Option<ProcessingBlockerView>> {
        Ok(None)
    }

    /// 事务内按 MongoDB 权威角色、范围、对象和审批事实构造访问快照。
    pub(super) async fn assignment_access_for_executor(
        &self,
        account_kind: erp_core::AccountKind,
        actor_id: &str,
        read_permission: &Permission,
        read_role_ids: &[String],
        manage_role_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<ActorAccess> {
        let role_ids = active_role_ids(&self.db, account_kind, actor_id, executor).await?;
        let active_read_roles = intersect_role_ids(&role_ids, read_role_ids);
        let active_manage_roles = intersect_role_ids(&role_ids, manage_role_ids);
        let mut permissions = Vec::with_capacity(2);
        if !active_read_roles.is_empty() {
            permissions.push(read_permission.clone());
        }
        if !active_manage_roles.is_empty() {
            permissions.push(Permission::parse(MANAGE_PERMISSION).expect("固定权限合法"));
        }
        let participant_document_ids = self
            .db
            .document_participants()
            .document_ids_by_user(actor_id, executor)
            .await?
            .into_iter()
            .collect();
        let (organization_ids, responsibility_scopes) = self
            .scope_access_for_executor(actor_id, &role_ids, &active_manage_roles, executor)
            .await?;
        Ok(ActorAccess {
            actor_id: actor_id.to_string(),
            permissions,
            participant_document_ids,
            can_manage: !active_manage_roles.is_empty(),
            organization_ids,
            responsibility_scopes,
        })
    }

    /// 分别保留事务内每个角色的数据范围，禁止角色与用户范围交叉放大。
    async fn scope_access_for_executor(
        &self,
        actor_id: &str,
        role_ids: &[String],
        manage_role_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<(Vec<String>, Vec<(String, Option<String>)>)> {
        let user_subject_ids = vec![actor_id.to_string()];
        let user_scopes = self
            .db
            .data_scopes()
            .list_by_subjects(DataScopeSubjectType::User, &user_subject_ids, executor)
            .await?;
        let role_scopes = self
            .db
            .data_scopes()
            .list_by_subjects(DataScopeSubjectType::Role, role_ids, executor)
            .await?
            .into_iter()
            .fold(HashMap::<String, Vec<DataScope>>::new(), |mut grouped, scope| {
                grouped.entry(scope.subject_id.clone()).or_default().push(scope);
                grouped
            });
        let mut responsibility_scopes = Vec::new();
        let mut management_scopes = Vec::new();
        for role_id in role_ids {
            let role_scopes = role_scopes.get(role_id).map(Vec::as_slice).unwrap_or_default();
            let pairs = responsibility_scope_for_role(role_id, role_scopes, &user_scopes);
            responsibility_scopes.extend(pairs.iter().cloned());
            if manage_role_ids.contains(role_id) {
                management_scopes.extend(pairs);
            }
        }
        responsibility_scopes.sort();
        responsibility_scopes.dedup();
        Ok((
            organizations_from_pairs(&management_scopes),
            responsibility_scopes,
        ))
    }

    /// 使用调用方 executor 读取固定注册表对象事实并重验参与权。
    pub(super) async fn ensure_item_access_with_executor(
        &self,
        item: &WorkItem,
        access: &ActorAccess,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let policy = object_policy(item.work_item_type, &item.business_object_type)
            .ok_or_else(|| Error::Forbidden("任务类型未注册责任策略".to_string()))?;
        let keys = HashSet::from([(policy.object_kind, item.business_object_id.clone())]);
        let facts = self.load_object_facts(&keys, executor).await?;
        if authorized_item_fields(item.clone(), access, &facts).is_none() {
            return Err(Error::Forbidden("业务对象不可访问".to_string()));
        }
        Ok(())
    }

    /// 在领域决定事务内按当前账号、有效角色、读取权限、数据范围和对象参与事实重验访问。
    pub(crate) async fn ensure_domain_decision_access(
        &self,
        actor: &AuditActor,
        item: &WorkItem,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let account = self
            .db
            .accounts()
            .find_work_item_account(actor.id(), executor)
            .await?
            .ok_or_else(|| Error::Forbidden("操作账号不存在、已停用或身份已变化".to_string()))?;
        AvailableWorkItemAccount::from_account_kind(&account, actor.kind())
            .map_err(|_| Error::Forbidden("操作账号不存在、已停用或身份已变化".to_string()))?;
        let policy = object_policy(item.work_item_type, &item.business_object_type)
            .ok_or_else(|| Error::Forbidden("任务类型未注册责任策略".to_string()))?;
        let read_permission = Permission::parse(policy.read_permission).expect("责任策略权限必须合法");
        let policy_revision = MongoCasbinAdapter::new(self.db.clone())
            .policy_revision(executor)
            .await?;
        let role_ids = active_role_ids(&self.db, actor.kind(), actor.id(), executor).await?;
        let execution_permissions =
            required_execution_permissions(item.work_item_type, &item.business_object_type)
                .ok_or_else(|| Error::Forbidden("任务类型未注册完整执行权限".to_string()))?;
        for permission in execution_permissions.as_slice() {
            let granting_roles = self
                .roles_granting_permission(&role_ids, permission, true)
                .await?;
            if granting_roles.is_empty() {
                return Err(Error::Forbidden(
                    "当前账号已不具备任务所需的完整执行权限".to_string(),
                ));
            }
        }
        let read_role_ids = self
            .roles_granting_permission(&role_ids, &read_permission, true)
            .await?;
        if read_role_ids.is_empty() {
            return Err(Error::Forbidden(
                "当前账号已不具备任务业务对象读取权限".to_string(),
            ));
        }
        let access = self
            .assignment_access_for_executor(
                actor.kind(),
                actor.id(),
                &read_permission,
                &read_role_ids,
                &[],
                executor,
            )
            .await?;
        self.ensure_item_access_with_executor(item, &access, executor)
            .await
            .map_err(|_| Error::Forbidden("当前账号不具备任务业务对象的参与权或读取权".to_string()))?;
        ensure_policy_revision(&self.db, policy_revision, executor).await
    }

    pub(super) async fn managed_access(&self, actor: &AuditActor) -> Result<ActorAccess> {
        let access = self.actor_access(actor).await?;
        if access.can_manage && !access.organization_ids.is_empty() {
            return Ok(access);
        }
        Err(Error::Forbidden("当前账号没有任务责任管理权限".to_string()))
    }
}

/// 事务内校验 Casbin 持久化策略仍与事务外稳定授权快照一致。
pub(super) async fn ensure_policy_revision(
    db: &Database,
    expected_revision: u64,
    executor: &mut dyn Executor,
) -> Result<()> {
    let actual = MongoCasbinAdapter::new(db.clone())
        .policy_revision(executor)
        .await?;
    if actual != expected_revision {
        return Err(Error::Forbidden("任务分派期间授权策略已变化，请重试".to_string()));
    }
    Ok(())
}

/// 从事务内 Casbin `g` 授权事实与启用角色形成角色集合。
pub(super) async fn active_role_ids(
    db: &Database,
    account_kind: erp_core::AccountKind,
    account_id: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<String>> {
    let subject = crate::iam::subject(account_kind, account_id);
    let mut role_ids = MongoCasbinAdapter::new(db.clone())
        .subject_roles(&subject, executor)
        .await?
        .into_iter()
        .filter_map(|role_key| role_key.strip_prefix("role:").map(str::to_string))
        .collect::<Vec<_>>();
    role_ids.sort();
    role_ids.dedup();
    if role_ids.is_empty() {
        return Ok(role_ids);
    }
    let enabled = db
        .roles()
        .enabled_roles(&role_ids, executor)
        .await?
        .into_iter()
        .map(|role| role.base.id)
        .collect::<HashSet<_>>();
    role_ids.retain(|role_id| enabled.contains(role_id));
    Ok(role_ids)
}

pub(super) fn intersect_role_ids(active: &[String], authorized: &[String]) -> Vec<String> {
    active
        .iter()
        .filter(|role_id| authorized.contains(role_id))
        .cloned()
        .collect()
}

/// 根据实体注册关系和当前权限形成仓储候选对象形状。
///
/// # 参数
/// * `access` - 当前账号的权限与参与范围事实
///
/// # 返回
/// 返回当前账号具备读取权限的工作项类型和业务对象类型组合。
///
/// # 错误
/// 无；注册关系中的固定权限必须可解析。
pub(super) fn object_access_shapes(access: &ActorAccess) -> Vec<(WorkItemType, String)> {
    WorkItemType::registered_brief_relations()
        .iter()
        .filter(|policy| has_permission(access, policy.read_permission))
        .map(|policy| (policy.work_item_type, policy.business_object_type.to_string()))
        .collect()
}

/// 判断当前访问快照是否覆盖实体关系要求的读取权限。
///
/// # 参数
/// * `access` - 当前账号权限快照
/// * `permission` - 实体关系注册的固定权限代码
///
/// # 返回
/// 任一已授予权限覆盖要求时返回 `true`。
///
/// # 错误
/// 无；固定权限代码无效属于程序错误并触发断言。
pub(super) fn has_permission(access: &ActorAccess, permission: &str) -> bool {
    let required = Permission::parse(permission).expect("对象注册表权限必须合法");
    PermissionSet::new(access.permissions.clone()).covers_one(&required)
}

/// 按对象权限、参与关系与权威版本过滤工作项列表投影。
///
/// # 参数
/// * `rows` - 仓储返回的候选工作项行
/// * `access` - 当前账号访问事实
/// * `facts` - 已批量加载的业务对象事实
///
/// # 返回
/// 返回当前账号可见且已补齐对象展示字段的工作项。
///
/// # 错误
/// 无；未注册或无法证明访问权的候选项会失败关闭并被过滤。
pub(super) fn authorized_fields(
    rows: Vec<database::WorkItemRow>,
    access: &ActorAccess,
    facts: &ObjectFactMap,
) -> Vec<dto::WorkItemFields> {
    rows.into_iter()
        .filter_map(|row| {
            let policy = object_policy(row.work_item_type, &row.business_object_type)?;
            let fact = facts.get(&(policy.object_kind, row.business_object_id.clone()))?;
            if !has_permission(access, policy.read_permission)
                || !has_item_participation(
                    row.work_item_type,
                    row.owner_user_id.as_deref(),
                    &row.owner_role,
                    &row.owner_organization_id,
                    access,
                    fact,
                )
                || !fact.subject_versions.accepts(&row.subject_version)
            {
                return None;
            }
            let mut fields = dto::WorkItemFields::from(row);
            apply_object_display(&mut fields, fact);
            Some(fields)
        })
        .collect()
}

/// 按对象权限、参与关系与权威版本形成单个工作项投影。
///
/// # 参数
/// * `item` - 待授权工作项实体
/// * `access` - 当前账号访问事实
/// * `facts` - 已加载的业务对象事实
///
/// # 返回
/// 授权通过时返回补齐对象展示字段的投影，否则返回 `None`。
///
/// # 错误
/// 无；未注册或无法证明访问权时失败关闭。
pub(super) fn authorized_item_fields(
    item: WorkItem,
    access: &ActorAccess,
    facts: &ObjectFactMap,
) -> Option<dto::WorkItemFields> {
    let policy = object_policy(item.work_item_type, &item.business_object_type)?;
    let fact = facts.get(&(policy.object_kind, item.business_object_id.clone()))?;
    if !has_permission(access, policy.read_permission)
        || !has_item_participation(
            item.work_item_type,
            item.owner_user_id.as_deref(),
            &item.owner_role,
            &item.owner_organization_id,
            access,
            fact,
        )
        || !fact.subject_versions.accepts(&item.subject_version)
    {
        return None;
    }
    let mut fields = dto::WorkItemFields::from(item);
    apply_object_display(&mut fields, fact);
    Some(fields)
}

/// 判断转交目标是否满足任务类型要求的权限、参与关系和对象版本。
///
/// # 参数
/// * `item` - 待转交任务
/// * `access` - 目标账号访问事实
/// * `facts` - 已加载的业务对象事实
///
/// # 返回
/// 目标账号可接收任务时返回 `true`。
///
/// # 错误
/// 无；对象未注册或事实缺失时返回 `false`。
pub(super) fn has_assignment_candidate_access(
    item: &WorkItem,
    access: &ActorAccess,
    facts: &ObjectFactMap,
) -> bool {
    let Some(policy) = object_policy(item.work_item_type, &item.business_object_type) else {
        return false;
    };
    let Some(fact) = facts.get(&(policy.object_kind, item.business_object_id.clone())) else {
        return false;
    };
    has_permission(access, policy.read_permission)
        && has_execution_permissions(item.work_item_type, &item.business_object_type, access)
        && (item.work_item_type.uses_explicit_owner_authorization()
            || has_object_participation(access, &item.owner_role, &item.owner_organization_id, fact))
        && fact.subject_versions.accepts(&item.subject_version)
}

/// 返回执行任务的完整权限；普通任务返回空集，未注册执行对象失败关闭。
pub(super) fn required_execution_permissions(
    work_item_type: WorkItemType,
    business_object_type: &str,
) -> Option<PermissionSet> {
    if work_item_type == WorkItemType::BusinessException && business_object_type == "SUPPLIER_OFFERING" {
        return Some(PermissionSet::new([Permission::parse(
            "supplier_offering:resolve_supply_exception",
        )
        .expect("业务异常固定权限必须合法")]));
    }
    work_item_type.required_execution_permissions(business_object_type)
}

/// 判断账号是否覆盖执行任务在目标工作面所需的全部权限。
pub(super) fn has_execution_permissions(
    work_item_type: WorkItemType,
    business_object_type: &str,
    access: &ActorAccess,
) -> bool {
    required_execution_permissions(work_item_type, business_object_type)
        .is_some_and(|required| PermissionSet::new(access.permissions.clone()).covers(&required))
}

/// 判断账号是否满足工作项的参与条件。
///
/// # 参数
/// * `work_item_type` - 工作项类型
/// * `owner_user_id` - 当前具体负责人
/// * `owner_role` - 责任角色标识
/// * `owner_organization_id` - 责任组织 ID
/// * `access` - 当前账号访问事实
/// * `fact` - 业务对象事实
///
/// # 返回
/// 具备对象参与关系，或是供给分配任务的具体负责人时返回 `true`。
///
/// # 错误
/// 无；调用方必须另行验证对象权限。
pub(super) fn has_item_participation(
    work_item_type: WorkItemType,
    owner_user_id: Option<&str>,
    owner_role: &str,
    owner_organization_id: &str,
    access: &ActorAccess,
    fact: &ObjectFact,
) -> bool {
    let is_explicit_owner =
        work_item_type.uses_explicit_owner_authorization() && owner_user_id == Some(access.actor_id.as_str());
    is_explicit_owner || has_object_participation(access, owner_role, owner_organization_id, fact)
}

pub(super) fn has_object_participation(
    access: &ActorAccess,
    owner_role: &str,
    owner_organization_id: &str,
    fact: &ObjectFact,
) -> bool {
    fact.created_by == access.actor_id
        || access.participant_document_ids.contains(&fact.root_document_id)
        || covers_responsibility(access, owner_role, owner_organization_id)
        || (access.can_manage && covers_organization(access, owner_organization_id))
}

pub(super) struct ViewAccess {
    pub(super) processing_state: ProcessingState,
    pub(super) processing_blocker: Option<ProcessingBlockerView>,
    pub(super) allowed_actions: Vec<WorkItemAllowedAction>,
    pub(super) action_blockers: Vec<ProcessingBlockerView>,
}

impl ViewAccess {
    pub(super) fn ready(allowed_actions: Vec<WorkItemAllowedAction>) -> Self {
        Self {
            processing_state: ProcessingState::Ready,
            processing_blocker: None,
            allowed_actions,
            action_blockers: Vec::new(),
        }
    }

    pub(super) fn blocked(blocker: ProcessingBlockerView) -> Self {
        Self {
            processing_state: ProcessingState::ApprovalBlocked,
            processing_blocker: Some(blocker.clone()),
            allowed_actions: Vec::new(),
            action_blockers: vec![blocker],
        }
    }
}

/// 计算当前账号在指定队列范围内可执行的工作项动作。
///
/// # 参数
/// * `item` - 已完成对象授权的工作项投影
/// * `scope` - 当前队列范围
/// * `actor_id` - 当前账号 ID
/// * `access` - 当前账号访问事实
///
/// # 返回
/// 返回查看、处理、审批或管理动作集合。
///
/// # 错误
/// 无；未满足责任或管理条件的动作不会出现在结果中。
pub(super) fn allowed_actions(
    item: &dto::WorkItemFields,
    scope: WorkItemScope,
    actor_id: &str,
    access: &ActorAccess,
) -> Vec<WorkItemAllowedAction> {
    let mut actions = vec![WorkItemAllowedAction::View];
    let is_explicit_owner = item.work_item_type.uses_explicit_owner_authorization()
        && item.owner_user_id.as_deref() == Some(actor_id);
    if item.owner_user_id.as_deref() == Some(actor_id)
        && has_execution_permissions(item.work_item_type, &item.business_object_type, access)
        && (is_explicit_owner
            || covers_responsibility(access, &item.owner_role, &item.owner_organization_id)
            || item.status != WorkItemStatus::Open)
    {
        actions.push(WorkItemAllowedAction::Process);
    }
    // 开放的单据审批任务由审批运行时直接指派给责任人（owner_user_id = 指派
    // 人，owner_role 为语义标签而非角色 ID，无法用责任范围校验）；最终授权由
    // /admin/approval-decisions 写时重验（账号启用 + approval_instance:decide +
    // 单据读权）。
    if item.work_item_type.is_document_approval()
        && item.status == WorkItemStatus::Open
        && item.approval_node_execution_id.is_some()
        && item.owner_user_id.as_deref() == Some(actor_id)
    {
        actions.push(WorkItemAllowedAction::Approve);
        actions.push(WorkItemAllowedAction::Reject);
    }
    let is_approval_responsibility =
        item.work_item_type.is_document_approval() || item.approval_node_execution_id.is_some();
    if access.can_manage && scope == WorkItemScope::Managed && !is_approval_responsibility {
        if has_permission(access, REASSIGN_PERMISSION) {
            actions.push(WorkItemAllowedAction::Reassign);
        }
        if is_w29_fields_closable(item) && has_permission(access, CLOSE_PERMISSION) {
            actions.push(WorkItemAllowedAction::Close);
        }
    }
    actions
}

/// 将实体对审批任务的通用责任变更禁令映射为稳定审批错误码。
pub(super) fn ensure_generic_work_item_mutation(item: &WorkItem) -> Result<()> {
    item.ensure_generic_responsibility_mutation()
        .map_err(|_| Error::from_approval_code(ErrorCode::ApprovalGenericWorkItemMutationForbidden))
}

pub(super) fn ensure_managed_access(access: &ActorAccess) -> Result<()> {
    if !access.can_manage || access.organization_ids.is_empty() {
        return Err(Error::Forbidden("当前账号没有任务责任管理范围".to_string()));
    }
    Ok(())
}

pub(super) fn ensure_item_in_managed_scope(item: &WorkItem, access: &ActorAccess) -> Result<()> {
    if covers_organization(access, &item.owner_organization_id) {
        return Ok(());
    }
    Err(Error::Forbidden("任务不在当前账号的责任管理范围内".to_string()))
}

pub(super) fn organization_filter(access: &ActorAccess) -> Vec<String> {
    if access
        .organization_ids
        .iter()
        .any(|organization_id| organization_id == "*")
    {
        return Vec::new();
    }
    access.organization_ids.to_vec()
}

pub(super) fn organizations_from_pairs(pairs: &[(String, Option<String>)]) -> Vec<String> {
    if pairs.iter().any(|(_, organization_id)| organization_id.is_none()) {
        return vec!["*".to_string()];
    }
    let mut organizations = pairs
        .iter()
        .filter_map(|(_, organization_id)| organization_id.clone())
        .collect::<Vec<_>>();
    organizations.sort();
    organizations.dedup();
    organizations
}

pub(super) fn responsibility_scope_for_role(
    role_id: &str,
    role_scopes: &[DataScope],
    user_scopes: &[DataScope],
) -> Vec<(String, Option<String>)> {
    let Some(role_coverage) = OrganizationCoverage::from_scopes(role_scopes) else {
        return Vec::new();
    };
    // 默认政策仍由 Service 拥有：用户未配置显式范围时解释为 All；角色未配置
    // 时上方已失败关闭为 None。
    let user_coverage = OrganizationCoverage::from_scopes(user_scopes).unwrap_or(OrganizationCoverage::All);
    role_coverage
        .intersect(&user_coverage)
        .map(|coverage| {
            ResponsibilityScopeSet::for_role(role_id, &coverage)
                .as_slice()
                .to_vec()
        })
        .unwrap_or_default()
}

pub(super) fn covers_responsibility(access: &ActorAccess, role: &str, organization_id: &str) -> bool {
    ResponsibilityScopeSet::new(access.responsibility_scopes.clone()).covers(role, organization_id)
}

pub(super) fn detail_scope(item: &WorkItem, actor_id: &str, access: &ActorAccess) -> Result<WorkItemScope> {
    if item.is_terminal()
        && (has_personal_history_access(item, actor_id)
            || (access.can_manage && covers_organization(access, &item.owner_organization_id)))
    {
        return Ok(WorkItemScope::History);
    }
    if item.is_owned_by(actor_id) {
        return Ok(WorkItemScope::Mine);
    }
    if item.status == WorkItemStatus::Open
        && access.can_manage
        && covers_organization(access, &item.owner_organization_id)
    {
        return Ok(WorkItemScope::Managed);
    }
    Err(Error::Forbidden("当前账号无权查看该任务".to_string()))
}

pub(super) fn has_personal_history_access(item: &WorkItem, actor_id: &str) -> bool {
    item.responsibility_actor_ids.iter().any(|id| id == actor_id)
        || item.completed_by.as_deref() == Some(actor_id)
        || item.closed_by.as_deref() == Some(actor_id)
}

pub(super) fn covers_organization(access: &ActorAccess, organization_id: &str) -> bool {
    OrganizationCoverage::from_targets(access.organization_ids.clone())
        .is_some_and(|coverage| coverage.covers(organization_id))
}
