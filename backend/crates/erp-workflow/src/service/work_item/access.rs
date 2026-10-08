//! 责任队列授权快照、范围过滤与允许动作。

use std::collections::HashSet;

use application_core::AuditActor;
use persistence_core::{Executor, NoTransaction};

use super::dto;
use super::order_access::{
    require_order_task_read, require_supplier_portal_task_read, require_supplier_portal_task_review,
    task_read_error,
};
use crate::entity::work_item::{
    AvailableWorkItemAccount, WorkItem, WorkItemBriefRelation, WorkItemStatus, WorkItemType,
};
use crate::error::{Error, ErrorCode, Result};
use crate::ports::{ObjectFact, ObjectFactMap, permission_covers};
use crate::repository::DocumentRegistryExt;
use crate::repository::prelude::*;
use crate::service::approval::execution::runtime_service::approval_task_readable_with_executor;

/// 按任务类型和业务对象类型查找固定责任关系。
///
/// # 参数
/// * `work_item_type` - 工作项类型
/// * `business_object_type` - 业务对象类型稳定码
///
/// # 返回
/// 已注册时返回关系；未注册时返回 `None`。
///
/// # 错误
/// 不返回错误。
pub fn object_policy(
    work_item_type: WorkItemType,
    business_object_type: &str,
) -> Option<&'static WorkItemBriefRelation> {
    work_item_type.brief_relation(business_object_type)
}
use super::{ProcessingBlockerView, ProcessingState, WorkItemAllowedAction, WorkItemScope, WorkItemService};

pub const MANAGE_PERMISSION: &str = "work_item:manage";
pub const REASSIGN_PERMISSION: &str = "work_item:reassign";
pub const CLOSE_PERMISSION: &str = "work_item:close";

pub struct ActorAccess {
    pub actor_id: String,
    pub permissions: Vec<String>,
    pub participant_document_ids: HashSet<String>,
    pub managed_owner_ids: Option<Vec<String>>,
    pub can_manage: bool,
}
impl ActorAccess {
    /// 以必填身份构造访问快照；权限与范围默认为空。
    ///
    /// # 参数
    /// * `actor_id` - 已认证操作人
    ///
    /// # 返回
    /// 返回空权限的访问快照。
    ///
    /// # 错误
    /// 无。
    pub fn new(actor_id: String) -> Self {
        Self {
            actor_id,
            permissions: Vec::new(),
            participant_document_ids: HashSet::new(),
            managed_owner_ids: None,
            can_manage: false,
        }
    }

    /// 设置权限代码。
    ///
    /// # 参数
    /// * `permissions` - 权限代码
    ///
    /// # 返回
    /// 返回更新后的快照。
    ///
    /// # 错误
    /// 无。
    pub fn with_permissions(mut self, permissions: Vec<String>) -> Self {
        self.permissions = permissions;
        self
    }

    /// 设置管理数据范围。
    ///
    /// # 参数
    /// * `ids` - 管理范围内负责人；`None` 表示不按负责人收窄
    ///
    /// # 返回
    /// 返回更新后的快照。
    ///
    /// # 错误
    /// 无。
    pub fn with_managed_owner_ids(mut self, ids: Option<Vec<String>>) -> Self {
        self.managed_owner_ids = ids;
        self
    }

    /// 设置管理权限标记。
    ///
    /// # 参数
    /// * `can_manage` - 是否具备管理权限
    ///
    /// # 返回
    /// 返回更新后的快照。
    ///
    /// # 错误
    /// 无。
    pub fn with_can_manage(mut self, can_manage: bool) -> Self {
        self.can_manage = can_manage;
        self
    }

    /// 设置参与单据范围。
    ///
    /// # 参数
    /// * `ids` - 参与人关联单据
    ///
    /// # 返回
    /// 返回更新后的快照。
    ///
    /// # 错误
    /// 无。
    pub fn with_participant_document_ids(mut self, ids: std::collections::HashSet<String>) -> Self {
        self.participant_document_ids = ids;
        self
    }
}

