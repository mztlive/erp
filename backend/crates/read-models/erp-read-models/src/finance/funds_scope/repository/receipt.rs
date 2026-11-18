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

// 读取回款列表时，从最终授权与筛选的同一集合取当前页、总数、完整窄版本和份额汇总。
//
// 调用参数：
// * `db` - 回款集合所在数据库。
// * `query` - 已规范化的回款列表查询。
// * `filter` - 回款主表筛选。
// * `authorization` - 已解析来源授权。
// * `condition` - 负责人、经办人与组织条件。
//
// 当前页、计数、完整窄版本和授权份额摘要。聚合没有文档时页与计数为空，计数按零继续读取头信息。
//
// 聚合、解码或回款金额流读取失败时返回对应错误。计数达到 10000 时返回校验错误，文案为回款查询超过上限。
pub(in crate::finance::funds_scope) async fn page(
    db: &Database,
    query: &CustomerReceiptListQuery,
    filter: &CustomerReceiptFilter,
    authorization: &FundsAuthorization,
    condition: &FundsLinkedCondition,
    executor: &mut dyn Executor,
) -> Result<FlowPage<CustomerReceiptRow>> {
    authorization.validate(executor).await?;
    let result: crate::Result<_> = async {
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
    .await;

    let result = result?;
    authorization.validate(executor).await?;
    Ok(result)
}

// 回款列表交付前的二次检查只读取完整匹配集合的责任版本，不复算分页金额及分配视图。
//
// 调用参数：
// * `db` - 回款集合所在数据库。
// * `query` - 已规范化的回款列表查询，只使用其排序。
// * `filter` - 回款主表筛选。
// * `authorization` - 已解析来源授权。
// * `condition` - 负责人、经办人与组织条件。
//
// 按原主表排序的全部匹配回款及获授权来源版本。
//
// 聚合或投影解码失败时返回对应错误。
pub(in crate::finance::funds_scope) async fn versions(
    db: &Database,
    query: &CustomerReceiptListQuery,
    filter: &CustomerReceiptFilter,
    authorization: &FundsAuthorization,
    condition: &FundsLinkedCondition,
    executor: &mut dyn Executor,
) -> Result<Vec<FlowVersion>> {
    authorization.validate(executor).await?;
    let result: crate::Result<_> = async {
        let mut stages = pipeline(query, filter, authorization, condition, false);
        stages.extend(flow::recheck_stages(sort(query)));
        aggregate(db.collection(Database::CUSTOMER_RECEIPTS), stages, executor).await
    }
    .await;

    let result = result?;
    authorization.validate(executor).await?;
    Ok(result)
}

// 令页面、总数、版本和汇总复用回款的最终匹配条件：先施加主单、来源授权和业务筛选，再执行各读分支。
fn pipeline(
    query: &CustomerReceiptListQuery,
    filter: &CustomerReceiptFilter,
    authorization: &FundsAuthorization,
    condition: &FundsLinkedCondition,
    summary: bool,
) -> Vec<Document> {
    let mut stages = vec![doc! { "$match": filter.to_doc() }];
    stages.extend(operator_stages(query, condition));
    stages.extend(authorization.guard_stages());

    stages.push(allocation_lookup(query, authorization, condition, summary));
    if query.sales_order_id.is_some() || query.receivable_account_id.is_some() {
        stages.push(pending_lookup(query));
        stages.push(doc! { "$match": { "$expr": { "$or": [
            { "$anyElementTrue": [{ "$map": { "input": "$scope_links", "as": "link", "in": "$$link.scope_match" } }] },
            { "$gt": [{ "$size": "$scope_pending" }, 0] }
        ] } } });
    }

    stages.push(super::whole_facts::fields("$scope_links.order".into(), Bson::Array(vec![]),
        doc! { "$allElementsTrue": [{ "$map": { "input": "$scope_links", "as": "link", "in": "$$link.exists" } }] }.into()));
    stages.extend(authorization.whole.stages());
    stages.extend(flow::visibility(condition.owner_user_ids.is_some() || condition.org_unit_ids.is_some()));
    stages
}

// 处理回款核销份额前，沿分录和应收子账确认真实销售来源；无法解析的引用不授予份额资格。
fn allocation_lookup(
    query: &CustomerReceiptListQuery,
    authorization: &FundsAuthorization,
    condition: &FundsLinkedCondition,
    summary: bool,
) -> Document {
    let scope = linked_condition_document(condition, "sales_owner_user_id");
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
        &authorization.sales_stages(),
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

// 筛选回款来源单据时交集匹配精确销售单与应收子账；结果只选择单据，不证明各核销份额可读。
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

// 按应收子账搜索回款时联查拟核销分录，使具备整账资格的草稿也能命中，不将拟核销当作过账份额。
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

// 筛选回款经办人时区分登记人与核销人：前者取领域实体创建人，后者取已提交的审批快照。
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

// 准备回款当前页时仅投影公开行需要的主表字段，不对页外单据装配分配详情。
fn projection() -> Document {
    doc! { "_id": 0, "id": 1, "status": 1, "receipt_no": 1, "counterparty_party_id": 1, "customer_id": 1,
    "received_at": 1, "amount": 1, "bank_reference": 1, "version": 1, "created_at": 1, "pending_allocations": 1 }
}

// 排序回款页面和完整匹配版本时沿用规范化的领域字段，并用单据 ID 保持同值排序稳定。
fn sort(query: &CustomerReceiptListQuery) -> Document {
    flow::sort(query.paging.sort_by, matches!(query.paging.sort_dir, SortDir::Asc))
}

#[cfg(test)]
mod tests {
    use erp_finance::dto::receivable::CustomerReceiptListParams;

    use super::super::tests::authorization;
    use super::*;

    // 验证客户回款：登记使用领域创建人，核销使用提交快照，两类经办身份不互相替代。
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

    // 验证客户回款：未提供经办条件不加限制，提供经办条件但无动作类型时保持失败关闭。
    #[test]
    fn receipt_operator_filter_requires_explicit_kind() {
        let query = CustomerReceiptListParams::default().normalized().unwrap();
        assert!(operator_stages(&query, &FundsLinkedCondition::default()).is_empty());
        let condition = FundsLinkedCondition { operator_user_ids: Some(vec![]), ..Default::default() };
        assert_eq!(operator_stages(&query, &condition), [doc! { "$match": { "$expr": false } }]);
    }

    // 验证客户回款：经办条件先于核销关联执行，两类经办均保持原来源、可见性与分页共同前置条件。
    #[test]
    fn receipt_operator_filter_precedes_allocation_sources() {
        let mut query = CustomerReceiptListParams::default().normalized().unwrap();
        let condition =
            FundsLinkedCondition { operator_user_ids: Some(vec!["operator".into()]), ..Default::default() };
        for kind in [ReceiptOperatorKind::Register, ReceiptOperatorKind::Settle] {
            query.operator_kind = Some(kind);
            let stages =
                pipeline(&query, &CustomerReceiptFilter::default(), &authorization(), &condition, false);
            let source_index = if kind == ReceiptOperatorKind::Register { 2 } else { 3 }
                + authorization().guard_stages().len();
            assert_eq!(
                stages[source_index].get_document("$lookup").unwrap().get_str("from").unwrap(),
                Database::RECEIPT_ALLOCATIONS
            );
        }
    }
}
