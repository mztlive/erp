//! 审批业务适配器注册与资格重验。
//!
//! 12 个 `PROCESS_REQUIRED` 类型必须登记完整规格；9 个 `NO_APPROVAL` 类型
//! 不得注册空适配器。领域动作由各 DocumentType 子阶段接线。

use bpm::ProcessKind;

use super::policy::{
    ApprovalDomainAction, ApprovalSubjectSnapshotField, ApprovalSubjectVersionSource, DocumentApprovalPolicy,
    OwnerOrganizationSource, ProcessRequiredApprovalPolicy, SeparationOfDutiesPolicy, WorkItemOwnerRole,
    policy_of, require_process_required,
};
use super::process_kind::process_kind_of;
use crate::entity::document_registry::DocumentType;
use crate::error::{Error, Result};
use crate::ports::OrderTaskSource;

/// 适配器读取范围：按单据责任组织重验 对象范围 与对象读取权。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdapterReadScope {
    /// 使用当前单据组织与创建人上下文。
    DocumentOrganizationAndCreator,
}

/// `PROCESS_REQUIRED` 适配器规格。缺少任一字段即注册不完整。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalAdapterSpec {
    /// 固定单据类型。
    pub document_type: DocumentType,
    /// 一对一流程种类。
    pub process_kind: ProcessKind,
    /// 提交版本权威来源。
    pub subject_version_source: ApprovalSubjectVersionSource,
    /// 启动快照字段。
    pub subject_snapshot_fields: &'static [ApprovalSubjectSnapshotField],
    /// 提交并启动动作。
    pub on_approval_start: ApprovalDomainAction,
    /// 最终通过动作。
    pub on_final_approve: ApprovalDomainAction,
    /// 终态驳回动作。
    pub on_reject: ApprovalDomainAction,
    /// 撤回与受阻取消动作。
    pub cancel_action: ApprovalDomainAction,
    /// WorkItem 责任角色。
    pub owner_role: WorkItemOwnerRole,
    /// 责任组织来源。
    pub owner_organization_source: OwnerOrganizationSource,
    /// 对象读取范围。
    pub read_scope: AdapterReadScope,
}

/// 绑定/升级时的单据组织与创建人上下文。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindingRevalidationContext {
    /// 当前或拟创建订单来源，变更单必须指向原单。
    pub order_source: Option<OrderTaskSource>,
    /// 订单及变更沿当前原单取得的内部业务部门；不得填入结算主体。
    pub business_org_unit_id: Option<String>,
    /// 当前业务负责人；非订单类型可使用不可变创建人。
    pub scope_owner_user_id: Option<String>,
    /// 当前单据责任组织。
    pub organization_id: String,
    /// 单据创建人或提交人。
    pub creator_id: String,
}

impl BindingRevalidationContext {
    /// 按固定业务类型映射授权事实；身份维度不得混用。
    /// 以必填身份构造绑定重验上下文；可选维度默认为空。
    ///
    /// # 参数
    /// * `organization_id` - 当前单据责任组织
    /// * `creator_id` - 单据创建人或提交人
    ///
    /// # 返回
    /// 返回可选维度均为 `None` 的上下文。
    ///
    /// # 错误
    /// 无。
    pub fn new(organization_id: String, creator_id: String) -> Self {
        Self {
            order_source: None,
            business_org_unit_id: None,
            scope_owner_user_id: None,
            organization_id,
            creator_id,
        }
    }

    /// 设置订单来源。
    ///
    /// # 参数
    /// * `order_source` - 当前或拟创建订单来源
    ///
    /// # 返回
    /// 返回更新后的上下文。
    ///
    /// # 错误
    /// 无。
    pub fn with_order_source(mut self, order_source: Option<OrderTaskSource>) -> Self {
        self.order_source = order_source;
        self
    }

