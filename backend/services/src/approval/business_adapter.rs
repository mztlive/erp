//! 审批业务适配器注册与资格重验。
//!
//! 12 个 `PROCESS_REQUIRED` 类型必须登记完整规格；8 个 `NO_APPROVAL` 类型
//! 不得注册空适配器。领域动作由各 DocumentType 子阶段接线。

use bpm::model::types::ApprovalDefinitionStatus;
use bpm::{ProcessKind, SubjectRef};
use database::repository::bpm::DefinitionGraph;
use entities::access_control::{DataScope, DataScopeType};
use entities::document_registry::DocumentType;
use entities::sales_order::BusinessType;

use crate::errors::{Error, Result};

use super::policy::{
    policy_of, require_process_required, ApprovalDomainAction, ApprovalRequirement,
    ApprovalSubjectSnapshotField, ApprovalSubjectVersionSource, DocumentApprovalPolicy,
    OwnerOrganizationSource, ProcessRequiredApprovalPolicy, SeparationOfDutiesPolicy, WorkItemOwnerRole,
    ALL_DOCUMENT_TYPES,
};
use super::process_kind::process_kind_of;

/// 未完成目标 rollout 的必须审批类型失败码。
pub const APPROVAL_DOCUMENT_TYPE_NOT_CUT_OVER: &str = "APPROVAL_DOCUMENT_TYPE_NOT_CUT_OVER";

/// 适配器读取范围：按单据责任组织重验 DataScope 与对象读取权。
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
    /// 当前单据责任组织。
    pub organization_id: String,
    /// 单据创建人或提交人。
    pub creator_id: String,
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
        AdapterReadScope::DocumentOrganizationAndCreator => {}
    }
    match spec.owner_organization_source {
        OwnerOrganizationSource::SubjectSnapshotResponsibleOrgId => {}
    }
    Ok(())
}

/// 将销售单 `BusinessType` 穷尽分派到独立 `DocumentType`。
///
/// # 参数
/// * `business_type` - 销售单业务性质
///
/// # 返回
/// 实物及服务映射 `SalesOrder`，卡券映射 `VoucherSalesOrder`。
pub fn document_type_of_sales_business(business_type: BusinessType) -> DocumentType {
    match business_type {
        BusinessType::GoodsService => DocumentType::SalesOrder,
        BusinessType::Voucher => DocumentType::VoucherSalesOrder,
    }
}

/// 为单据类型构造唯一 `bpm::SubjectRef`。
///
/// # 参数
/// * `document_type` - 固定单据类型
/// * `business_object_id` - 业务对象主键
///
/// # 返回
/// `subject_kind` 取流程种类稳定码，`subject_id` 取业务主键。
///
/// # 错误
/// 主键为空或超长时返回校验错误。
pub fn subject_ref_for(document_type: DocumentType, business_object_id: &str) -> Result<SubjectRef> {
    SubjectRef::new(process_kind_of(document_type).as_str(), business_object_id)
        .map_err(|error| Error::ValidationError(error.to_string()))
}

/// 按销售业务性质构造唯一主体引用。
///
/// # 错误
/// 业务主键非法时返回校验错误。
pub fn subject_ref_for_sales_business(
    business_type: BusinessType,
    business_object_id: &str,
) -> Result<SubjectRef> {
    subject_ref_for(document_type_of_sales_business(business_type), business_object_id)
}

/// 由 BPM 主体种类解析已登记单据类型。
///
/// # 错误
/// 未登记种类失败关闭，不得回落默认类型。
pub fn document_type_from_subject_kind(kind: &str) -> Result<DocumentType> {
    ALL_DOCUMENT_TYPES
        .iter()
        .copied()
        .find(|document_type| document_type.as_str() == kind)
        .ok_or_else(|| Error::ValidationError(format!("未登记单据类型: {kind}")))
}

/// 目标运行时接线后，除试点外的必须审批类型不得进入新路径。
///
/// # 错误
/// 未 cut-over 的 `PROCESS_REQUIRED` 返回 `APPROVAL_DOCUMENT_TYPE_NOT_CUT_OVER`。
pub fn ensure_runtime_cut_over(document_type: DocumentType) -> Result<()> {
    match policy_of(document_type)? {
        DocumentApprovalPolicy::NoApproval(_) => Ok(()),
        DocumentApprovalPolicy::ProcessRequired(_) if document_type == DocumentType::StockAdjustment => {
            Ok(())
        }
        DocumentApprovalPolicy::ProcessRequired(_) => Err(document_type_not_cut_over()),
    }
}

