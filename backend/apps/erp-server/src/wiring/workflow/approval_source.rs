//! 审批管理沿真实业务来源授权；任务参与者不调用普通业务范围。

use std::collections::BTreeSet;

use application_core::AuditActor;
use erp_core::ids::{CustomerRefundId, PaymentReversalId, ReceiptReversalId, SupplierRefundId};
use erp_finance::repository::{PayableExt, ReceivableExt};
use erp_identity::{Permission, SharedRbacService};
use erp_inventory::authorization::require_management_source;
use erp_returns::repository::ReturnsExt;
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::ports::OrderTaskSource;
use erp_workflow::service::approval::policy::require_process_required;
use mongodb::Database;
use persistence_core::Executor;

use super::order_access;
use crate::wiring::errors::{Error, Result};
use crate::wiring::inventory::inventory_service;
use crate::wiring::{funds_access_with_rbac, purchase_access};

/// 管理访问必须覆盖完整来源对象；缺失来源和未接入类型一律拒绝。
///
/// # 参数
/// * `db` - 业务单据集合所在数据库。
/// * `rbac` - 现有 RBAC 快照服务。
/// * `actor` - 当前操作人。
/// * `kind` - 审批单据类型。
/// * `id` - 单据编号。
/// * `executor` - 调用方执行器。
///
/// # 返回
/// 来源详情可读时返回 `true`；类型未接入、来源缺失或无权限时返回 `false`。
///
/// # 错误
/// 订单来源、库存或资金来源查询中的非拒绝类错误原样返回。
pub(super) async fn readable(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    kind: DocumentType,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<bool> {
    if OrderTaskSource::approval_kind(kind).is_some() {
        return Ok(order_access::approval_readable(db, rbac, actor, kind, id, executor).await?);
    }
    if kind == DocumentType::StockAdjustment {
        return stock_readable(db, rbac, actor, id, executor).await;
    }
    let Some((resource, source_id)) = finance_source(db, kind, id, executor).await? else {
        return Ok(false);
    };
    let purchase = purchase_access(db.clone(), rbac.clone());
    match funds_access_with_rbac(db.clone(), rbac.clone())
        .source_document_readable(actor, resource, &source_id, Some(&purchase), executor)
        .await
        .map_err(Error::from)
    {
        Err(Error::Forbidden(_) | Error::NotFound(_)) => Ok(false),
        result => result,
    }
}

/// 库存管理读取复用真实库存调整仓库范围。
async fn stock_readable(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<bool> {
    match inventory_service(db.clone(), rbac.clone())
        .readable_stock_adjustment(id, actor, executor)
        .await
        .map_err(Error::from)
    {
        Ok(_) => Ok(true),
        Err(Error::Forbidden(_) | Error::NotFound(_)) => Ok(false),
        Err(error) => Err(error),
    }
}

/// 按强外键取得财务源，结算主体身份不得代替来源授权。
async fn finance_source(
    db: &Database,
    kind: DocumentType,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<Option<(&'static str, String)>> {
    match kind {
        DocumentType::CustomerReceipt => Ok(Some(("customer_receipt", id.into()))),
        DocumentType::SalesInvoiceRequest => Ok(Some(("sales_invoice_request", id.into()))),
        DocumentType::SupplierPayment => Ok(Some(("supplier_payment", id.into()))),
        DocumentType::CustomerRefund => customer_refund_source(db, id, executor).await,
        DocumentType::SupplierRefund => supplier_refund_source(db, id, executor).await,
        DocumentType::ReceiptReversal => Ok(db
            .receipt_reversals()
            .find_by_id(&ReceiptReversalId::new(id), executor)
            .await?
            .map(|row| ("customer_receipt", row.original_customer_receipt_id.to_string()))),
        DocumentType::PaymentReversal => Ok(db
            .payment_reversals()
            .find_by_id(&PaymentReversalId::new(id), executor)
            .await?
            .map(|row| ("supplier_payment", row.original_supplier_payment_id.to_string()))),
        _ => Ok(None),
    }
}

/// 客户退款严格选择原回款或应收分录的真实子账。
async fn customer_refund_source(
    db: &Database,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<Option<(&'static str, String)>> {
    let Some(row) = db.customer_refunds().find_by_id(&CustomerRefundId::new(id), executor).await? else {
        return Ok(None);
    };
    match (row.original_receipt_id, row.original_receivable_entry_id) {
        (Some(id), None) => Ok(Some(("customer_receipt", id.to_string()))),
        (None, Some(id)) => Ok(db
            .receivable_entries()
            .find_by_id(&id, executor)
            .await?
            .map(|entry| ("receivable_account", entry.receivable_account_id.to_string()))),
        _ => Ok(None),
    }
}

/// 供应商退款沿原付款或应付分录授权，采购与结算来源由资金读取器识别。
async fn supplier_refund_source(
    db: &Database,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<Option<(&'static str, String)>> {
    let Some(row) = db.supplier_refunds().find_by_id(&SupplierRefundId::new(id), executor).await? else {
        return Ok(None);
    };
    match (row.original_payment_id, row.original_payable_entry_id) {
        (Some(id), None) => Ok(Some(("supplier_payment", id.to_string()))),
        (None, Some(id)) => Ok(db
            .payable_entries()
            .find_by_id(&id, executor)
            .await?
            .map(|entry| ("payable_account", entry.payable_account_id.to_string()))),
        _ => Ok(None),
    }
}

/// 以固定操作、同角色完整资格及完整真实来源执行一次统一授权。
/// # 参数
/// operation 中的入口和完整必需权限来自服务端审批政策；document目标与executor来自当前业务事务。
/// # 返回
/// 所有固定资格与来源数据范围同时成立时成功。
/// # 错误
/// 未登记组合、缺失来源及授权错误拒绝。
pub(super) async fn require_management(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    operation: (&Permission, &[Permission]),
    kind: DocumentType,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let (entry, required) = operation;
    validate_management(entry, required, kind)?;
    if OrderTaskSource::approval_kind(kind).is_some() {
        return Ok(order_access::require_management(db, rbac, actor, operation, kind, id, executor).await?);
    }
    if kind == DocumentType::StockAdjustment {
        return Ok(
            require_management_source(db.clone(), rbac.clone(), actor, id, entry, required, executor).await?
        );
    }
    let (resource, source_id) = finance_source(db, kind, id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("审批财务来源不存在".into()))?;
    Ok(funds_access_with_rbac(db.clone(), rbac.clone())
        .require_source_operation(actor, resource, &source_id, entry, required, executor)
        .await?)
}

/// 仅接受当前类型政策声明的固定完整管理组合。
fn validate_management(entry: &Permission, required: &[Permission], kind: DocumentType) -> Result<()> {
    if entry.resource() != "approval_instance"
        || !matches!(entry.action(), "read" | "resume" | "cancel" | "cancel_blocked" | "upgrade_binding")
    {
        return Err(Error::ValidationError("未登记审批管理入口".into()));
    }
    let policy = require_process_required(kind)?;
    let admin = if entry.action() == "upgrade_binding" {
        policy.definition_admin_permission
    } else {
        policy.runtime_admin_permission
    };
    let expected =
        [entry.to_string(), "approval_instance:read".into(), admin].into_iter().collect::<BTreeSet<_>>();
    let actual = required.iter().map(ToString::to_string).chain([entry.to_string()]).collect::<BTreeSet<_>>();
    if actual != expected {
        return Err(Error::ValidationError("审批类型与完整管理权限组合不匹配".into()));
    }
    Ok(())
}

#[cfg(test)]
mod management_contract_tests {
    use super::*;

    #[test]
    fn management_entry_requires_the_complete_policy_of_the_actual_type() {
        let entry = Permission::parse("approval_instance:cancel").unwrap();
        let required = ["approval_instance:read", "stock_adjustment:approval_runtime_admin"]
            .into_iter()
            .map(|code| Permission::parse(code).unwrap())
            .collect::<Vec<_>>();
        assert!(validate_management(&entry, &required, DocumentType::StockAdjustment).is_ok());
        assert!(validate_management(&entry, &required[..1], DocumentType::StockAdjustment).is_err());
        assert!(validate_management(&entry, &required, DocumentType::SalesOrder).is_err());
    }

    #[test]
    fn binding_upgrade_requires_definition_management_in_the_same_registration() {
        let entry = Permission::parse("approval_instance:upgrade_binding").unwrap();
        let required = ["approval_instance:read", "stock_adjustment:approval_definition_admin"]
            .into_iter()
            .map(|code| Permission::parse(code).unwrap())
            .collect::<Vec<_>>();
        assert!(validate_management(&entry, &required, DocumentType::StockAdjustment).is_ok());
        let wrong = [
            Permission::parse("approval_instance:read").unwrap(),
            Permission::parse("stock_adjustment:approval_runtime_admin").unwrap(),
        ];
        assert!(validate_management(&entry, &wrong, DocumentType::StockAdjustment).is_err());
    }
}
