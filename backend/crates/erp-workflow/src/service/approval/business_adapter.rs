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
use crate::ports::{OrderTaskSource, WorkflowScopeObject};

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
    /// 销售订单当前客户，用于独立详情范围的协作事实。
    pub customer_id: Option<String>,
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
            customer_id: None,
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

    /// 设置销售订单当前客户。
    ///
    /// # 参数
    /// * `customer_id` - 销售订单当前客户
    ///
    /// # 返回
    /// 返回更新后的上下文。
    ///
    /// # 错误
    /// 无。
    pub fn with_customer_id(mut self, customer_id: Option<String>) -> Self {
        self.customer_id = customer_id;
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

    pub fn scope_object(&self, document_type: DocumentType) -> WorkflowScopeObject {
        let order = OrderTaskSource::approval_kind(document_type).is_some();
        WorkflowScopeObject {
            order_source: order.then(|| self.order_source.clone()).flatten(),
            customer_id: order.then(|| self.customer_id.clone()).flatten(),
            owner_user_id: self.scope_owner_user_id.clone().unwrap_or_else(|| self.creator_id.clone()),
            business_org_unit_id: order.then(|| self.business_org_unit_id.clone()).flatten(),
            warehouse_id: (document_type == DocumentType::StockAdjustment)
                .then(|| self.organization_id.clone()),
            settlement_party_id: (!order && document_type != DocumentType::StockAdjustment)
                .then(|| self.organization_id.clone()),
        }
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
/// # 错误
/// 三类动作未注册或相同、快照/角色缺失时返回部署不变量错误。
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
        cancel_action: policy.cancel_action,
        owner_role: policy.work_item_owner_role,
        owner_organization_source: policy.owner_organization_source,
        read_scope: AdapterReadScope::DocumentOrganizationAndCreator,
    })
}

