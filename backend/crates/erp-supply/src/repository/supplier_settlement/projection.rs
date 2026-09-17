use mongodb::bson::{Document, doc};

use super::{DIFFERENCE_SORT_FIELDS, ITEM_SORT_FIELDS, STATEMENT_SORT_FIELDS};

/// 构建结算单排序文档（白名单映射，禁止透传任意字段名）。
///
/// # 参数
/// * `sort_by` - 排序字段；`None` 或不在白名单内时默认 `created_at`
/// * `sort_ascending` - 升序为 `true`，降序为 `false`
///
/// # 返回
/// 返回排序条件文档。
pub(super) fn statement_sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    sort_doc(STATEMENT_SORT_FIELDS, sort_by, sort_ascending)
}

/// 构建结算明细排序文档（白名单映射，禁止透传任意字段名）。
///
/// # 参数
/// * `sort_by` - 排序字段；`None` 或不在白名单内时默认 `created_at`
/// * `sort_ascending` - 升序为 `true`，降序为 `false`
///
/// # 返回
/// 返回排序条件文档。
pub(super) fn item_sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    sort_doc(ITEM_SORT_FIELDS, sort_by, sort_ascending)
}

/// 构建结算差异排序文档（白名单映射，禁止透传任意字段名）。
///
/// # 参数
/// * `sort_by` - 排序字段；`None` 或不在白名单内时默认 `created_at`
/// * `sort_ascending` - 升序为 `true`，降序为 `false`
///
/// # 返回
/// 返回排序条件文档。
pub(super) fn difference_sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    sort_doc(DIFFERENCE_SORT_FIELDS, sort_by, sort_ascending)
}

/// 构建白名单排序文档。
///
/// # 参数
/// * `whitelist` - 允许的排序字段集合
/// * `sort_by` - 排序字段；`None` 或不在白名单内时默认 `created_at`
/// * `sort_ascending` - 升序为 `true`，降序为 `false`
///
/// # 返回
/// 返回排序条件文档。
pub(super) fn sort_doc(whitelist: &[&str], sort_by: Option<&str>, sort_ascending: bool) -> Document {
    let direction = if sort_ascending { 1 } else { -1 };
    let field = sort_by.filter(|field| whitelist.contains(field)).unwrap_or("created_at");
    doc! { field: direction, "id": direction }
}

/// 供应商结算单列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
pub(super) fn supplier_settlement_statement_projection() -> Document {
    doc! {
        "id": 1,
        "statement_no": 1,
        "supplier_id": 1,
        "period_start": 1,
        "period_end": 1,
        "period_policy_id": 1,
        "period_policy_version": 1,
        "period_timezone": 1,
        "external_bill_no": 1,
        "external_bill_version": 1,
        "erp_amount": 1,
        "supplier_amount": 1,
        "difference_amount": 1,
        "status": 1,
        "subject_hash": 1,
        "source_as_of": 1,
        "source_snapshot_at": 1,
        "source_snapshot_hash": 1,
        "refresh_cutoff_policy_id": 1,
        "refresh_cutoff_policy_version": 1,
        "prepared_by": 1,
        "business_org_unit_id": 1,
        "difference_handler_user_id": 1,
        "reviewed_by": 1,
        "review_result": 1,
        "review_reason_code": 1,
        "review_comment": 1,
        "reviewed_at": 1,
        "confirmed_at": 1,
        "payable_account_id": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 供应商结算明细列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
pub(super) fn supplier_settlement_item_projection() -> Document {
    doc! {
        "id": 1,
        "statement_id": 1,
        "supplier_fulfillment_order_id": 1,
        "supplier_fulfillment_item_id": 1,
        "quantity": 1,
        "order_amount": 1,
        "freight_amount": 1,
        "service_fee_amount": 1,
        "refund_amount": 1,
        "erp_calculated_amount": 1,
        "erp_calculated_net_amount": 1,
        "erp_calculated_tax_amount": 1,
        "supplier_billed_amount": 1,
        "supplier_billed_net_amount": 1,
        "supplier_billed_tax_amount": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 供应商结算差异列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
pub(super) fn supplier_settlement_difference_projection() -> Document {
    doc! {
        "id": 1,
        "statement_item_id": 1,
        "difference_type": 1,
        "difference_amount": 1,
        "status": 1,
        "resolution": 1,
        "resolved_by": 1,
        "resolved_at": 1,
        "version": 1,
        "created_at": 1,
    }
}