/// 返回未 cut-over 的稳定冲突。
///
/// # 返回
/// 返回 `APPROVAL_DOCUMENT_TYPE_NOT_CUT_OVER`。
pub fn document_type_not_cut_over() -> Error {
    Error::ConflictError(APPROVAL_DOCUMENT_TYPE_NOT_CUT_OVER.to_string())
}

/// 按政策动作进入目标运行时。未 cut-over 类型不得回退旧运行时。
///
/// # 参数
/// * `document_type` - 固定单据类型
/// * `action` - 合同签署的强类型领域动作
///
/// # 错误
/// 未接入类型返回 `APPROVAL_DOCUMENT_TYPE_NOT_CUT_OVER`；试点动作尚未接线时失败关闭。
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
    Err(Error::BusinessLogicError(format!(
        "审批领域动作 {} 尚未绑定，已按安全策略拒绝推进",
        action.as_str()
    )))
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
        SeparationOfDutiesPolicy::ForbidSubmitterAsApprover => {}
    }
    if assignee_ids.iter().any(|assignee| assignee == creator_id) {
        return Err(Error::ValidationError("提交人不得审批自己的单据".to_string()));
    }
    Ok(())
}

/// 判断单条范围是否覆盖当前单据组织。
///
/// # 返回
/// 公司范围或组织/团队目标命中为 `true`；本人/协作不得覆盖。
fn scope_matches_organization(scope: &DataScope, organization_id: &str) -> bool {
    match scope.scope_type {
        DataScopeType::Company => true,
        DataScopeType::Organization | DataScopeType::Team => {
            scope.scope_targets.iter().any(|target| target == organization_id)
        }
        DataScopeType::SelfOwned | DataScopeType::Collaborative => false,
    }
}

/// 判断已加载范围是否覆盖当前单据组织。
///
/// 空范围不得误放行。公司范围或组织/团队命中才覆盖；本人/协作不覆盖。
///
/// # 参数
/// * `scopes` - 用户或角色已加载范围
/// * `organization_id` - 单据责任组织
///
/// # 返回
/// 至少一条范围命中时为 `true`。
pub fn data_scope_covers_organization(scopes: &[DataScope], organization_id: &str) -> bool {
    scopes
        .iter()
        .any(|scope| scope_matches_organization(scope, organization_id))
}

/// 用户范围：无单独范围时沿用角色；有范围时必须显式覆盖组织。
///
/// # 返回
/// 空用户范围返回 `true`（交由角色范围证明）；否则与角色范围相同规则。
pub fn user_scope_covers_organization(scopes: &[DataScope], organization_id: &str) -> bool {
    if scopes.is_empty() {
        return true;
    }
    data_scope_covers_organization(scopes, organization_id)
}

