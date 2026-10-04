//! 回款的最终来源授权、经办筛选及数据库分页查询。

use erp_finance::dto::receivable::{CustomerReceiptListQuery, ReceiptOperatorKind, SortDir};
use erp_finance::repository::{
    CustomerReceiptFilter, CustomerReceiptRow, FundsSummaryRepository, ReceivableExt,
};
use erp_sales::repository::SalesOrderExt;
use erp_workflow::repository::ApprovalIntegrationExt;
use mongodb::Database;
use mongodb::bson::{Bson, Document, doc};
use persistence_core::{Executor, QueryFilter};

use super::super::{FundsAuthorization, FundsLinkedCondition};
use super::flow::{self, FlowHeader, FlowPage, FlowVersion, first, lookup};
use super::{aggregate, linked_condition_document, source_stages};
use crate::{Error, Result};

/// 同一最终条件取当前页、总数、完整窄版本与份额摘要。
pub(in crate::finance::funds_scope) async fn page(
    db: &Database,
    query: &CustomerReceiptListQuery,
    filter: &CustomerReceiptFilter,
    authorization: &FundsAuthorization,
    condition: &FundsLinkedCondition,
    executor: &mut dyn Executor,
) -> Result<FlowPage<CustomerReceiptRow>> {
    let mut stages = pipeline(query, filter, authorization, condition, false);
    stages.push(flow::facet(query.paging.page, query.paging.page_size, projection(), sort(query))?);
    let mut rows = aggregate(db.collection(Database::CUSTOMER_RECEIPTS), stages, executor).await?;
    let mut snapshot = rows.pop().unwrap_or_else(|| FlowPage {
        items: vec![],
        versions: vec![],
        total: vec![],
        summary: vec![],
    });
    snapshot
        .count()
        .map_err(|_| Error::ValidationError("回款查询超过上限，请收窄组织或负责人条件".into()))?;
    let mut stages = pipeline(query, filter, authorization, condition, false);
    stages.extend(flow::header_stages(sort(query)));
    let headers: Vec<FlowHeader> =
        aggregate(db.collection(Database::CUSTOMER_RECEIPTS), stages, executor).await?;
    let ids = headers.iter().map(|header| header.version.id.clone()).collect::<Vec<_>>();
    let links = FundsSummaryRepository::new(db).receipt_links(&ids, executor).await?;
    (snapshot.versions, snapshot.summary) = flow::summaries(headers, links);
    Ok(snapshot)
}

/// 二次授权只装载完整匹配责任版本，禁止复算页面金额和分配视图。
pub(in crate::finance::funds_scope) async fn versions(
    db: &Database,
    query: &CustomerReceiptListQuery,
    filter: &CustomerReceiptFilter,
    authorization: &FundsAuthorization,
    condition: &FundsLinkedCondition,
    executor: &mut dyn Executor,
) -> Result<Vec<FlowVersion>> {
    let mut stages = pipeline(query, filter, authorization, condition, false);
    stages.extend(flow::recheck_stages(sort(query)));
    aggregate(db.collection(Database::CUSTOMER_RECEIPTS), stages, executor).await
}

/// 全部业务和来源条件先形成最终集合，再由页面或版本分支消费。
fn pipeline(
    query: &CustomerReceiptListQuery,
    filter: &CustomerReceiptFilter,
    authorization: &FundsAuthorization,
    condition: &FundsLinkedCondition,
    summary: bool,
) -> Vec<Document> {
    let mut stages = vec![doc! { "$match": filter.to_doc() }];
    stages.extend(operator_stages(query, condition));
    stages.push(allocation_lookup(query, authorization, condition, summary));
    if query.sales_order_id.is_some() || query.receivable_account_id.is_some() {
        stages.push(pending_lookup(query));
        stages.push(doc! { "$match": { "$expr": { "$or": [
            { "$anyElementTrue": [{ "$map": { "input": "$scope_links", "as": "link", "in": "$$link.scope_match" } }] },
            { "$gt": [{ "$size": "$scope_pending" }, 0] }
        ] } } });
    }
    stages.extend(flow::visibility(
        authorization.ledger_read,
        condition.owner_user_ids.is_some() || condition.org_unit_ids.is_some(),
    ));
    stages
}