/// 证明规格声明了合同要求的全部适配器字段。
///
/// # 错误
/// 任一字段缺失或三类动作不互异时返回错误。
pub fn ensure_adapter_spec_complete(spec: &ApprovalAdapterSpec) -> Result<()> {
    if spec.subject_snapshot_fields.is_empty()
        || spec.owner_role.as_str().is_empty()
        || spec.on_approval_start == spec.on_final_approve
        || spec.on_approval_start == spec.cancel_action
        || spec.on_final_approve == spec.cancel_action
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

/// 全部固定单据类型均已切入目标运行时。
///
/// # 错误
/// 政策缺失时返回部署不变量错误。
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
/// # 错误
/// 动作不属于该类型或领域端口尚未绑定时失败关闭。
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

/// 岗位分离：禁止创建人/提交人担任指定审批人。
///
/// # 错误
/// 创建人出现在审批人集合时返回校验错误。
pub fn ensure_separation_of_duties(
    policy: SeparationOfDutiesPolicy,
    creator_id: &str,
    assignee_ids: &[String],
) -> Result<()> {
    match policy {
        SeparationOfDutiesPolicy::ForbidSubmitterAsApprover => {},
    }
    if assignee_ids.iter().any(|assignee| assignee == creator_id) {
        return Err(Error::ValidationError("提交人不得审批自己的单据".to_string()));
    }
    Ok(())
}

/// 领域 Adapter 按单据组织/创建人上下文给出对象读取权。
///
/// 本阶段只登记规格，不伪造读取成功；未接线返回 `None`。
///
/// # 错误
/// 组织或审批人为空时返回校验错误。
pub fn adapter_object_read_decision(
    spec: &ApprovalAdapterSpec,
    context: &BindingRevalidationContext,
    assignee_user_id: &str,
) -> Result<Option<bool>> {
    adapter_object_read_decision_with(
        spec,
        context,
        assignee_user_id,
        &crate::ports::FailClosedObjectReadPort,
    )
}

/// Domain-wired object-read decision used by approval binding.
pub fn adapter_object_read_decision_with(
    spec: &ApprovalAdapterSpec,
    context: &BindingRevalidationContext,
    assignee_user_id: &str,
    port: &dyn crate::ports::ApprovalObjectReadPort,
) -> Result<Option<bool>> {
    match spec.read_scope {
        AdapterReadScope::DocumentOrganizationAndCreator => {},
    }
    if context.organization_id.trim().is_empty() || assignee_user_id.trim().is_empty() {
        return Err(Error::ValidationError("单据组织或审批人不能为空".to_string()));
    }
    let _ = context.creator_id.as_str();
    port.object_read_decision(
        spec.document_type,
        &context.organization_id,
        &context.creator_id,
        assignee_user_id,
    )
}

/// 未接线的对象读取权必须失败关闭。
///
/// # 错误
/// `None` 表示 Adapter 未接线，禁止默认放行。
pub fn require_wired_object_read(decision: Option<bool>) -> Result<bool> {
    decision.ok_or_else(|| Error::ValidationError("对象读取权未接线，已按安全策略拒绝".to_string()))
}

/// 校验指定用户具备对象读取权。
///
/// # 错误
/// 不能读取被审对象时返回校验错误。
pub fn ensure_object_readable(can_read: bool) -> Result<()> {
    if can_read {
        return Ok(());
    }
    Err(Error::ValidationError("审批人不能读取被审单据".to_string()))
}

#[cfg(test)]
mod tests {
    #[test]
    fn binding_revalidation_context_new_defaults_options_to_none() {
        let context = super::BindingRevalidationContext::new("org-1".into(), "creator-1".into())
            .with_customer_id(Some("customer-1".into()));
        assert_eq!(context.organization_id, "org-1");
        assert_eq!(context.creator_id, "creator-1");
        assert_eq!(context.customer_id.as_deref(), Some("customer-1"));
        assert!(context.order_source.is_none() && context.business_org_unit_id.is_none());
    }

    #[test]
    fn scope_facts_keep_business_org_warehouse_and_settlement_separate() {
        let context = super::BindingRevalidationContext {
            order_source: Some(super::OrderTaskSource::Sales("order".into())),
            customer_id: Some("customer".into()),
            business_org_unit_id: Some("department".into()),
            scope_owner_user_id: Some("sales-owner".into()),
            organization_id: "responsibility-id".into(),
            creator_id: "creator".into(),
        };
        let sales = context.scope_object(super::DocumentType::SalesOrder);
        assert_eq!(sales.business_org_unit_id.as_deref(), Some("department"));
        assert_eq!(sales.owner_user_id, "sales-owner");
        assert!(sales.warehouse_id.is_none() && sales.settlement_party_id.is_none());
        let warehouse = context.scope_object(super::DocumentType::StockAdjustment);
        assert_eq!(warehouse.warehouse_id.as_deref(), Some("responsibility-id"));
        assert!(warehouse.business_org_unit_id.is_none() && warehouse.settlement_party_id.is_none());
        let finance = context.scope_object(super::DocumentType::CustomerReceipt);
        assert_eq!(finance.settlement_party_id.as_deref(), Some("responsibility-id"));
        assert!(finance.business_org_unit_id.is_none() && finance.warehouse_id.is_none());
        assert_eq!(context.organization_id, "responsibility-id");
    }

    use super::*;
    use crate::service::approval::policy::{ALL_DOCUMENT_TYPES, policy_of};

    /// 12 个必须审批类型的适配器规格完整，9 个无审批类型不得注册空适配器。
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
        assert_eq!(required, 12);
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
        assert!(ensure_object_readable(true).is_ok());
        assert!(ensure_object_readable(false).is_err());
    }

    /// 读取权未接线或显式拒绝必须失败关闭。
    #[test]
    fn object_read_unwired_and_denied_fail_closed() {
        let context = BindingRevalidationContext {
            order_source: None,
            customer_id: None,
            business_org_unit_id: None,
            scope_owner_user_id: None,
            organization_id: "org-1".to_string(),
            creator_id: "creator-1".to_string(),
        };
        let pilot = adapter_spec_of(DocumentType::StockAdjustment).expect("试点必须有适配器");
        assert_eq!(
            adapter_object_read_decision(&pilot, &context, "creator-1")
                .expect("库存读取由真实权限范围端口接线"),
            None
        );
        let sales = adapter_spec_of(DocumentType::SalesOrder).expect("销售单必须有适配器");
        assert_eq!(
            adapter_object_read_decision(&sales, &context, "u1")
                .expect("领域读取已迁出，workflow 默认失败关闭"),
            None
        );
        assert!(adapter_spec_of(DocumentType::SupplierPayment).is_err());
        let payment_reversal = adapter_spec_of(DocumentType::PaymentReversal).expect("付款冲正必须有适配器");
        assert_eq!(
            adapter_object_read_decision(&payment_reversal, &context, "u1")
                .expect("领域读取已迁出，workflow 默认失败关闭"),
            None
        );
        assert_eq!(
            adapter_object_read_decision(&pilot, &context, "u1").expect("库存读取不得恢复常量 helper"),
            None
        );
        assert!(require_wired_object_read(None).is_err());
        assert!(ensure_object_readable(false).is_err());

        struct AllowAll;
        impl crate::ports::ApprovalObjectReadPort for AllowAll {
            fn object_read_decision(
                &self,
                _document_type: DocumentType,
                _organization_id: &str,
                _creator_id: &str,
                _assignee_user_id: &str,
            ) -> Result<Option<bool>> {
                Ok(Some(true))
            }
        }
        assert_eq!(
            adapter_object_read_decision_with(&sales, &context, "u1", &AllowAll)
                .expect("注入端口后必须返回领域判定"),
            Some(true)
        );
    }
}
