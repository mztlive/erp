//! 命令与候选人的业务资格；任务范围由共享 identity 授权服务证明。

use std::collections::HashSet;

use application_core::AuditActor;
use erp_core::Result as CoreResult;
use erp_identity::{Permission, PermissionSet};
use persistence_core::{Executor, NoTransaction};

#[cfg(test)]
use super::WorkItemAllowedAction;
#[cfg(test)]
use super::dto;
use super::order_access::{
    require_order_task_read, require_supplier_portal_task_read, require_supplier_portal_task_review,
    task_read_error,
};
use super::{WorkItemScope, WorkItemService};
use crate::entity::work_item::{
    TaskAccess, WorkItem, WorkItemStatus, WorkItemType, required_execution_permissions,
};
use crate::error::{Error, ErrorCode, Result};
use crate::ports::{ObjectFact, ObjectFactMap};
use crate::repository::DocumentRegistryExt;
use crate::repository::prelude::*;
use crate::service::approval::execution::runtime_service::approval_task_readable_with_executor;

/// 办理人或转交候选人的操作权限与业务参与事实；查询范围另由 identity 证明。
pub struct TaskEligibility {
    pub(super) actor: AuditActor,
    pub task: TaskAccess,
    pub participant_document_ids: HashSet<String>,
}
impl TaskEligibility {
    /// 以必填身份构造访问快照；权限与范围默认为空。
    ///
    /// # 参数
    /// * `actor` - 已认证的完整身份
    ///
    /// # 返回
    /// 返回空权限的访问快照。
    ///
    /// # 错误
    /// 无。
    pub fn new(actor: AuditActor) -> Self {
        Self { task: TaskAccess::new(actor.id().to_owned()), actor, participant_document_ids: HashSet::new() }
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
    pub fn with_permissions(mut self, permissions: PermissionSet) -> Self {
        self.task.permissions = permissions;
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
    /// 任务控制命令执行前按 manage DataScope 重验任务负责人范围。
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
    pub async fn ensure_task_control_access(&self, actor: &AuditActor, item: &WorkItem) -> Result<()> {
        self.ensure_task_manage(actor, item, &mut NoTransaction).await
    }

    /// 按操作人账号类型与 ID 构造访问快照。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `item` - 实际工作项
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回权限、参与单据及当前任务的管理资格，不用于其他任务。
    ///
    /// # 错误
    /// 权限、参与单据或任务管理授权失败时返回对应错误。
    pub async fn task_eligibility(
        &self,
        actor: &AuditActor,
        item: &WorkItem,
        executor: &mut dyn Executor,
    ) -> Result<TaskEligibility> {
        let mut required = item.qualification_permissions()?;
        required.extend(
            ["work_item:list", "work_item:detail", "work_item:manage", "approval_instance:decide"]
                .into_iter()
                .map(Permission::parse)
                .collect::<CoreResult<Vec<_>>>()?,
        );
        let permissions = self
            .identity
            .qualified_permissions(actor, &required, executor)
            .await?
            .ok_or_else(|| Error::Forbidden("任务操作账号已失效".into()))?
            .permissions();
        let can_manage = match self.authorization.require(actor, "manage", &item.base.id, executor).await {
            Ok(()) => true,
            Err(Error::Forbidden(_)) => false,
            Err(error) => return Err(error),
        };
        let mut access = self.business_eligibility(actor, permissions, executor).await?;
        if can_manage {
            access.task.managed_task_ids.insert(item.base.id.clone());
        }
        Ok(access)
    }

    /// 在已核验功能资格后读取任务业务参与事实。
    /// # 参数
    /// actor、permissions 为已经证明的身份与功能资格，executor 为原执行器。
    /// # 返回
    /// 返回当前参与单据事实，不产生额外任务管理授权。
    /// # 错误
    /// 参与关系读取失败时拒绝。
    pub(super) async fn business_eligibility(
        &self,
        actor: &AuditActor,
        permissions: PermissionSet,
        executor: &mut dyn Executor,
    ) -> Result<TaskEligibility> {
        let participants = self.db.document_participants().document_ids_by_user(actor.id(), executor).await?;
        Ok(TaskEligibility::new(actor.clone())
            .with_permissions(permissions)
            .with_participant_document_ids(participants.into_iter().collect()))
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
        access: &TaskEligibility,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if item.work_item_type.is_document_approval() {
            return if approval_task_readable_with_executor(
                &self.identity,
                &self.db,
                &self.auth,
                &access.actor,
                item,
                executor,
            )
            .await?
            {
                Ok(())
            } else {
                Err(Error::Forbidden("审批任务责任链不匹配".into()))
            };
        }
        let policy = item
            .work_item_type
            .brief_relation(&item.business_object_type)
            .ok_or_else(|| Error::Forbidden("任务类型未注册责任策略".to_string()))?;
        let keys = HashSet::from([(policy.object_kind, item.business_object_id.clone())]);
        let facts = self.facts.load_object_facts(&keys, executor).await?;
        let fact = facts
            .get(&(policy.object_kind, item.business_object_id.clone()))
            .ok_or_else(|| Error::Forbidden("任务业务对象不可访问".into()))?;
        require_supplier_portal_task_read(
            &self.auth,
            &access.actor,
            policy.object_kind,
            &item.business_object_id,
            executor,
        )
        .await?;
        if !item.owns_procurement_task(&access.task.actor_id) {
            require_order_task_read(&self.auth, &access.actor, policy.object_kind, fact, executor).await?;
        }
        if !has_item_access(item, access, &facts) {
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
        let policy = item
            .work_item_type
            .brief_relation(&item.business_object_type)
            .ok_or_else(|| Error::Forbidden("任务类型未注册责任策略".into()))?;
        let required = item.qualification_permissions()?;
        let qualified = self
            .identity
            .qualified_permissions(actor, &required, executor)
            .await?
            .ok_or_else(|| Error::Forbidden("操作账号不存在、已停用或身份已变化".into()))?;
        let permissions = qualified.permissions();
        if !required.iter().all(|permission| permissions.covers_one(permission)) {
            return Err(Error::Forbidden("当前账号已不具备任务所需的完整执行或读取权限".into()));
        }
        let access = self.business_eligibility(actor, permissions, executor).await?;
        self.ensure_item_access_with_executor(item, &access, executor).await.map_err(task_read_error)?;
        require_supplier_portal_task_review(
            &self.auth,
            actor,
            policy.object_kind,
            &item.business_object_id,
            executor,
        )
        .await?;
        Ok(self.identity.ensure_policy_snapshot_with_executor(qualified.policy_revision(), executor).await?)
    }

    /// 在操作执行器内验证 work_item:manage 及其 DataScope。
    /// # 参数
    /// `actor` 为真实账号，`item` 为当前任务，`executor` 与写入共享事务。
    /// # 返回
    /// 仅在当前管理数据范围覆盖负责人时成功。
    /// # 错误
    /// 无权限、越界、账号失效或授权读取失败时拒绝。
    pub async fn ensure_task_manage(
        &self,
        actor: &AuditActor,
        item: &WorkItem,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.authorization.require(actor, "manage", &item.base.id, executor).await
    }
}

/// 按对象权限、参与关系与权威版本判断单个工作项业务资格。
///
/// # 参数
/// * `item` - 待授权工作项实体
/// * `access` - 当前账号访问事实
/// * `facts` - 已加载的业务对象事实
///
/// # 返回
/// 权限、参与关系和业务版本均满足时返回 true。
///
/// # 错误
/// 无；未注册或无法证明访问权时失败关闭。
pub fn has_item_access(item: &WorkItem, access: &TaskEligibility, facts: &ObjectFactMap) -> bool {
    let Some(policy) = item.work_item_type.brief_relation(&item.business_object_type) else { return false };
    let Some(fact) = facts.get(&(policy.object_kind, item.business_object_id.clone())) else { return false };
    if item.work_item_type == WorkItemType::SalesInquiryProcessing {
        let Some(ids) = fact.responsibility_scope_ids.as_ref() else { return false };
        let Some(version) =
            item.subject_version.strip_prefix("requirement-").and_then(|value| value.parse().ok())
        else {
            return false;
        };
        if !item.owns_inquiry_task(&access.task.actor_id, &item.business_object_id, version, ids) {
            return false;
        }
    }
    access.task.permits(policy.read_permission)
        && (supplier_fulfillment_owner(
            item.work_item_type,
            &item.business_object_type,
            item.owner_user_id.as_deref(),
            access,
        ) || has_item_participation(
            &item.base.id,
            item.work_item_type,
            item.owner_user_id.as_deref(),
            &item.owner_role,
            &item.owner_organization_id,
            access,
            fact,
        ))
        && fact.subject_versions.accepts_task(
            item.work_item_type,
            item.status,
            &item.base.id,
            &item.subject_version,
        )
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
pub fn has_assignment_candidate_access(
    item: &WorkItem,
    access: &TaskEligibility,
    facts: &ObjectFactMap,
) -> bool {
    if item.work_item_type == WorkItemType::ProcurementOrderCreation && !item.is_open_procurement_task() {
        return false;
    }
    let Some(policy) = item.work_item_type.brief_relation(&item.business_object_type) else {
        return false;
    };
    let Some(fact) = facts.get(&(policy.object_kind, item.business_object_id.clone())) else {
        return false;
    };
    if item.work_item_type == WorkItemType::SalesInquiryProcessing
        && !fact.responsibility_scope_ids.as_ref().is_some_and(|ids| {
            !item.responsibility_scope_ids().is_empty()
                && item.responsibility_scope_ids().iter().all(|id| ids.contains(id))
        })
    {
        return false;
    }
    access.task.permits(policy.read_permission)
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
    access: &TaskEligibility,
) -> bool {
    is_supplier_fulfillment_task(kind, object_type) && owner == Some(access.task.actor_id.as_str())
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
    access: &TaskEligibility,
) -> bool {
    required_execution_permissions(work_item_type, business_object_type)
        .is_some_and(|required| required.iter().all(|code| access.task.permits(code)))
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
    task_id: &str,
    work_item_type: WorkItemType,
    owner_user_id: Option<&str>,
    owner_role: &str,
    owner_organization_id: &str,
    access: &TaskEligibility,
    fact: &ObjectFact,
) -> bool {
    let is_explicit_owner = work_item_type.uses_explicit_owner_authorization()
        && owner_user_id == Some(access.task.actor_id.as_str());
    is_explicit_owner
        || access.task.can_manage(task_id)
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
    access: &TaskEligibility,
    _owner_role: &str,
    _owner_organization_id: &str,
    fact: &ObjectFact,
) -> bool {
    (!fact.created_by.trim().is_empty() && fact.created_by == access.task.actor_id)
        || access.participant_document_ids.contains(&fact.root_document_id)
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
    if item.work_item_type == WorkItemType::SalesInquiryProcessing {
        return Err(Error::Forbidden("询价责任变更必须使用询价专用命令".into()));
    }
    item.ensure_generic_responsibility_mutation()
        .map_err(|_| Error::from_approval_code(ErrorCode::ApprovalGenericWorkItemMutationForbidden))
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
pub fn detail_scope(item: &WorkItem, actor_id: &str, access: &TaskEligibility) -> Result<WorkItemScope> {
    if item.is_terminal()
        && (has_personal_history_access(item, actor_id) || access.task.can_manage(&item.base.id))
    {
        return Ok(WorkItemScope::for_task(item, actor_id));
    }
    if item.is_owned_by(actor_id) {
        return Ok(WorkItemScope::for_task(item, actor_id));
    }
    if item.status == WorkItemStatus::Open && access.task.can_manage(&item.base.id) {
        return Ok(WorkItemScope::for_task(item, actor_id));
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

#[cfg(test)]
mod task_policy_tests {
    use erp_core::AccountKind;
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
        let full = TaskEligibility::new(AuditActor::new(
            "reviewer".to_string(),
            "reviewer".to_string(),
            AccountKind::Admin,
        ))
        .with_permissions(PermissionSet::new(
            vec!["supplier_portal_request:detail".into(), "supplier_portal_request:review".into()]
                .into_iter()
                .map(|code: String| Permission::parse(code).unwrap()),
        ));
        assert!(has_item_access(&task, &full, &facts));
        let fields = dto::WorkItemFields::from(task.clone());
        assert!(
            fields
                .allowed_actions(WorkItemScope::Mine, "reviewer", &full.task)
                .contains(&WorkItemAllowedAction::Process)
        );
        assert!(
            !fields
                .allowed_actions(WorkItemScope::Mine, "other", &full.task)
                .contains(&WorkItemAllowedAction::Process)
        );
        assert!(has_assignment_candidate_access(&task, &full, &facts));
        let read_only = TaskEligibility::new(AuditActor::new(
            "reviewer".to_string(),
            "reviewer".to_string(),
            AccountKind::Admin,
        ))
        .with_permissions(PermissionSet::new(
            vec!["supplier_portal_request:detail".into()]
                .into_iter()
                .map(|code: String| Permission::parse(code).unwrap()),
        ));
        assert!(has_item_access(&task, &read_only, &facts));
        let read_fields = dto::WorkItemFields::from(task.clone());
        assert!(
            !read_fields
                .allowed_actions(WorkItemScope::Mine, "reviewer", &read_only.task)
                .contains(&WorkItemAllowedAction::Process)
        );
        assert!(!has_assignment_candidate_access(&task, &read_only, &facts));
        let unrelated = TaskEligibility::new(AuditActor::new(
            "other".to_string(),
            "other".to_string(),
            AccountKind::Admin,
        ))
        .with_permissions(full.task.permissions.clone());
        assert!(!has_item_access(&task, &unrelated, &facts));
        assert!(WorkItemType::SupplierPortalReview.required_execution_permissions("unknown").is_none());
    }

    #[test]
    fn object_creator_participation_requires_exact_nonblank_identity() {
        let fact = ObjectFact::new("root", "资金单据", "creator");
        let creator = TaskEligibility::new(AuditActor::new(
            "creator".to_string(),
            "creator".to_string(),
            AccountKind::Admin,
        ));
        assert!(has_object_participation(&creator, "finance", "organization", &fact));
        for actor_id in ["", "another", " creator "] {
            let access = TaskEligibility::new(AuditActor::new(
                actor_id.to_string(),
                actor_id.to_string(),
                AccountKind::Admin,
            ));
            assert!(!has_object_participation(&access, "finance", "organization", &fact));
        }
        for identity in ["", " ", "\t\n"] {
            let fact = ObjectFact::new("root", "资金单据", identity);
            let access = TaskEligibility::new(AuditActor::new(
                identity.to_string(),
                identity.to_string(),
                AccountKind::Admin,
            ));
            assert!(!has_object_participation(&access, "finance", "organization", &fact));
        }
    }

    #[test]
    fn root_participant_qualifies_independently_of_missing_creator() {
        let participant = TaskEligibility::new(AuditActor::new(
            "reader".to_string(),
            "reader".to_string(),
            AccountKind::Admin,
        ))
        .with_participant_document_ids(HashSet::from(["root".into()]));
        let unrelated = TaskEligibility::new(AuditActor::new(
            "reader".to_string(),
            "reader".to_string(),
            AccountKind::Admin,
        ))
        .with_participant_document_ids(HashSet::from(["other-root".into()]));
        for identity in ["", " ", "\t\n", "another-creator"] {
            let fact = ObjectFact::new("root", "资金单据", identity);
            assert!(has_object_participation(&participant, "finance", "organization", &fact));
            assert!(!has_object_participation(&unrelated, "finance", "organization", &fact));
        }
    }

    #[test]
    fn fulfillment_exception_owner_is_limited_to_exact_task_type_and_object() {
        let owner = TaskEligibility::new(AuditActor::new(
            "handler".to_string(),
            "handler".to_string(),
            AccountKind::Admin,
        ));
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