/// 核销引用按分录、子账和销售事实解析，缺失引用保持失败关闭。
fn allocation_lookup(
    query: &CustomerReceiptListQuery,
    authorization: &FundsAuthorization,
    condition: &FundsLinkedCondition,
    summary: bool,
) -> Document {
    let scope = doc! { "$and": [authorization.sales.document(), linked_condition_document(condition, "sales_owner_user_id")] };
    let mut stages = vec![
        doc! { "$match": { "deleted_at": 0_i64, "$expr": { "$eq": ["$customer_receipt_id", "$$receipt"] } } },
        lookup(
            Database::RECEIVABLE_ENTRIES,
            "receivable_entry_id",
            "id",
            "scope_entry",
            doc! {},
            doc! { "id": 1, "receivable_account_id": 1 },
        ),
        doc! { "$set": { "scope_account_id": first("scope_entry.receivable_account_id") } },
        lookup(
            Database::RECEIVABLE_ACCOUNTS,
            "scope_account_id",
            "id",
            "scope_account",
            doc! {},
            doc! { "id": 1, "sales_order_id": 1 },
        ),
        doc! { "$set": { "scope_order_id": first("scope_account.sales_order_id") } },
    ];
    stages.extend(source_stages(
        Database::SALES_ORDERS,
        "$scope_order_id".into(),
        "scope_source",
        scope,
        "sales_owner_user_id",
        "order_no",
    ));
    let mut project = doc! { "_id": 0, "id": 1, "order": "$scope_order_id", "owner": "$scope_source.owner_user_id", "source_version": "$scope_source.version",
    "exists": { "$ne": [{ "$ifNull": ["$scope_source.id", Bson::Null] }, Bson::Null] },
    "matched": "$scope_source_allowed", "scope_match": scope_match(query, "$scope_account_id", "$scope_order_id") };
    if summary {
        project.insert("allocated_amount", 1);
        project.insert("allocation_action", 1);
    }
    stages.push(doc! { "$project": project });
    doc! { "$lookup": { "from": Database::RECEIPT_ALLOCATIONS, "let": { "receipt": "$id" }, "pipeline": stages, "as": "scope_links" } }
}

/// 精确销售单与子账条件求交；它只选择单据，不替代份额授权。
fn scope_match(query: &CustomerReceiptListQuery, account: &str, order: &str) -> Bson {
    let mut conditions = Vec::<Bson>::new();
    if let Some(id) = &query.sales_order_id {
        conditions.push(doc! { "$eq": [order, id] }.into());
    }
    if let Some(id) = &query.receivable_account_id {
        conditions.push(doc! { "$eq": [account, id.as_ref()] }.into());
    }
    doc! { "$and": conditions }.into()
}

/// 拟核销分录使有整账资格的草稿仍可在精确子账查询中命中。
fn pending_lookup(query: &CustomerReceiptListQuery) -> Document {
    let stages = vec![
        doc! { "$match": { "deleted_at": 0_i64, "$expr": { "$in": ["$id", "$$entries"] } } },
        lookup(
            Database::RECEIVABLE_ACCOUNTS,
            "receivable_account_id",
            "id",
            "scope_account",
            doc! {},
            doc! { "id": 1, "sales_order_id": 1 },
        ),
        doc! { "$match": { "scope_account.0": { "$exists": true } } },
        doc! { "$set": { "scope_order_id": first("scope_account.sales_order_id") } },
        doc! { "$match": { "$expr": scope_match(query, "$receivable_account_id", "$scope_order_id") } },
        doc! { "$project": { "_id": 0, "id": 1 } },
    ];
    doc! { "$lookup": { "from": Database::RECEIVABLE_ENTRIES,
    "let": { "entries": { "$ifNull": ["$pending_allocations.receivable_entry_id", []] } },
    "pipeline": stages, "as": "scope_pending" } }
}

/// 登记人直接来自不可变领域创建人，核销人仍来自已提交审批快照。
fn operator_stages(query: &CustomerReceiptListQuery, condition: &FundsLinkedCondition) -> Vec<Document> {
    let Some(operators) = &condition.operator_user_ids else {
        return Vec::new();
    };
    match query.operator_kind {
        Some(ReceiptOperatorKind::Register) => {
            return vec![doc! { "$match": { "created_by": { "$in": operators } } }];
        },
        Some(ReceiptOperatorKind::Settle) => {},
        None => return vec![doc! { "$match": { "$expr": false } }],
    }
    let filter = doc! { "document_type": "customer_receipt", "payload.submitted_by": { "$in": operators }, "deleted_at": 0_i64 };
    vec![
        lookup(
            Database::APPROVAL_SUBJECT_SNAPSHOTS,
            "id",
            "business_object_id",
            "scope_operators",
            filter,
            doc! { "id": 1 },
        ),
        doc! { "$match": { "scope_operators.0": { "$exists": true } } },
    ]
}

