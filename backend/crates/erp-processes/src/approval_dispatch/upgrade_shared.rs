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

pub(crate) fn ensure_exact_document_id(document_type: DocumentType, document_id: &str) -> Result<()> {
    if document_id.is_empty() || document_id.trim() != document_id {
        return Err(Error::ValidationError("单据 ID 必须是非空精确主键".to_string()));
    }
    erp_workflow::entity::approval_integration::subject_ref_for(document_type, document_id)
        .map_err(|error| Error::ValidationError(error.to_string()))?;
    Ok(())
}

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

pub(crate) fn ensure_known_sales_business_type(actual: BusinessType) -> Result<()> {
    match crate::approval_dispatch::sales_subject::document_type_of_sales_business(actual) {
        DocumentType::SalesOrder | DocumentType::VoucherSalesOrder => Ok(()),
        _ => Err(Error::Internal("销售单业务性质映射不完整".to_string())),
    }
}

pub(crate) fn ensure_goods_service_source(actual: BusinessType, target: DocumentType) -> Result<()> {
    if actual != BusinessType::GoodsService {
        return Err(Error::ValidationError(format!("{} 的来源销售单必须是实物及服务销售单", target.label())));
    }
    Ok(())
}

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

pub(crate) fn ensure_exact_nonempty_fact(value: &str, message: &str) -> Result<()> {
    if value.is_empty() || value.trim() != value {
        return Err(Error::Internal(message.to_string()));
    }
    Ok(())
}

pub(crate) fn already_submitted(document_type: DocumentType) -> Error {
    Error::ConflictError(format!("{}不是从未提交审批的初始草稿，不能升级绑定", document_type.label()))
}