impl<A: crate::ports::WorkflowAuthorizationPort + Send + Sync + 'static> WorkItemService<A> {
    /// 写命令执行前重验对象存在、阅读权限和参与依据。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `item` - 待访问的工作项
    ///
    /// # 返回
    /// 对象可访问时无返回值。
    ///
    /// # 错误
    /// 授权快照读取失败时返回对应错误；对象不可访问时经 `task_read_error` 隐藏存在性。
    pub async fn ensure_object_participation(&self, actor: &AuditActor, item: &WorkItem) -> Result<()> {
        let access = self.actor_access(actor).await?;
        self.ensure_item_access(item, &access).await.map_err(task_read_error)
    }

    /// 用当前访问快照确认该任务能投影为已授权字段。
    ///
    /// # 参数
    /// * `item` - 待访问的工作项
    /// * `access` - 当前账号访问快照
    ///
    /// # 返回
    /// 投影非空时无返回值。
    ///
    /// # 错误
    /// 投影为空时返回禁止；对象事实读取失败时返回对应错误。
    pub async fn ensure_item_access(&self, item: &WorkItem, access: &ActorAccess) -> Result<()> {
        if self.authorized_fields_for_items(vec![item.clone()], access).await?.is_empty() {
            return Err(Error::Forbidden("业务对象不可访问".to_string()));
        }
        Ok(())
    }

    /// 按操作人账号类型与 ID 构造访问快照。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回含权限、参与单据与管理范围的快照。
    ///
    /// # 错误
    /// 角色、权限、参与单据或管理范围读取失败时返回对应错误。
    pub async fn actor_access(&self, actor: &AuditActor) -> Result<ActorAccess> {
        self.actor_access_for(actor.kind(), actor.id()).await
    }

    async fn actor_access_for(
        &self,
        account_kind: erp_core::AccountKind,
        actor_id: &str,
    ) -> Result<ActorAccess> {
        let role_ids = self.auth.role_ids(account_kind, actor_id).await?;
        let permissions = self.auth.permission_codes(account_kind, actor_id).await?;
        let participant_document_ids = self
            .db
            .document_participants()
            .document_ids_by_user(actor_id, &mut NoTransaction)
            .await?
            .into_iter()
            .collect();
        let has_manage_permission = self.auth.permissions_cover(&permissions, &[MANAGE_PERMISSION])?;
        let manage_role_ids =
            self.roles_granting_permission(&role_ids, MANAGE_PERMISSION, has_manage_permission).await?;
        let managed_owner_ids = self
            .auth
            .managed_task_owners(
                &AuditActor::new(actor_id.to_string(), actor_id.to_string(), account_kind),
                &mut NoTransaction,
            )
            .await?;
        Ok(ActorAccess::new(actor_id.to_string())
            .with_permissions(permissions)
            .with_participant_document_ids(participant_document_ids)
            .with_managed_owner_ids(managed_owner_ids)
            .with_can_manage(!manage_role_ids.is_empty()))
    }

    /// 定位实际授予指定权限的角色，使管理数据范围与权限来源关联。
    ///
    /// # 参数
    /// * `role_ids` - 账号当前角色
    /// * `permission` - 待定位的权限
    /// * `account_has_permission` - 账号是否已被判定具备该权限
    ///
    /// # 返回
    /// 账号不具备该权限时返回空；否则返回 `enforce` 通过的角色 ID。
    ///
    /// # 错误
    /// 角色鉴权失败时返回对应错误。
    pub async fn roles_granting_permission(
        &self,
        role_ids: &[String],
        permission: &str,
        account_has_permission: bool,
    ) -> Result<Vec<String>> {
        if !account_has_permission {
            return Ok(Vec::new());
        }
        let mut granting_roles = Vec::new();
        for role_id in role_ids {
            if self.auth.enforce(&format!("role:{role_id}"), permission).await? {
                granting_roles.push(role_id.clone());
            }
        }
        Ok(granting_roles)
    }

    /// 计算该任务在指定队列范围内的查看状态与允许动作。
    ///
    /// # 参数
    /// * `item` - 已授权的工作项投影
    /// * `scope` - 当前队列范围
    /// * `actor` - 已认证操作人
    /// * `access` - 当前账号访问快照
    ///
    /// # 返回
    /// 存在处理阻塞时返回阻塞视图；历史范围或非开放任务返回空动作；否则返回按权限算出的就绪视图。
    ///
    /// # 错误
    /// 处理阻塞读取失败时返回对应错误。
    pub async fn view_access(
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
    ///
    /// # 参数
    /// * `account_kind` - 账号类型
    /// * `actor_id` - 账号 ID
    /// * `read_permission` - 对象读取权限
    /// * `read_role_ids` - 已证明授予读取权的角色
    /// * `manage_role_ids` - 已证明授予管理权的角色
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回只包含当前事务仍有效角色所对应权限的访问快照。
    ///
    /// # 错误
    /// 角色、参与单据或管理范围读取失败时返回对应错误。
    pub async fn assignment_access_for_executor(
        &self,
        account_kind: erp_core::AccountKind,
        actor_id: &str,
        read_permission: &str,
        read_role_ids: &[String],
        manage_role_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<ActorAccess> {
        let role_ids = self.auth.role_ids_with_executor(account_kind, actor_id, executor).await?;
        let active_read_roles = intersect_role_ids(&role_ids, read_role_ids);
        let active_manage_roles = intersect_role_ids(&role_ids, manage_role_ids);
        let mut permissions = Vec::with_capacity(2);
        if !active_read_roles.is_empty() {
            permissions.push(read_permission.to_string());
        }
        if !active_manage_roles.is_empty() {
            permissions.push(MANAGE_PERMISSION.to_string());
        }
        let participant_document_ids = self
            .db
            .document_participants()
            .document_ids_by_user(actor_id, executor)
            .await?
            .into_iter()
            .collect();
        let managed_owner_ids = self
            .auth
            .managed_task_owners(
                &AuditActor::new(actor_id.to_string(), actor_id.to_string(), account_kind),
                executor,
            )
            .await?;
        Ok(ActorAccess::new(actor_id.to_string())
            .with_permissions(permissions)
            .with_participant_document_ids(participant_document_ids)
            .with_managed_owner_ids(managed_owner_ids)
            .with_can_manage(!active_manage_roles.is_empty()))
    }

    /// 使用调用方 executor 读取固定注册表对象事实并重验参与权。
    ///
    /// # 参数
    /// * `item` - 待访问的工作项
    /// * `access` - 当前账号访问快照
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 审批任务责任链匹配，或普通对象事实与读取权都成立时无返回值。
    ///
    /// # 错误
    /// 账号缺失、责任链不匹配、对象未注册、事实不可访问或读取权不足时返回禁止；端口读取失败时返回对应错误。
    pub async fn ensure_item_access_with_executor(
        &self,
        item: &WorkItem,
        access: &ActorAccess,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if item.work_item_type.is_document_approval() {
            let account = self
                .auth
                .load_account(&access.actor_id, executor)
                .await?
                .ok_or_else(|| Error::Forbidden("审批任务账号不存在".into()))?;
            let actor = AuditActor::new(account.id, account.login_account, account.kind);
            return if approval_task_readable_with_executor(&self.db, &self.auth, &actor, item, executor)
                .await?
            {
                Ok(())
            } else {
                Err(Error::Forbidden("审批任务责任链不匹配".into()))
            };
        }
        let policy = object_policy(item.work_item_type, &item.business_object_type)
            .ok_or_else(|| Error::Forbidden("任务类型未注册责任策略".to_string()))?;
        let keys = HashSet::from([(policy.object_kind, item.business_object_id.clone())]);
        let facts = self.facts.load_object_facts(&keys, executor).await?;
        let fact = facts
            .get(&(policy.object_kind, item.business_object_id.clone()))
            .ok_or_else(|| Error::Forbidden("任务业务对象不可访问".into()))?;
        require_supplier_portal_task_read(
            &self.auth,
            &access.actor_id,
            policy.object_kind,
            &item.business_object_id,
            executor,
        )
        .await?;
        require_order_task_read(&self.auth, &access.actor_id, policy.object_kind, fact, executor).await?;
        if authorized_item_fields(item.clone(), access, &facts).is_none() {
            return Err(Error::Forbidden("业务对象不可访问".to_string()));
        }
        Ok(())
    }

    /// 在领域决定事务内按当前账号、有效角色、读取权限、数据范围和对象参与事实重验访问。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `item` - 待决定的工作项
    /// * `executor` - 决定写入的事务执行器
    ///
    /// # 返回
    /// 执行权限、读取权限、对象参与与策略版本都仍成立时无返回值。
    ///
    /// # 错误
    /// 账号失效、任务未注册执行权限、权限或对象资格不足时返回禁止；授权或事实读取失败时返回对应错误。
    pub async fn ensure_domain_decision_access(
        &self,
        actor: &AuditActor,
        item: &WorkItem,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let account = self
            .auth
            .load_account(actor.id(), executor)
            .await?
            .ok_or_else(|| Error::Forbidden("操作账号不存在、已停用或身份已变化".to_string()))?;
        AvailableWorkItemAccount::from_account_kind(&account, actor.kind())
            .map_err(|_| Error::Forbidden("操作账号不存在、已停用或身份已变化".to_string()))?;
        let policy = object_policy(item.work_item_type, &item.business_object_type)
            .ok_or_else(|| Error::Forbidden("任务类型未注册责任策略".to_string()))?;
        let read_permission = policy.read_permission;
        let policy_revision = self.auth.policy_revision_with_executor(executor).await?;
        let role_ids = self.auth.role_ids_with_executor(actor.kind(), actor.id(), executor).await?;
        let execution_permissions =
            required_execution_permissions(item.work_item_type, &item.business_object_type)
                .ok_or_else(|| Error::Forbidden("任务类型未注册完整执行权限".to_string()))?;
        for permission in execution_permissions.as_slice() {
            let granting_roles = self.roles_granting_permission(&role_ids, permission, true).await?;
            if granting_roles.is_empty() {
                return Err(Error::Forbidden("当前账号已不具备任务所需的完整执行权限".to_string()));
            }
        }
        let read_role_ids = self.roles_granting_permission(&role_ids, read_permission, true).await?;
        if read_role_ids.is_empty() {
            return Err(Error::Forbidden("当前账号已不具备任务业务对象读取权限".to_string()));
        }
        let access = self
            .assignment_access_for_executor(
                actor.kind(),
                actor.id(),
                read_permission,
                &read_role_ids,
                &[],
                executor,
            )
            .await?;
        self.ensure_item_access_with_executor(item, &access, executor).await.map_err(task_read_error)?;
        require_supplier_portal_task_review(
            &self.auth,
            actor.id(),
            policy.object_kind,
            &item.business_object_id,
            executor,
        )
        .await?;
        self.auth.ensure_policy_snapshot_with_executor(policy_revision, executor).await
    }

    /// 构造并要求操作人具备非空的任务管理范围。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 具备管理权限且管理负责人范围不是空列表时返回访问快照。
    ///
    /// # 错误
    /// 没有管理权限或管理范围为空列表时返回禁止；快照读取失败时返回对应错误。
    pub async fn managed_access(&self, actor: &AuditActor) -> Result<ActorAccess> {
        let access = self.actor_access(actor).await?;
        if access.can_manage && !access.managed_owner_ids.as_ref().is_some_and(Vec::is_empty) {
            return Ok(access);
        }
        Err(Error::Forbidden("当前账号没有任务责任管理权限".to_string()))
    }
}

/// 从事务内 Casbin `g` 授权事实与启用角色形成角色集合。
///
/// # 参数
/// * `auth` - 授权端口
/// * `account_kind` - 账号类型
/// * `account_id` - 账号 ID
/// * `executor` - 调用方执行器
///
/// # 返回
/// 返回事务内当前角色 ID。
///
/// # 错误
/// 授权端口读取失败时返回对应错误。
pub async fn active_role_ids<A: crate::ports::WorkflowAuthorizationPort>(
    auth: &A,
    account_kind: erp_core::AccountKind,
    account_id: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<String>> {
    auth.role_ids_with_executor(account_kind, account_id, executor).await
}

/// 取当前启用角色与已授权角色的交集。
///
/// # 参数
/// * `active` - 当前启用角色
/// * `authorized` - 已授权角色
///
/// # 返回
/// 返回两边都出现的角色 ID，保留 `active` 的顺序。
///
/// # 错误
/// 不返回错误。
pub fn intersect_role_ids(active: &[String], authorized: &[String]) -> Vec<String> {
    active.iter().filter(|role_id| authorized.contains(role_id)).cloned().collect()
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
pub fn object_access_shapes(access: &ActorAccess) -> Vec<(WorkItemType, String)> {
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
pub fn has_permission(access: &ActorAccess, permission: &str) -> bool {
    access.permissions.iter().any(|owned| permission_covers(owned, permission))
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
pub fn authorized_fields(
    rows: Vec<crate::repository::WorkItemRow>,
    access: &ActorAccess,
    facts: &ObjectFactMap,
) -> Vec<dto::WorkItemFields> {
    rows.into_iter()
        .filter_map(|row| {
            let policy = object_policy(row.work_item_type, &row.business_object_type)?;
            let fact = facts.get(&(policy.object_kind, row.business_object_id.clone()))?;
            if !has_permission(access, policy.read_permission)
                || !(supplier_fulfillment_owner(
                    row.work_item_type,
                    &row.business_object_type,
                    row.owner_user_id.as_deref(),
                    access,
                ) || has_item_participation(
                    row.work_item_type,
                    row.owner_user_id.as_deref(),
                    &row.owner_role,
                    &row.owner_organization_id,
                    access,
                    fact,
                ))
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
pub fn authorized_item_fields(
    item: WorkItem,
    access: &ActorAccess,
    facts: &ObjectFactMap,
) -> Option<dto::WorkItemFields> {
    let policy = object_policy(item.work_item_type, &item.business_object_type)?;
    let fact = facts.get(&(policy.object_kind, item.business_object_id.clone()))?;
    if !has_permission(access, policy.read_permission)
        || !(supplier_fulfillment_owner(
            item.work_item_type,
            &item.business_object_type,
            item.owner_user_id.as_deref(),
            access,
        ) || has_item_participation(
            item.work_item_type,
            item.owner_user_id.as_deref(),
            &item.owner_role,
            &item.owner_organization_id,
            access,
            fact,
        ))
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
pub fn has_assignment_candidate_access(item: &WorkItem, access: &ActorAccess, facts: &ObjectFactMap) -> bool {
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

/// W26 只有供应商履约对象的两个正式异常任务使用具体责任人授权。
fn is_supplier_fulfillment_task(kind: WorkItemType, object_type: &str) -> bool {
    object_type == "SUPPLIER_FULFILLMENT_ORDER"
        && matches!(kind, WorkItemType::IntegrationResultUnknown | WorkItemType::BusinessException)
}

/// 具体任务责任只补充该对象的任务读取，普通单据访问另行校验。
fn supplier_fulfillment_owner(
    kind: WorkItemType,
    object_type: &str,
    owner: Option<&str>,
    access: &ActorAccess,
) -> bool {
    is_supplier_fulfillment_task(kind, object_type) && owner == Some(access.actor_id.as_str())
}

/// 返回执行任务的完整权限；普通任务返回空集，未注册执行对象失败关闭。
///
/// # 参数
/// * `work_item_type` - 工作项类型
/// * `business_object_type` - 业务对象类型稳定码
///
/// # 返回
/// 供应商履约异常、供给异常和实体已注册的执行任务返回权限码；普通任务返回空向量。未注册执行对象返回 `None`。
///
/// # 错误
/// 不返回错误。
pub fn required_execution_permissions(
    work_item_type: WorkItemType,
    business_object_type: &str,
) -> Option<Vec<&'static str>> {
    if is_supplier_fulfillment_task(work_item_type, business_object_type) {
        return Some(vec![
            "supplier_fulfillment_order:detail",
            "supplier_fulfillment_order:investigate",
            "supplier_fulfillment_order:complete",
        ]);
    }
    if work_item_type == WorkItemType::BusinessException && business_object_type == "SUPPLIER_OFFERING" {
        return Some(vec!["supplier_offering:resolve_supply_exception"]);
    }
    work_item_type.required_execution_permissions(business_object_type).map(|codes| codes.to_vec())
}

/// 判断账号是否覆盖执行任务在目标工作面所需的全部权限。
///
/// # 参数
/// * `work_item_type` - 工作项类型
/// * `business_object_type` - 业务对象类型稳定码
/// * `access` - 当前账号访问快照
///
/// # 返回
/// 所需权限全部被账号权限覆盖时返回 `true`；未注册执行对象返回 `false`。
///
/// # 错误
/// 不返回错误。
pub fn has_execution_permissions(
    work_item_type: WorkItemType,
    business_object_type: &str,
    access: &ActorAccess,
) -> bool {
    required_execution_permissions(work_item_type, business_object_type).is_some_and(|required| {
        required.iter().all(|code| access.permissions.iter().any(|owned| permission_covers(owned, code)))
    })
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
pub fn has_item_participation(
    work_item_type: WorkItemType,
    owner_user_id: Option<&str>,
    owner_role: &str,
    owner_organization_id: &str,
    access: &ActorAccess,
    fact: &ObjectFact,
) -> bool {
    let is_explicit_owner =
        work_item_type.uses_explicit_owner_authorization() && owner_user_id == Some(access.actor_id.as_str());
    is_explicit_owner
        || (access.can_manage && covers_owner(access, owner_user_id))
        || has_object_participation(access, owner_role, owner_organization_id, fact)
}

/// 按领域创建人或根单据参与名单判断对象参与关系。
///
/// # 参数
/// * `access` - 当前账号访问事实
/// * `_owner_role` - 责任角色标识，保留调用合同，不用于对象参与判断
/// * `_owner_organization_id` - 责任组织 ID，保留调用合同，不用于对象参与判断
/// * `fact` - 业务对象的领域创建人和根单据事实
///
/// # 返回
/// 非空创建人与当前账号精确匹配，或账号参与根单据时返回 `true`。
///
/// # 错误
/// 无；空白创建人不授予创建人资格，调用方须独立验证权限及任务责任。
pub fn has_object_participation(
    access: &ActorAccess,
    _owner_role: &str,
    _owner_organization_id: &str,
    fact: &ObjectFact,
) -> bool {
    (!fact.created_by.trim().is_empty() && fact.created_by == access.actor_id)
        || access.participant_document_ids.contains(&fact.root_document_id)
}

pub struct ViewAccess {
    pub processing_state: ProcessingState,
    pub processing_blocker: Option<ProcessingBlockerView>,
    pub allowed_actions: Vec<WorkItemAllowedAction>,
    pub action_blockers: Vec<ProcessingBlockerView>,
}

impl ViewAccess {
    /// 构造可处理的查看结果。
    ///
    /// # 参数
    /// * `allowed_actions` - 当前允许的动作
    ///
    /// # 返回
    /// 返回处理状态为就绪、无阻塞的视图。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn ready(allowed_actions: Vec<WorkItemAllowedAction>) -> Self {
        Self {
            processing_state: ProcessingState::Ready,
            processing_blocker: None,
            allowed_actions,
            action_blockers: Vec::new(),
        }
    }

    /// 构造审批受阻、不允许动作的查看结果。
    ///
    /// # 参数
    /// * `blocker` - 处理阻塞原因
    ///
    /// # 返回
    /// 返回处理状态为审批受阻、动作为空的视图。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn blocked(blocker: ProcessingBlockerView) -> Self {
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
pub fn allowed_actions(
    item: &dto::WorkItemFields,
    scope: WorkItemScope,
    actor_id: &str,
    access: &ActorAccess,
) -> Vec<WorkItemAllowedAction> {
    let mut actions = vec![WorkItemAllowedAction::View];
    if item.owner_user_id.as_deref() == Some(actor_id)
        && has_execution_permissions(item.work_item_type, &item.business_object_type, access)
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
        && has_permission(access, "approval_instance:decide")
    {
        actions.push(WorkItemAllowedAction::Approve);
        actions.push(WorkItemAllowedAction::Reject);
    }
    let is_approval_responsibility =
        item.work_item_type.is_document_approval() || item.approval_node_execution_id.is_some();
    if access.can_manage
        && covers_owner(access, item.owner_user_id.as_deref())
        && scope == WorkItemScope::Managed
        && !is_approval_responsibility
    {
        if has_permission(access, REASSIGN_PERMISSION) {
            actions.push(WorkItemAllowedAction::Reassign);
        }
        if item.is_w29_closable() && has_permission(access, CLOSE_PERMISSION) {
            actions.push(WorkItemAllowedAction::Close);
        }
    }
    actions
}

/// 将实体对审批任务的通用责任变更禁令映射为稳定审批错误码。
///
/// # 参数
/// * `item` - 待变更责任的工作项
///
/// # 返回
/// 实体允许通用责任变更时无返回值。
///
/// # 错误
/// 实体拒绝时返回 `ApprovalGenericWorkItemMutationForbidden`。
pub fn ensure_generic_work_item_mutation(item: &WorkItem) -> Result<()> {
    item.ensure_generic_responsibility_mutation()
        .map_err(|_| Error::from_approval_code(ErrorCode::ApprovalGenericWorkItemMutationForbidden))
}

/// 要求访问快照具备非空的任务管理范围。
///
/// # 参数
/// * `access` - 当前账号访问快照
///
/// # 返回
/// 具备管理权限且负责人范围不是空列表时无返回值。
///
/// # 错误
/// 否则返回禁止。
pub fn ensure_managed_access(access: &ActorAccess) -> Result<()> {
    if !access.can_manage || access.managed_owner_ids.as_ref().is_some_and(Vec::is_empty) {
        return Err(Error::Forbidden("当前账号没有任务责任管理范围".to_string()));
    }
    Ok(())
}

/// 要求任务当前负责人落在管理范围内。
///
/// # 参数
/// * `item` - 工作项
/// * `access` - 当前账号访问快照
///
/// # 返回
/// 具备管理权限且负责人被范围覆盖时无返回值。
///
/// # 错误
/// 否则返回禁止。
pub fn ensure_item_in_managed_scope(item: &WorkItem, access: &ActorAccess) -> Result<()> {
    if access.can_manage && covers_owner(access, item.owner_user_id.as_deref()) {
        return Ok(());
    }
    Err(Error::Forbidden("任务不在当前账号的责任管理范围内".to_string()))
}

/// 判断当前账号查看该任务时应使用的队列范围。
///
/// # 参数
/// * `item` - 工作项
/// * `actor_id` - 当前账号 ID
/// * `access` - 当前账号访问快照
///
/// # 返回
/// 终态且本人有历史或管理覆盖时返回历史；本人当前负责时返回我的；开放且管理范围覆盖时返回管理。
///
/// # 错误
/// 三种范围都不成立时返回禁止。
pub fn detail_scope(item: &WorkItem, actor_id: &str, access: &ActorAccess) -> Result<WorkItemScope> {
    if item.is_terminal()
        && (has_personal_history_access(item, actor_id)
            || (access.can_manage && covers_owner(access, item.owner_user_id.as_deref())))
    {
        return Ok(WorkItemScope::History);
    }
    if item.is_owned_by(actor_id) {
        return Ok(WorkItemScope::Mine);
    }
    if item.status == WorkItemStatus::Open
        && access.can_manage
        && covers_owner(access, item.owner_user_id.as_deref())
    {
        return Ok(WorkItemScope::Managed);
    }
    Err(Error::Forbidden("当前账号无权查看该任务".to_string()))
}

/// 判断账号是否出现在任务的历史责任、完成或关闭记录中。
///
/// # 参数
/// * `item` - 工作项
/// * `actor_id` - 当前账号 ID
///
/// # 返回
/// 责任历史、完成人或关闭人包含该账号时返回 `true`。
///
/// # 错误
/// 不返回错误。
pub fn has_personal_history_access(item: &WorkItem, actor_id: &str) -> bool {
    item.responsibility_actor_ids.iter().any(|id| id == actor_id)
        || item.completed_by.as_deref() == Some(actor_id)
        || item.closed_by.as_deref() == Some(actor_id)
}

/// 判断管理范围是否覆盖指定负责人。
///
/// # 参数
/// * `access` - 当前账号访问快照
/// * `owner` - 当前负责人；没有负责人时为 `None`
///
/// # 返回
/// 管理范围未收窄时返回 `true`；否则仅当负责人 ID 在列表中时返回 `true`。
///
/// # 错误
/// 不返回错误。
pub fn covers_owner(access: &ActorAccess, owner: Option<&str>) -> bool {
    access
        .managed_owner_ids
        .as_ref()
        .is_none_or(|ids| owner.is_some_and(|owner| ids.iter().any(|id| id == owner)))
}

fn apply_object_display(fields: &mut dto::WorkItemFields, fact: &ObjectFact) {
    fields.business_object_label = fact.label.clone();
    fields.root_business_object_id = fact.root_document_id.clone();
    let subject = fact.subject_briefs.get(&fields.subject_version);
    fields.counterparty_label =
        subject.and_then(|item| item.counterparty_label.clone()).or_else(|| fact.counterparty_label.clone());
    let preserve_task_impact = fields.work_item_type.uses_explicit_owner_authorization()
        && fields.impact_summary.as_deref().map(str::trim).is_some_and(|value| !value.is_empty());
    if !preserve_task_impact {
        fields.impact_summary =
            subject.and_then(|item| item.impact_summary.clone()).or_else(|| fact.impact_summary.clone());
    }
}

#[cfg(test)]
mod task_policy_tests {
    use erp_core::ids::WorkItemId;

    use super::*;
    use crate::entity::work_item::SupplierPortalReviewTaskData;
    use crate::ports::ObjectKind;

    #[test]
    fn supplier_portal_review_requires_current_owner_and_review_permission() {
        let task = WorkItem::new_supplier_portal_review(
            WorkItemId::new("task"),
            SupplierPortalReviewTaskData {
                request_id: "request".into(),
                subject_version: "submission-1".into(),
                owner_user_id: "reviewer".into(),
                owner_organization_id: "procurement".into(),
                due_at: None,
                impact_summary: None,
            },
        )
        .unwrap();
        let facts = ObjectFactMap::from([(
            (ObjectKind::SupplierPortalRequest, "request".into()),
            ObjectFact::new("request", "供应商申请", "supplier"),
        )]);
        let full = ActorAccess::new("reviewer".into()).with_permissions(vec![
            "supplier_portal_request:detail".into(),
            "supplier_portal_request:review".into(),
        ]);
        let fields = authorized_item_fields(task.clone(), &full, &facts).unwrap();
        assert!(
            allowed_actions(&fields, WorkItemScope::Mine, "reviewer", &full)
                .contains(&WorkItemAllowedAction::Process)
        );
        assert!(
            !allowed_actions(&fields, WorkItemScope::Mine, "other", &full)
                .contains(&WorkItemAllowedAction::Process)
        );
        assert!(has_assignment_candidate_access(&task, &full, &facts));
        let read_only = ActorAccess::new("reviewer".into())
            .with_permissions(vec!["supplier_portal_request:detail".into()]);
        let read_fields = authorized_item_fields(task.clone(), &read_only, &facts).unwrap();
        assert!(
            !allowed_actions(&read_fields, WorkItemScope::Mine, "reviewer", &read_only)
                .contains(&WorkItemAllowedAction::Process)
        );
        assert!(!has_assignment_candidate_access(&task, &read_only, &facts));
        let unrelated = ActorAccess::new("other".into()).with_permissions(full.permissions.clone());
        assert!(authorized_item_fields(task, &unrelated, &facts).is_none());
        assert!(WorkItemType::SupplierPortalReview.required_execution_permissions("unknown").is_none());
    }

    #[test]
    fn object_creator_participation_requires_exact_nonblank_identity() {
        let fact = ObjectFact::new("root", "资金单据", "creator");
        let creator = ActorAccess::new("creator".into());
        assert!(has_object_participation(&creator, "finance", "organization", &fact));
        for actor_id in ["", "another", " creator "] {
            let access = ActorAccess::new(actor_id.into());
            assert!(!has_object_participation(&access, "finance", "organization", &fact));
        }
        for identity in ["", " ", "\t\n"] {
            let fact = ObjectFact::new("root", "资金单据", identity);
            let access = ActorAccess::new(identity.into());
            assert!(!has_object_participation(&access, "finance", "organization", &fact));
        }
    }

    #[test]
    fn root_participant_qualifies_independently_of_missing_creator() {
        let participant =
            ActorAccess::new("reader".into()).with_participant_document_ids(HashSet::from(["root".into()]));
        let unrelated = ActorAccess::new("reader".into())
            .with_participant_document_ids(HashSet::from(["other-root".into()]));
        for identity in ["", " ", "\t\n", "another-creator"] {
            let fact = ObjectFact::new("root", "资金单据", identity);
            assert!(has_object_participation(&participant, "finance", "organization", &fact));
            assert!(!has_object_participation(&unrelated, "finance", "organization", &fact));
        }
    }

    #[test]
    fn fulfillment_exception_owner_is_limited_to_exact_task_type_and_object() {
        let owner = ActorAccess::new("handler".into());
        assert!(supplier_fulfillment_owner(
            WorkItemType::BusinessException,
            "SUPPLIER_FULFILLMENT_ORDER",
            Some("handler"),
            &owner
        ));
        assert!(!supplier_fulfillment_owner(
            WorkItemType::BusinessException,
            "SUPPLIER_FULFILLMENT_ORDER",
            Some("someone-else"),
            &owner
        ));
        assert!(!supplier_fulfillment_owner(
            WorkItemType::BusinessException,
            "SUPPLIER_OFFERING",
            Some("handler"),
            &owner
        ));
        assert!(!supplier_fulfillment_owner(
            WorkItemType::DocumentApproval,
            "SUPPLIER_FULFILLMENT_ORDER",
            Some("handler"),
            &owner
        ));
        let permissions = required_execution_permissions(
            WorkItemType::IntegrationResultUnknown,
            "SUPPLIER_FULFILLMENT_ORDER",
        )
        .unwrap();
        assert_eq!(
            permissions,
            vec![
                "supplier_fulfillment_order:detail",
                "supplier_fulfillment_order:investigate",
                "supplier_fulfillment_order:complete"
            ]
        );
    }
}
