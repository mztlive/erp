//! 付款按采购或供应商结算真实来源完成授权后执行数据库分页。

use erp_audit::AuditExt;
use erp_finance::dto::payable::SupplierPaymentListQuery;
use erp_finance::repository::{
    FinancialSummaryLink, FundsSummaryRepository, PayableExt, SupplierPaymentFilter, SupplierPaymentRow,
};
use erp_procurement::repository::PurchaseOrderExt;
use erp_supply::repository::SupplierSettlementExt;
use mongodb::Database;
use mongodb::bson::{Bson, Document, doc};
use persistence_core::{Executor, QueryFilter};

use super::super::{FundsAuthorization, FundsLinkedCondition};
use super::flow::{self, FlowHeader, FlowPage, FlowVersion, first, lookup};
use super::{aggregate, linked_condition_document, source_scope_document, source_stages};
use crate::{Error, Result};

/// 同一最终条件返回当前页、总数、完整窄版本和授权份额摘要。
///
/// # 参数
/// 数据库、规范化筛选、当前来源授权与调用方事务执行器。
/// # 返回
/// 页面和计数由单独分页分支生成；完整版本与金额采用同事务独立游标。
/// # 错误
/// 数据库失败、候选超限或元信息集合与计数不一致时拒绝。
pub(in crate::finance::funds_scope) async fn page(
    db: &Database,
    query: &SupplierPaymentListQuery,
    filter: &SupplierPaymentFilter,
    authorization: &FundsAuthorization,
    condition: &FundsLinkedCondition,
    executor: &mut dyn Executor,
) -> Result<FlowPage<SupplierPaymentRow>> {
    let mut stages = pipeline(filter, authorization, condition);
    stages.push(flow::facet(query.paging.page, query.paging.page_size, projection(), sort(query))?);
    let mut rows = aggregate(db.collection(Database::SUPPLIER_PAYMENTS), stages, executor).await?;
    let snapshot = rows.pop().unwrap_or_else(|| FlowPage {
        items: vec![],
        versions: vec![],
        total: vec![],
        summary: vec![],
    });
    if payment_count(&snapshot)? == 0 {
        return Ok(snapshot);
    }
    let mut stages = pipeline(filter, authorization, condition);
    stages.extend(flow::header_stages(sort(query)));
    let headers: Vec<FlowHeader> =
        aggregate(db.collection(Database::SUPPLIER_PAYMENTS), stages, executor).await?;
    let ids = headers.iter().map(|row| row.version.id.clone()).collect::<Vec<_>>();
    let links = FundsSummaryRepository::new(db).payment_links(&ids, executor).await?;
    finish_page(snapshot, headers, links)
}

/// 第二次授权只取完整责任版本，页外金额和分配视图不重新生成。
///
/// # 参数
/// 原规范化筛选、当前来源授权与调用方事务执行器。
/// # 返回
/// 按原主表排序返回全部匹配付款及获授权来源版本。
/// # 错误
/// 数据库或投影解码失败时拒绝。
pub(in crate::finance::funds_scope) async fn versions(
    db: &Database,
    query: &SupplierPaymentListQuery,
    filter: &SupplierPaymentFilter,
    authorization: &FundsAuthorization,
    condition: &FundsLinkedCondition,
    executor: &mut dyn Executor,
) -> Result<Vec<FlowVersion>> {
    let mut stages = pipeline(filter, authorization, condition);
    stages.extend(flow::recheck_stages(sort(query)));
    aggregate(db.collection(Database::SUPPLIER_PAYMENTS), stages, executor).await
}

/// 原父单金额流按同拍完整头信息归组，拒绝缺失的非当前页版本。
fn finish_page(
    mut snapshot: FlowPage<SupplierPaymentRow>,
    headers: Vec<FlowHeader>,
    links: Vec<FinancialSummaryLink>,
) -> Result<FlowPage<SupplierPaymentRow>> {
    if u64::try_from(headers.len()).unwrap_or(u64::MAX) != payment_count(&snapshot)? {
        return Err(Error::Internal("付款范围头信息与完整计数不一致".into()));
    }
    (snapshot.versions, snapshot.summary) = flow::summaries(headers, links);
    Ok(snapshot)
}

/// 付款候选上限保留该资源的原校验文案，其他错误原样传播。
fn payment_count<T>(snapshot: &FlowPage<T>) -> Result<u64> {
    snapshot.count().map_err(|error| match error {
        Error::ValidationError(_) => {
            Error::ValidationError("付款查询超过上限，请收窄组织或负责人条件".into())
        },
        other => other,
    })
}

/// 经办筛选、业务条件与真实来源权限形成一个数据库最终集合。
fn pipeline(
    filter: &SupplierPaymentFilter,
    authorization: &FundsAuthorization,
    condition: &FundsLinkedCondition,
) -> Vec<Document> {
    let mut stages = vec![doc! { "$match": filter.to_doc() }, allocation_lookup(authorization, condition)];
    stages.extend(operator_stages(condition));
    stages.extend(flow::visibility(
        authorization.ledger_read,
        condition.owner_user_ids.is_some() || condition.org_unit_ids.is_some(),
    ));
    stages
}