/// 与 Resolver 同等强度：用户覆盖且角色覆盖。双方皆空必须拒绝。
///
/// # 返回
/// 用户与角色范围同时覆盖当前组织时为 `true`。
pub fn assignment_scope_covers_organization(
    user_scopes: &[DataScope],
    role_scopes: &[DataScope],
    organization_id: &str,
) -> bool {
    user_scope_covers_organization(user_scopes, organization_id)
        && data_scope_covers_organization(role_scopes, organization_id)
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
    match spec.read_scope {
        AdapterReadScope::DocumentOrganizationAndCreator => {}
    }
    if context.organization_id.trim().is_empty() || assignee_user_id.trim().is_empty() {
        return Err(Error::ValidationError("单据组织或审批人不能为空".to_string()));
    }
    if spec.document_type == DocumentType::StockAdjustment {
        return Ok(Some(crate::inventory::stock_adjustment_object_readable(
            &context.organization_id,
            assignee_user_id,
        )?));
    }
    if spec.document_type == DocumentType::SalesOrder || spec.document_type == DocumentType::VoucherSalesOrder
    {
        return Ok(Some(crate::sales_order::sales_order_object_readable(
            &context.organization_id,
            assignee_user_id,
        )?));
    }
    if spec.document_type == DocumentType::SalesChangeOrder {
        return Ok(Some(crate::sales_review::sales_change_order_object_readable(
            &context.organization_id,
            assignee_user_id,
        )?));
    }
    if spec.document_type == DocumentType::PurchaseOrder {
        return Ok(Some(crate::purchase_order::purchase_order_object_readable(
            &context.organization_id,
            assignee_user_id,
        )?));
    }
    if spec.document_type == DocumentType::PurchaseChangeOrder {
        return Ok(Some(
            crate::purchase_order::purchase_change_order_object_readable(
                &context.organization_id,
                assignee_user_id,
            )?,
        ));
    }
    if spec.document_type == DocumentType::CustomerReceipt {
        return Ok(Some(crate::receivable::customer_receipt_object_readable(
            &context.organization_id,
            assignee_user_id,
        )?));
    }
    if spec.document_type == DocumentType::SupplierPayment {
        return Ok(Some(crate::payable::supplier_payment_object_readable(
            &context.organization_id,
            assignee_user_id,
        )?));
    }
    if spec.document_type == DocumentType::CustomerRefund {
        return Ok(Some(crate::returns::customer_refund_object_readable(
            &context.organization_id,
            assignee_user_id,
        )?));
    }
    let _ = context.creator_id.as_str();
    Ok(None)
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

/// 以用户+角色范围和显式读取权重验绑定资格。
///
/// # 错误
/// 组织未覆盖或不能读取时返回校验错误。
pub fn ensure_binding_scope(
    spec: &ApprovalAdapterSpec,
    user_scopes: &[DataScope],
    role_scopes: &[DataScope],
    organization_id: &str,
    can_read: bool,
) -> Result<()> {
    match spec.read_scope {
        AdapterReadScope::DocumentOrganizationAndCreator => {}
    }
    if !assignment_scope_covers_organization(user_scopes, role_scopes, organization_id) {
        return Err(Error::ValidationError(
            "审批人数据范围不覆盖当前单据组织".to_string(),
        ));
    }
    ensure_object_readable(can_read)
}

/// bind/upgrade 共用闸门：先证明组织覆盖，再要求 Adapter 显式读取权。
///
/// # 错误
/// 范围不足、读取权未接线或显式拒绝时返回错误。
pub fn revalidate_assignee_binding_access(
    spec: &ApprovalAdapterSpec,
    user_scopes: &[DataScope],
    role_scopes: &[DataScope],
    context: &BindingRevalidationContext,
    assignee_user_id: &str,
) -> Result<()> {
    if !assignment_scope_covers_organization(user_scopes, role_scopes, &context.organization_id) {
        return Err(Error::ValidationError(
            "审批人数据范围不覆盖当前单据组织".to_string(),
        ));
    }
    let can_read = require_wired_object_read(adapter_object_read_decision(spec, context, assignee_user_id)?)?;
    ensure_binding_scope(spec, user_scopes, role_scopes, &context.organization_id, can_read)
}

/// 重验已发布图状态，禁止绑定草稿或退役定义。
///
/// # 错误
/// 状态不是 `PUBLISHED` 时返回未配置。
pub fn ensure_published_status(status: ApprovalDefinitionStatus) -> Result<()> {
    if status == ApprovalDefinitionStatus::Published {
        return Ok(());
    }
    Err(super::binding::process_not_configured())
}

/// 从定义图提取去重后的指定审批人。
///
/// # 参数
/// * `graph` - 已加载定义图
///
/// # 返回
/// 返回排序去重后的参与人 ID。
pub fn assignee_ids_of(graph: &DefinitionGraph) -> Vec<String> {
    let mut ids = graph
        .nodes
        .iter()
        .map(|node| node.assignee_participant_id.as_str().to_string())
        .collect::<Vec<_>>();
    ids.sort();
    ids.dedup();
    ids
}

/// 绑定要求：无审批跳过，必须审批查询发布定义。
///
/// # 返回
/// 返回政策决定。
pub fn binding_requirement_of(policy: &DocumentApprovalPolicy) -> ApprovalRequirement {
    policy.requirement()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::approval::policy::policy_of;
    use entities::access_control::DataScopeSubjectType;

    /// 12 个必须审批类型的适配器规格完整，8 个无审批类型不得注册空适配器。
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
                }
                DocumentApprovalPolicy::NoApproval(_) => {
                    no_approval += 1;
                    assert!(adapter_spec_of(document_type).is_err());
                }
            }
        }
        assert_eq!(required, 12);
        assert_eq!(no_approval, 8);
    }

    /// 每个 DocumentType 都能构造唯一 SubjectRef。
    #[test]
    fn every_document_type_builds_unique_subject_ref() {
        let mut kinds = std::collections::BTreeSet::new();
        for document_type in ALL_DOCUMENT_TYPES {
            let subject = subject_ref_for(document_type, "doc-1").expect("主体引用必须可构造");
            assert_eq!(subject.subject_id(), "doc-1");
            assert_eq!(subject.subject_kind(), process_kind_of(document_type).as_str());
            assert!(kinds.insert(subject.subject_kind().to_string()));
        }
        assert_eq!(kinds.len(), 20);
    }

    /// 销售单按 BusinessType 穷尽分派到不同 DocumentType 与 ProcessKind。
    #[test]
    fn sales_business_type_dispatches_to_distinct_kinds() {
        assert_eq!(
            document_type_of_sales_business(BusinessType::GoodsService),
            DocumentType::SalesOrder
        );
        assert_eq!(
            document_type_of_sales_business(BusinessType::Voucher),
            DocumentType::VoucherSalesOrder
        );
        let sales = subject_ref_for_sales_business(BusinessType::GoodsService, "so-1").unwrap();
        let voucher = subject_ref_for_sales_business(BusinessType::Voucher, "so-1").unwrap();
        assert_ne!(sales.subject_kind(), voucher.subject_kind());
        assert_eq!(sales.subject_kind(), "sales_order");
        assert_eq!(voucher.subject_kind(), "voucher_sales_order");
    }

    /// 未 cut-over 的必须审批类型失败关闭，不得回退旧运行时。
    #[test]
    fn uncut_process_required_types_fail_closed() {
        assert!(ensure_runtime_cut_over(DocumentType::StockAdjustment).is_ok());
        let error = ensure_runtime_cut_over(DocumentType::SalesOrder).unwrap_err();
        assert_eq!(
            error.to_string(),
            format!("数据冲突: {APPROVAL_DOCUMENT_TYPE_NOT_CUT_OVER}")
        );
        assert!(ensure_runtime_cut_over(DocumentType::Delivery).is_ok());
        assert!(document_type_from_subject_kind("unknown").is_err());
        let uncut = execute_policy_domain_action(
            DocumentType::SalesOrder,
            ApprovalDomainAction::SalesOrderStartApprovalSubmission,
        )
        .unwrap_err();
        assert!(uncut.to_string().contains(APPROVAL_DOCUMENT_TYPE_NOT_CUT_OVER));
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

    /// 空范围不得误放行；角色空范围不能证明组织覆盖。
    #[test]
    fn empty_scopes_do_not_cover_organization() {
        assert!(!data_scope_covers_organization(&[], "org-1"));
        assert!(!assignment_scope_covers_organization(&[], &[], "org-1"));
        assert!(user_scope_covers_organization(&[], "org-1"));
    }

    /// 组织未命中与 SelfOwned 不得覆盖，即使角色是公司范围。
    #[test]
    fn organization_miss_and_self_owned_fail_closed() {
        let user_other = fixture_scope(
            "ds-user-other",
            DataScopeSubjectType::User,
            "u1",
            DataScopeType::Organization,
            &["org-2"],
        );
        let user_self = fixture_scope(
            "ds-user-self",
            DataScopeSubjectType::User,
            "u1",
            DataScopeType::SelfOwned,
            &[],
        );
        let role_company = fixture_scope(
            "ds-role-company",
            DataScopeSubjectType::Role,
            "role-1",
            DataScopeType::Company,
            &[],
        );
        assert!(!assignment_scope_covers_organization(
            &[user_other],
            std::slice::from_ref(&role_company),
            "org-1"
        ));
        assert!(!assignment_scope_covers_organization(
            &[user_self],
            std::slice::from_ref(&role_company),
            "org-1"
        ));
    }

    /// 读取权未接线或显式拒绝必须失败关闭。
    #[test]
    fn object_read_unwired_and_denied_fail_closed() {
        let unwired = adapter_spec_of(DocumentType::SupplierRefund).expect("未接入类型仍有规格");
        let context = BindingRevalidationContext {
            organization_id: "org-1".to_string(),
            creator_id: "creator-1".to_string(),
        };
        assert!(adapter_object_read_decision(&unwired, &context, "u1")
            .expect("未接线类型应返回 None")
            .is_none());
        let pilot = adapter_spec_of(DocumentType::StockAdjustment).expect("试点必须有适配器");
        assert_eq!(
            adapter_object_read_decision(&pilot, &context, "creator-1").expect("创建人可读"),
            Some(true)
        );
        let sales = adapter_spec_of(DocumentType::SalesOrder).expect("销售单必须有适配器");
        assert_eq!(
            adapter_object_read_decision(&sales, &context, "u1").expect("销售单读取权已接线"),
            Some(true)
        );
        let voucher = adapter_spec_of(DocumentType::VoucherSalesOrder).expect("卡券销售单必须有适配器");
        assert_eq!(
            adapter_object_read_decision(&voucher, &context, "u1").expect("卡券读取权已接线"),
            Some(true)
        );
        let change = adapter_spec_of(DocumentType::SalesChangeOrder).expect("销售变更必须有适配器");
        assert_eq!(
            adapter_object_read_decision(&change, &context, "u1").expect("销售变更读取权已接线"),
            Some(true)
        );
        let purchase = adapter_spec_of(DocumentType::PurchaseOrder).expect("采购单必须有适配器");
        assert_eq!(
            adapter_object_read_decision(&purchase, &context, "u1").expect("采购单读取权已接线"),
            Some(true)
        );
        let purchase_change =
            adapter_spec_of(DocumentType::PurchaseChangeOrder).expect("采购变更必须有适配器");
        assert_eq!(
            adapter_object_read_decision(&purchase_change, &context, "u1").expect("采购变更读取权已接线"),
            Some(true)
        );
        let receipt = adapter_spec_of(DocumentType::CustomerReceipt).expect("客户回款必须有适配器");
        assert_eq!(
            adapter_object_read_decision(&receipt, &context, "u1").expect("客户回款读取权已接线"),
            Some(true)
        );
        let refund = adapter_spec_of(DocumentType::CustomerRefund).expect("客户退款必须有适配器");
        assert_eq!(
            adapter_object_read_decision(&refund, &context, "u1").expect("客户退款读取权已接线"),
            Some(true)
        );
        let payment = adapter_spec_of(DocumentType::SupplierPayment).expect("供应商付款必须有适配器");
        assert_eq!(
            adapter_object_read_decision(&payment, &context, "u1").expect("供应商付款读取权已接线"),
            Some(true)
        );
        assert_eq!(
            adapter_object_read_decision(&pilot, &context, "u1").expect("组织上下文已给出"),
            Some(true)
        );
        assert!(require_wired_object_read(None).is_err());
        assert!(ensure_object_readable(false).is_err());
    }

    /// bind/upgrade 共用闸门会走到组织未覆盖、空范围和读取权拒绝。
    #[test]
    fn bind_upgrade_gate_hits_scope_and_read_failures() {
        let spec = adapter_spec_of(DocumentType::StockAdjustment).expect("试点必须有适配器");
        let context = BindingRevalidationContext {
            organization_id: "org-1".to_string(),
            creator_id: "creator-1".to_string(),
        };
        let empty = revalidate_assignee_binding_access(&spec, &[], &[], &context, "u1").unwrap_err();
        assert!(empty.to_string().contains("数据范围不覆盖当前单据组织"));

        let role_company = fixture_scope(
            "ds-role",
            DataScopeSubjectType::Role,
            "role-1",
            DataScopeType::Company,
            &[],
        );
        let user_org = fixture_scope(
            "ds-user",
            DataScopeSubjectType::User,
            "u1",
            DataScopeType::Organization,
            &["org-1"],
        );
        revalidate_assignee_binding_access(
            &spec,
            std::slice::from_ref(&user_org),
            std::slice::from_ref(&role_company),
            &context,
            "u1",
        )
        .expect("试点读取权已接线且组织覆盖时应通过");
        let sales = adapter_spec_of(DocumentType::SupplierRefund).expect("未接入类型");
        let unwired = revalidate_assignee_binding_access(
            &sales,
            std::slice::from_ref(&user_org),
            std::slice::from_ref(&role_company),
            &context,
            "u1",
        )
        .unwrap_err();
        assert!(unwired.to_string().contains("对象读取权未接线"));
        assert!(ensure_binding_scope(
            &spec,
            std::slice::from_ref(&user_org),
            std::slice::from_ref(&role_company),
            "org-1",
            false
        )
        .is_err());
    }

    fn fixture_scope(
        id: &str,
        subject_type: DataScopeSubjectType,
        subject_id: &str,
        scope_type: DataScopeType,
        targets: &[&str],
    ) -> DataScope {
        DataScope::new(
            entities::ids::DataScopeId::new(id),
            entities::access_control::DataScopeData {
                subject_type,
                subject_id: subject_id.to_string(),
                scope_type,
                scope_targets: targets.iter().map(|target| (*target).to_string()).collect(),
            },
        )
        .expect("范围夹具必须可构造")
    }
}