    /// 设置内部业务部门。
    ///
    /// # 参数
    /// * `business_org_unit_id` - 订单及变更沿当前原单取得的内部业务部门
    ///
    /// # 返回
    /// 返回更新后的上下文。
    ///
    /// # 错误
    /// 无。
    pub fn with_business_org_unit_id(mut self, business_org_unit_id: Option<String>) -> Self {
        self.business_org_unit_id = business_org_unit_id;
        self
    }

    /// 设置当前业务负责人。
    ///
    /// # 参数
    /// * `scope_owner_user_id` - 当前业务负责人
    ///
    /// # 返回
    /// 返回更新后的上下文。
    ///
    /// # 错误
    /// 无。
    pub fn with_scope_owner_user_id(mut self, scope_owner_user_id: Option<String>) -> Self {
        self.scope_owner_user_id = scope_owner_user_id;
        self
    }

    /// 校验订单绑定必需的来源、部门和负责人事实。
    /// # 参数
    /// document_type 为当前强业务主体类型。
    /// # 返回
    /// 非订单类型不增加订单约束，订单事实完整时成功。
    /// # 错误
    /// 订单来源、部门或负责人缺失时拒绝绑定；负责人未覆盖时沿用创建人。
    pub fn ensure_order_facts(&self, document_type: DocumentType) -> Result<()> {
        if OrderTaskSource::approval_kind(document_type).is_some()
            && (self.business_org_unit_id.as_deref().is_none_or(str::is_empty)
                || self.order_source.is_none()
                || self.scope_owner_user_id.as_deref().unwrap_or(&self.creator_id).is_empty())
        {
            return Err(Error::ValidationError("订单审批绑定缺少内部业务部门".into()));
        }
        Ok(())
    }
}

/// 按合同政策构造必须审批类型的适配器规格。
///
/// # 参数
/// * `document_type` - 固定单据类型
///
/// # 返回
/// 返回完整规格。
///
/// # 错误
/// `NO_APPROVAL` 类型不得注册空适配器。
pub fn adapter_spec_of(document_type: DocumentType) -> Result<ApprovalAdapterSpec> {
    let policy = require_process_required(document_type)?;
    spec_from_policy(&policy)
}

/// 由已校验政策填充适配器规格。
///
/// # 参数
/// * `policy` - 必须审批政策
///
/// # 返回
/// 返回完整的 `ApprovalAdapterSpec`。
///
/// # 错误
/// 三类动作未注册或相同、快照或角色缺失时返回内部错误或 `ApprovalPolicyNotRegistered`。
pub fn spec_from_policy(policy: &ProcessRequiredApprovalPolicy) -> Result<ApprovalAdapterSpec> {
    super::policy::ensure_actions_registered(policy)?;
    if policy.subject_snapshot_fields.is_empty() || policy.work_item_owner_role.as_str().is_empty() {
        return Err(Error::Internal("审批适配器规格不完整".to_string()));
    }
    Ok(ApprovalAdapterSpec {
        document_type: policy.document_type,
        process_kind: policy.process_kind,
        subject_version_source: policy.subject_version_source,
        subject_snapshot_fields: policy.subject_snapshot_fields,
        on_approval_start: policy.start_action,
        on_final_approve: policy.final_approve_action,
        on_reject: policy.on_reject,
        cancel_action: policy.cancel_action,
        owner_role: policy.work_item_owner_role,
        owner_organization_source: policy.owner_organization_source,
        read_scope: AdapterReadScope::DocumentOrganizationAndCreator,
    })
}

/// 证明规格声明了合同要求的全部适配器字段。
///
/// # 参数
/// * `spec` - 待证明的适配器规格
///
/// # 返回
/// 字段完整且三类动作互异时无返回值。
///
/// # 错误
/// 快照字段或责任角色为空、三类动作不互异，或流程种类与单据类型不一致时返回内部错误。
pub fn ensure_adapter_spec_complete(spec: &ApprovalAdapterSpec) -> Result<()> {
    if spec.subject_snapshot_fields.is_empty()
        || spec.owner_role.as_str().is_empty()
        || !ApprovalDomainAction::are_distinct(
            spec.on_approval_start,
            spec.on_final_approve,
            spec.cancel_action,
        )
        || [spec.on_approval_start, spec.on_final_approve, spec.cancel_action].contains(&spec.on_reject)
        || spec.on_reject.document_type() != spec.document_type
        || spec.process_kind != process_kind_of(spec.document_type)
    {
        return Err(Error::Internal("审批适配器规格不完整".to_string()));
    }
    match spec.read_scope {
        AdapterReadScope::DocumentOrganizationAndCreator => {},
    }
    match spec.owner_organization_source {
        OwnerOrganizationSource::SubjectSnapshotResponsibleOrgId => {},
    }
    Ok(())
}