/// 回款页只装载公开行所需主表字段，页外不装配分配视图。
fn projection() -> Document {
    doc! { "_id": 0, "id": 1, "status": 1, "receipt_no": 1, "counterparty_party_id": 1, "customer_id": 1,
    "received_at": 1, "amount": 1, "bank_reference": 1, "version": 1, "created_at": 1, "pending_allocations": 1 }
}

/// 保持领域排序字段和 ID 尾键，客户端已按 DTO 规范化。
fn sort(query: &CustomerReceiptListQuery) -> Document {
    flow::sort(query.paging.sort_by, matches!(query.paging.sort_dir, SortDir::Asc))
}

#[cfg(test)]
mod tests {
    use erp_finance::dto::receivable::CustomerReceiptListParams;

    use super::super::tests::authorization;
    use super::*;

    /// 登记使用领域创建人，核销使用提交快照，两类经办身份不互相替代。
    #[test]
    fn receipt_operator_filters_keep_registration_and_settlement_facts_distinct() {
        let mut query = CustomerReceiptListParams::default().normalized().unwrap();
        let condition =
            FundsLinkedCondition { operator_user_ids: Some(vec!["operator".into()]), ..Default::default() };
        query.operator_kind = Some(ReceiptOperatorKind::Register);
        assert_eq!(
            operator_stages(&query, &condition),
            [doc! { "$match": { "created_by": { "$in": ["operator"] } } }]
        );
        query.operator_kind = Some(ReceiptOperatorKind::Settle);
        let stages = operator_stages(&query, &condition);
        let lookup = stages[0].get_document("$lookup").unwrap();
        assert_eq!(lookup.get_str("from").unwrap(), Database::APPROVAL_SUBJECT_SNAPSHOTS);
        let filter = lookup.get_array("pipeline").unwrap()[0].as_document().unwrap();
        let conjunction = filter.get_document("$match").unwrap().get_array("$and").unwrap();
        assert_eq!(
            conjunction[1].as_document().unwrap(),
            &doc! {
                "document_type": "customer_receipt", "payload.submitted_by": { "$in": ["operator"] }, "deleted_at": 0_i64
            }
        );
        assert_eq!(
            conjunction[0].as_document().unwrap().get_document("$expr").unwrap(),
            &doc! { "$eq": ["$business_object_id", "$$linked"] }
        );
        assert_eq!(stages[1], doc! { "$match": { "scope_operators.0": { "$exists": true } } });
    }

    /// 未提供经办条件不加限制，提供经办条件但无动作类型时保持失败关闭。
    #[test]
    fn receipt_operator_filter_requires_explicit_kind() {
        let query = CustomerReceiptListParams::default().normalized().unwrap();
        assert!(operator_stages(&query, &FundsLinkedCondition::default()).is_empty());
        let condition = FundsLinkedCondition { operator_user_ids: Some(vec![]), ..Default::default() };
        assert_eq!(operator_stages(&query, &condition), [doc! { "$match": { "$expr": false } }]);
    }

    /// 经办条件先于核销关联执行，两类经办均保持原来源、可见性与分页共同前置条件。
    #[test]
    fn receipt_operator_filter_precedes_allocation_sources() {
        let mut query = CustomerReceiptListParams::default().normalized().unwrap();
        let condition =
            FundsLinkedCondition { operator_user_ids: Some(vec!["operator".into()]), ..Default::default() };
        for kind in [ReceiptOperatorKind::Register, ReceiptOperatorKind::Settle] {
            query.operator_kind = Some(kind);
            let stages =
                pipeline(&query, &CustomerReceiptFilter::default(), &authorization(), &condition, false);
            let source_index = if kind == ReceiptOperatorKind::Register { 2 } else { 3 };
            assert_eq!(
                stages[source_index].get_document("$lookup").unwrap().get_str("from").unwrap(),
                Database::RECEIPT_ALLOCATIONS
            );
        }
    }
}