/// 核销通过应付分录和子账识别实际来源类型，身份相同不能跨域串权。
fn allocation_lookup(authorization: &FundsAuthorization, condition: &FundsLinkedCondition) -> Document {
    let mut stages = vec![
        doc! { "$match": { "deleted_at": 0_i64, "$expr": { "$eq": ["$supplier_payment_id", "$$payment"] } } },
        lookup(
            Database::PAYABLE_ENTRIES,
            "payable_entry_id",
            "id",
            "scope_entry",
            doc! {},
            doc! { "id": 1, "payable_account_id": 1 },
        ),
        doc! { "$set": { "scope_account_id": first("scope_entry.payable_account_id") } },
        lookup(
            Database::PAYABLE_ACCOUNTS,
            "scope_account_id",
            "id",
            "scope_account",
            doc! {},
            doc! { "id": 1, "source_type": 1, "source_document_id": 1 },
        ),
        doc! { "$set": { "scope_source_id": first("scope_account.source_document_id"), "scope_source_type": first("scope_account.source_type") } },
    ];
    stages.extend(source_lookups(authorization, condition));
    let projection = doc! { "_id": 0, "id": 1, "order": "$scope_order_id", "owner": "$scope_source.owner_user_id", "source_version": "$scope_source.version",
    "exists": { "$ne": [{ "$ifNull": ["$scope_source.id", Bson::Null] }, Bson::Null] }, "matched": "$scope_source_allowed" };
    stages.push(doc! { "$project": projection });
    doc! { "$lookup": { "from": Database::PAYMENT_ALLOCATIONS, "let": { "payment": "$id" }, "pipeline": stages, "as": "scope_links" } }
}

/// 采购与结算分别采用各自已经解析的来源范围及负责人字段。
fn source_lookups(authorization: &FundsAuthorization, condition: &FundsLinkedCondition) -> Vec<Document> {
    let purchase = authorization
        .purchase_scope
        .as_ref()
        .map_or_else(|| doc! { "$expr": false }, |scope| scope.document());
    let settlement = authorization
        .settlement
        .as_ref()
        .map_or_else(|| doc! { "$expr": false }, |scope| source_scope_document(scope, "prepared_by"));
    let mut stages = source_stages(
        Database::PURCHASE_ORDERS,
        source_id("purchase_order"),
        "scope_purchase",
        doc! { "$and": [purchase, linked_condition_document(condition, "owner_user_id")] },
        "owner_user_id",
        "purchase_no",
    );
    stages.extend(source_stages(
        Database::SUPPLIER_SETTLEMENT_STATEMENTS,
        source_id("supplier_settlement"),
        "scope_settlement",
        doc! { "$and": [settlement, linked_condition_document(condition, "prepared_by")] },
        "prepared_by",
        "statement_no",
    ));
    stages.push(doc! { "$set": {
        "scope_source": { "$cond": [{ "$eq": ["$scope_source_type", "supplier_settlement"] }, "$scope_settlement", "$scope_purchase"] },
        "scope_source_allowed": { "$cond": [{ "$eq": ["$scope_source_type", "supplier_settlement"] }, "$scope_settlement_allowed", "$scope_purchase_allowed"] },
        "scope_order_id": { "$cond": [{ "$eq": ["$scope_source_type", "supplier_settlement"] }, { "$concat": ["supplier_settlement_statement:", "$scope_source_id"] }, "$scope_source_id"] }
    } });
    stages
}

/// 未命中实际来源类型时不向另一来源集合查询同名身份。
fn source_id(kind: &str) -> Bson {
    doc! { "$cond": [{ "$eq": ["$scope_source_type", kind] }, "$scope_source_id", Bson::Null] }.into()
}

/// 付款经办人只采用当前成功创建或提交审计，不把采购负责人混作经办人。
fn operator_stages(condition: &FundsLinkedCondition) -> Vec<Document> {
    let Some(operators) = &condition.operator_user_ids else {
        return Vec::new();
    };
    let filter = doc! { "resource_type": "supplier_payment", "success": true,
    "action": { "$in": ["supplier_payment.create", "supplier_payment.commit"] }, "actor_id": { "$in": operators } };
    vec![
        lookup(Database::AUDIT_LOGS, "id", "resource_id", "scope_operators", filter, doc! { "id": 1 }),
        doc! { "$match": { "scope_operators.0": { "$exists": true } } },
    ]
}

/// 页内付款行仅装载正式裁剪所需主表字段。
fn projection() -> Document {
    doc! { "_id": 0, "id": 1, "status": 1, "payment_no": 1, "supplier_id": 1, "paid_at": 1,
    "amount": 1, "bank_reference": 1, "version": 1, "created_at": 1 }
}

/// 保留领域排序字段及 ID 尾键，最终授权不改变顺序。
fn sort(query: &SupplierPaymentListQuery) -> Document {
    flow::sort(query.paging.sort_by, matches!(query.paging.sort_dir, application_core::SortDir::Asc))
}

#[cfg(test)]
mod tests {
    use erp_core::money::Amount;
    use erp_finance::entity::payable::PayableSourceType;
    use erp_finance::entity::receivable::AllocationAction;