/// 确认该单据类型已注册审批政策。
///
/// # 参数
/// * `document_type` - 固定单据类型
///
/// # 返回
/// `NO_APPROVAL` 与 `PROCESS_REQUIRED` 均通过时无返回值。
///
/// # 错误
/// `policy_of` 构造失败时传播其错误。
pub fn ensure_runtime_cut_over(document_type: DocumentType) -> Result<()> {
    match policy_of(document_type)? {
        DocumentApprovalPolicy::NoApproval(_) | DocumentApprovalPolicy::ProcessRequired(_) => Ok(()),
    }
}

/// 按政策动作进入目标运行时。
///
/// # 参数
/// * `document_type` - 固定单据类型
/// * `action` - 合同签署的强类型领域动作
///
/// # 返回
/// 不返回成功。动作属于该类型时仍因领域端口未绑定而失败。
///
/// # 错误
/// 政策或规格构造失败、动作不属于该类型，或领域端口尚未绑定时返回错误。
pub fn execute_policy_domain_action(document_type: DocumentType, action: ApprovalDomainAction) -> Result<()> {
    ensure_runtime_cut_over(document_type)?;
    let spec = adapter_spec_of(document_type)?;
    if spec.on_approval_start != action && spec.on_final_approve != action && spec.cancel_action != action {
        return Err(Error::ValidationError(format!(
            "动作 {} 不属于 {}",
            action.as_str(),
            document_type.label()
        )));
    }
    Err(Error::BusinessLogicError(format!("审批领域动作 {} 尚未绑定，已按安全策略拒绝推进", action.as_str())))
}

