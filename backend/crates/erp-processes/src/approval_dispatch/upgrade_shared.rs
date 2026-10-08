//! 审批升级共享校验：身份、单据类型与初始状态门禁。
//!
//! 本模块收拢 `build_facts` 与各 `ensure_*_state` 纯判定；`load` 与 Fresh
//! 门禁的按单据实现见 [`upgrade_documents`]，分类表与入口见 [`upgrade_subject`]。

use erp_procurement::entity::purchase_order::{PurchaseChangeOrderStatus, PurchaseOrderStatus};
use erp_sales::entity::sales_order::{BusinessType, CommercialStatus, ReviewStatus};
use erp_sales::entity::sales_review::SalesChangeOrderStatus;
use erp_workflow::entity::document_registry::DocumentType;

use super::upgrade_subject::ApprovalUpgradeSubjectFacts;
use crate::{Error, Result};

/// 核对 Fresh 分支重读到的主键和版本仍与已加载事实一致。
///
/// # 参数
/// * `facts` - 先前加载的强业务事实。
/// * `actual_id` - 本次重读到的主键。
/// * `actual_version` - 本次重读到的版本。
///
/// # 返回
/// 主键与版本都一致时返回。
///
/// # 错误
/// 主键不一致时返回内部错误；版本变化时返回冲突。
pub(crate) fn ensure_fresh_subject_identity(
    facts: &ApprovalUpgradeSubjectFacts,
    actual_id: &str,
    actual_version: u64,
) -> Result<()> {
    if facts.document_id != actual_id {
        return Err(Error::Internal("Fresh 强业务对象主键不一致".to_string()));
    }
    if facts.business_object_version != actual_version {
        return Err(Error::ConflictError("强业务对象版本已变化，请刷新后重试".to_string()));
    }
    Ok(())
}

/// 拒绝空主键和未修剪主键，并要求能构成主体引用。
///
/// # 参数
/// * `document_type` - 单据类型。
/// * `document_id` - 待校验主键。
///
/// # 返回
/// 主键可用时返回。
///
/// # 错误
/// 主键为空、含首尾空白，或主体引用构造失败时返回校验错误。
pub(crate) fn ensure_exact_document_id(document_type: DocumentType, document_id: &str) -> Result<()> {
    if document_id.is_empty() || document_id.trim() != document_id {
        return Err(Error::ValidationError("单据 ID 必须是非空精确主键".to_string()));
    }
    erp_workflow::entity::approval_integration::subject_ref_for(document_type, document_id)
        .map_err(|error| Error::ValidationError(error.to_string()))?;
    Ok(())
}

/// 要求销售单业务性质映射出的单据类型与请求类型相同。
///
/// # 参数
/// * `requested` - 请求单据类型。
/// * `actual` - 销售单上的业务性质。
///
/// # 返回
/// 两者一致时返回。
///
/// # 错误
/// 类型不一致时返回校验错误。
pub(crate) fn ensure_sales_document_type(requested: DocumentType, actual: BusinessType) -> Result<()> {
    let actual = crate::approval_dispatch::sales_subject::document_type_of_sales_business(actual);
    if actual != requested {
        return Err(Error::ValidationError(format!(
            "请求单据类型 {} 与销售单业务性质对应类型 {} 不一致",
            requested.as_str(),
            actual.as_str()
        )));
    }
    Ok(())
}

/// 要求销售业务性质只映射到销售单或卡券销售单。
///
/// # 参数
/// * `actual` - 来源销售单业务性质。
///
/// # 返回
/// 映射为上述两种类型时返回。
///
/// # 错误
/// 映射到其他类型时返回内部错误。
pub(crate) fn ensure_known_sales_business_type(actual: BusinessType) -> Result<()> {
    match crate::approval_dispatch::sales_subject::document_type_of_sales_business(actual) {
        DocumentType::SalesOrder | DocumentType::VoucherSalesOrder => Ok(()),
        _ => Err(Error::Internal("销售单业务性质映射不完整".to_string())),
    }
}

/// 要求来源销售单是实物及服务销售。
///
/// # 参数
/// * `actual` - 来源销售单业务性质。
/// * `target` - 正在加载的目标单据类型，只用于错误文案。
///
/// # 返回
/// 来源为实物及服务时返回。
///
/// # 错误
/// 来源不是实物及服务时返回校验错误。
pub(crate) fn ensure_goods_service_source(actual: BusinessType, target: DocumentType) -> Result<()> {
    if actual != BusinessType::GoodsService {
        return Err(Error::ValidationError(format!("{} 的来源销售单必须是实物及服务销售单", target.label())));
    }
    Ok(())
}

/// 要求销售单商业、审核和稳定状态都是草稿，且没有当前修订。
///
/// # 参数
/// * `order` - 已加载的销售单。
///
/// # 返回
/// 仍是初始草稿时返回。
///
/// # 错误
/// 任一状态已离开草稿或已有当前修订时返回已提交冲突。
pub(crate) fn ensure_initial_sales_order_state(
    order: &erp_sales::entity::sales_order::SalesOrder,
) -> Result<()> {
    if order.commercial_status != CommercialStatus::Draft
        || order.review_status != ReviewStatus::NotSubmitted
        || order.stable.status != CommercialStatus::Draft
        || order.stable.current_revision_id.is_some()
    {
        return Err(already_submitted(
            crate::approval_dispatch::sales_subject::document_type_of_sales_business(order.business_type),
        ));
    }
    Ok(())
}