    use super::flow::{FlowCount, SourceOwner, SourceVersion};
    use super::*;

    fn page(count: u64) -> FlowPage<SupplierPaymentRow> {
        FlowPage { items: vec![], versions: vec![], summary: vec![], total: vec![FlowCount { count }] }
    }

    fn header(id: &str, source: &str, owner: Option<&str>, amount: &str, whole: bool) -> FlowHeader {
        FlowHeader {
            version: FlowVersion {
                id: id.into(),
                version: 1,
                sources: vec![SourceVersion { id: source.into(), version: 3 }],
            },
            amount: amount.parse().unwrap(),
            whole,
            owners: vec![SourceOwner { id: source.into(), owner: owner.map(str::to_owned) }],
        }
    }

    fn link(
        id: &str,
        parent: &str,
        kind: PayableSourceType,
        amount: &str,
        action: AllocationAction,
    ) -> FinancialSummaryLink {
        FinancialSummaryLink {
            id: id.into(),
            parent_id: parent.into(),
            source_document_id: Some("same".into()),
            source_type: Some(kind),
            amount: amount.parse().unwrap(),
            action,
        }
    }

    #[test]
    fn complete_headers_keep_typed_sources_hidden_shares_and_off_page_versions() {
        let headers = vec![
            header("purchase", "same", Some("buyer"), "100", false),
            header("settlement", "supplier_settlement_statement:same", Some("preparer"), "5", true),
        ];
        let links = vec![
            link("apply", "purchase", PayableSourceType::PurchaseOrder, "60", AllocationAction::Apply),
            link(
                "settlement",
                "settlement",
                PayableSourceType::SupplierSettlement,
                "5",
                AllocationAction::Apply,
            ),
            link("hidden", "purchase", PayableSourceType::SupplierSettlement, "40", AllocationAction::Apply),
            link("reverse", "purchase", PayableSourceType::PurchaseOrder, "10", AllocationAction::Reverse),
        ];
        let result = finish_page(page(2), headers, links).unwrap();
        assert!(result.items.is_empty());
        assert_eq!(result.versions.len(), 2);
        assert_eq!(result.versions[1].sources[0].id, "supplier_settlement_statement:same");
        assert_eq!(
            result.summary[0].links.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
            ["apply", "reverse"]
        );
        let summary = flow::summary(&result.summary, "scope").unwrap();
        assert_eq!(
            summary
                .grouped
                .iter()
                .map(|row| (row.owner_user_id.as_str(), row.visible_share))
                .collect::<Vec<_>>(),
            [("buyer", "50".parse::<Amount>().unwrap()), ("preparer", "5".parse::<Amount>().unwrap())]
        );
        assert!(summary.whole_total.is_none());
        assert!(summary.permission_limited);
    }

    #[test]
    fn original_amount_find_sequence_preserves_intermediate_overflow_boundary() {
        let max = "79228162514264337593543950335";
        let links = vec![
            link("apply-max", "p", PayableSourceType::PurchaseOrder, max, AllocationAction::Apply),
            link("reverse-one", "p", PayableSourceType::PurchaseOrder, "1", AllocationAction::Reverse),
            link("apply-one", "p", PayableSourceType::PurchaseOrder, "1", AllocationAction::Apply),
        ];
        let result =
            finish_page(page(1), vec![header("p", "same", Some("buyer"), max, true)], links).unwrap();
        assert_eq!(
            result.summary[0].links.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
            ["apply-max", "reverse-one", "apply-one"]
        );
        let summary = flow::summary(&result.summary, "scope").unwrap();
        assert_eq!(summary.grouped[0].visible_share, max.parse().unwrap());
        assert_eq!(summary.whole_total, Some(max.parse().unwrap()));
    }

    #[test]
    fn missing_full_headers_are_rejected_and_absent_owner_keeps_unassigned_amount() {
        assert!(matches!(
            finish_page(page(2), vec![header("p", "same", None, "10", true)], vec![]),
            Err(Error::Internal(_))
        ));
        let result = finish_page(
            page(1),
            vec![header("p", "same", None, "10", true)],
            vec![link("a", "p", PayableSourceType::PurchaseOrder, "10", AllocationAction::Apply)],
        )
        .unwrap();
        let summary = flow::summary(&result.summary, "scope").unwrap();
        assert!(summary.grouped.is_empty());
        assert_eq!(summary.unassigned, "10".parse().unwrap());
        let empty = finish_page(page(0), vec![], vec![]).unwrap();
        assert!(empty.versions.is_empty());
        assert!(empty.summary.is_empty());
    }

    #[test]
    fn payment_capacity_keeps_original_boundary_and_resource_error_message() {
        assert_eq!(payment_count(&page(9_999)).unwrap(), 9_999);
        assert!(matches!(payment_count(&page(10_000)), Err(Error::ValidationError(message))
            if message == "付款查询超过上限，请收窄组织或负责人条件"));
        assert!(matches!(finish_page(page(10_001), vec![], vec![]), Err(Error::ValidationError(message))
            if message == "付款查询超过上限，请收窄组织或负责人条件"));
    }
}