/// 按单据政策校验创建人/提交人与指定审批人的岗位分离。
///
/// # 参数
/// * `policy` - 岗位分离策略
/// * `creator_id` - 创建人或提交人
/// * `assignee_ids` - 指定审批人
///
/// # 返回
/// 政策允许重合或创建人不在审批人集合中时无返回值。
///
/// # 错误
/// 政策禁止重合且创建人出现在审批人集合时返回校验错误。
pub fn ensure_separation_of_duties(
    policy: SeparationOfDutiesPolicy,
    creator_id: &str,
    assignee_ids: &[String],
) -> Result<()> {
    match policy {
        SeparationOfDutiesPolicy::AllowSubmitterAsApprover => return Ok(()),
        SeparationOfDutiesPolicy::ForbidSubmitterAsApprover => {},
    }
    if assignee_ids.iter().any(|assignee| assignee == creator_id) {
        return Err(Error::ValidationError("提交人不得审批自己的单据".to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn order_binding_requires_source_department_and_owner() {
        let empty = BindingRevalidationContext::new("settlement-party".into(), "creator".into());
        assert!(empty.ensure_order_facts(DocumentType::SalesOrder).is_err());
        assert!(empty.ensure_order_facts(DocumentType::StockAdjustment).is_ok());
        assert!(empty.ensure_order_facts(DocumentType::CustomerReceipt).is_ok());
        let complete = empty
            .with_order_source(Some(OrderTaskSource::Sales("order".into())))
            .with_business_org_unit_id(Some("department".into()));
        assert!(complete.ensure_order_facts(DocumentType::SalesOrder).is_ok());
        assert!(
            complete
                .clone()
                .with_business_org_unit_id(Some(String::new()))
                .ensure_order_facts(DocumentType::SalesOrder)
                .is_err()
        );
        assert!(
            complete.clone().with_order_source(None).ensure_order_facts(DocumentType::SalesOrder).is_err()
        );
        assert!(
            complete
                .with_scope_owner_user_id(Some(String::new()))
                .ensure_order_facts(DocumentType::SalesOrder)
                .is_err()
        );
    }

    use super::*;
    use crate::service::approval::policy::{ALL_DOCUMENT_TYPES, policy_of};

    /// 13 个必须审批类型的适配器规格完整，9 个无审批类型不得注册空适配器。
    #[test]
    fn adapter_registry_is_complete_and_no_approval_has_no_adapter() {
        let mut required = 0;
        let mut no_approval = 0;
        for document_type in ALL_DOCUMENT_TYPES {
            match policy_of(document_type).expect("政策必须存在") {
                DocumentApprovalPolicy::ProcessRequired(_) => {
                    required += 1;
                    let spec = adapter_spec_of(document_type).expect("必须审批类型必须有适配器");
                    ensure_adapter_spec_complete(&spec).expect("适配器字段必须完整");
                    assert_eq!(spec.document_type, document_type);
                    assert_eq!(spec.process_kind, process_kind_of(document_type));
                },
                DocumentApprovalPolicy::NoApproval(_) => {
                    no_approval += 1;
                    assert!(adapter_spec_of(document_type).is_err());
                },
            }
        }
        assert_eq!(required, 13);
        assert_eq!(no_approval, 9);
    }

    /// 全部固定类型均进入目标运行时，未知种类失败关闭。
    #[test]
    fn all_fixed_types_enter_target_runtime() {
        assert!(ensure_runtime_cut_over(DocumentType::StockAdjustment).is_ok());
        assert!(ensure_runtime_cut_over(DocumentType::SalesOrder).is_ok());
        assert!(ensure_runtime_cut_over(DocumentType::Delivery).is_ok());
        assert!(crate::entity::approval_integration::document_type_from_subject_kind("unknown").is_err());
        let sales = execute_policy_domain_action(
            DocumentType::SalesOrder,
            ApprovalDomainAction::SalesOrderStartApprovalSubmission,
        )
        .unwrap_err();
        assert!(matches!(sales, Error::BusinessLogicError(_)));
        let pilot = execute_policy_domain_action(
            DocumentType::StockAdjustment,
            ApprovalDomainAction::StockAdjustmentSubmit,
        )
        .unwrap_err();
        assert!(matches!(pilot, Error::BusinessLogicError(_)));
    }

    /// 提交人不得出现在指定审批人中。
    #[test]
    fn separation_of_duties_forbids_submitter_as_approver() {
        let policy = SeparationOfDutiesPolicy::ForbidSubmitterAsApprover;
        assert!(ensure_separation_of_duties(policy, "u1", &["u2".into()]).is_ok());
        assert!(ensure_separation_of_duties(policy, "u1", &["u1".into()]).is_err());
    }

    /// 销售类单据允许提交人位于单人或或签审批集合；其他审批类型继续拒绝。
    #[test]
    fn registered_policies_enforce_submitter_separation() {
        for document_type in ALL_DOCUMENT_TYPES {
            let DocumentApprovalPolicy::ProcessRequired(policy) = policy_of(document_type).unwrap() else {
                continue;
            };
            let allows_submitter = matches!(
                document_type,
                DocumentType::SalesOrder | DocumentType::VoucherSalesOrder | DocumentType::SalesChangeOrder
            );
            for assignees in [vec!["leader".into()], vec!["other".into(), "leader".into()]] {
                let result =
                    ensure_separation_of_duties(policy.separation_of_duties_policy, "leader", &assignees);
                if allows_submitter {
                    result.expect("销售领导可以审批自己提交的销售单");
                } else {
                    assert!(matches!(result, Err(Error::ValidationError(message))
                        if message == "提交人不得审批自己的单据"));
                }
            }
            assert!(
                ensure_separation_of_duties(policy.separation_of_duties_policy, "sales", &["leader".into()],)
                    .is_ok()
            );
        }
    }
}