/// 要求销售变更单仍是草稿，且没有提交、目标摘要或生效修订。
///
/// # 参数
/// * `change` - 已加载的销售变更单。
///
/// # 返回
/// 仍是初始草稿时返回。
///
/// # 错误
/// 状态已离开草稿，或已有提交、目标摘要、生效修订时返回已提交冲突。
pub(crate) fn ensure_initial_sales_change_state(
    change: &erp_sales::entity::sales_review::SalesChangeOrder,
) -> Result<()> {
    if change.stable.status != SalesChangeOrderStatus::Draft
        || change.current_submission_id.is_some()
        || change.target_content_hash.is_some()
        || change.effective_revision_id.is_some()
    {
        return Err(already_submitted(DocumentType::SalesChangeOrder));
    }
    Ok(())
}

/// 要求采购单仍是草稿，审批主体版本为 0，且没有提交或当前修订。
///
/// # 参数
/// * `order` - 已加载的采购单。
///
/// # 返回
/// 仍是初始草稿时返回。
///
/// # 错误
/// 状态、审批主体版本、提交或当前修订任一不满足时返回已提交冲突。
pub(crate) fn ensure_initial_purchase_state(
    order: &erp_procurement::entity::purchase_order::PurchaseOrder,
) -> Result<()> {
    if order.stable.status != PurchaseOrderStatus::Draft
        || order.approval_subject_version != 0
        || order.current_submission_id.is_some()
        || order.stable.current_revision_id.is_some()
    {
        return Err(already_submitted(DocumentType::PurchaseOrder));
    }
    Ok(())
}

/// 要求采购变更单仍是草稿，审批主体版本为 0，且没有提交、目标摘要或生效修订。
///
/// # 参数
/// * `change` - 已加载的采购变更单。
///
/// # 返回
/// 仍是初始草稿时返回。
///
/// # 错误
/// 状态、审批主体版本、提交、目标摘要或生效修订任一不满足时返回已提交冲突。
pub(crate) fn ensure_initial_purchase_change_state(
    change: &erp_procurement::entity::purchase_order::PurchaseChangeOrder,
) -> Result<()> {
    if change.stable.status != PurchaseChangeOrderStatus::Draft
        || change.approval_subject_version != 0
        || change.current_submission_id.is_some()
        || change.target_content_hash.is_some()
        || change.effective_revision_id.is_some()
    {
        return Err(already_submitted(DocumentType::PurchaseChangeOrder));
    }
    Ok(())
}

/// 用强实体字段组装升级事实，不读取注册投影。
///
/// # 参数
/// * `document_type` - 已核验的单据类型。
/// * `requested_id` - 查询使用的主键。
/// * `actual_id` - 实体主键。
/// * `business_object_version` - 实体版本。
/// * `document_no` - 正式单号；没有时为空字符串。
/// * `responsible_org_id` - 责任组织，或固定父链给出的组织。
/// * `creator_id` - 不可变创建人。
///
/// # 返回
/// 返回字段完整的强业务事实。
///
/// # 错误
/// 查询主键与实体主键不一致、版本为 0，或责任组织、创建人空白或未修剪时返回内部错误。
pub(crate) fn build_facts(
    document_type: DocumentType,
    requested_id: &str,
    actual_id: &str,
    business_object_version: u64,
    document_no: String,
    responsible_org_id: &str,
    creator_id: &str,
) -> Result<ApprovalUpgradeSubjectFacts> {
    if requested_id != actual_id {
        return Err(Error::Internal("强业务对象主键与查询主键不一致".to_string()));
    }
    if business_object_version == 0 {
        return Err(Error::Internal("强业务对象版本非法".to_string()));
    }
    ensure_exact_nonempty_fact(responsible_org_id, "强业务对象责任组织缺失或非法")?;
    ensure_exact_nonempty_fact(creator_id, "强业务对象不可变创建人缺失或非法")?;
    Ok(ApprovalUpgradeSubjectFacts {
        document_type,
        document_id: actual_id.to_string(),
        business_object_version,
        document_no,
        responsible_org_id: responsible_org_id.to_string(),
        creator_id: creator_id.to_string(),
    })
}

/// 要求事实字段非空且不含首尾空白。
///
/// # 参数
/// * `value` - 待校验字段。
/// * `message` - 失败时使用的内部错误文案。
///
/// # 返回
/// 字段可用时返回。
///
/// # 错误
/// 字段为空或含首尾空白时返回内部错误。
pub(crate) fn ensure_exact_nonempty_fact(value: &str, message: &str) -> Result<()> {
    if value.is_empty() || value.trim() != value {
        return Err(Error::Internal(message.to_string()));
    }
    Ok(())
}

/// 构造“不是从未提交的初始草稿”冲突错误。
///
/// # 参数
/// * `document_type` - 用于文案的单据类型。
///
/// # 返回
/// 返回冲突错误。
///
/// # 错误
/// 不返回错误。
pub(crate) fn already_submitted(document_type: DocumentType) -> Error {
    Error::ConflictError(format!("{}不是从未提交审批的初始草稿，不能升级绑定", document_type.label()))
}
