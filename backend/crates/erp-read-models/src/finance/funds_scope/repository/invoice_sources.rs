//! 发票分配只关联一次当前子账和真实来源，保留存在性与范围资格。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_finance::repository::{PayableExt, ReceivableExt};
use erp_procurement::repository::PurchaseOrderExt;
use erp_sales::repository::SalesOrderExt;
use erp_supply::repository::SupplierSettlementExt;
use mongodb::Database;
use mongodb::bson::{Bson, Document, doc};

use super::super::{FundsAuthorization, FundsLinkedCondition};
use super::{source_scope_document, source_stages};

/// 从分配引用读取必要的子账字段；缺失引用保持空事实。
///
/// # 参数
/// 财务子账集合和当前分配账户引用表达式。
/// # 返回
/// 只装载真实来源引用的子账关联管道。
/// # 错误
/// 无。
pub(super) fn account_stages(collection: &str, reference: &str) -> Vec<Document> {
    vec![
        doc! { "$lookup": { "from": collection, "let": { "account_id": reference },
        "pipeline": [
            { "$match": { "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
                "$expr": { "$eq": ["$id", "$$account_id"] } } },
            { "$project": { "_id": 0, "id": 1, "sales_order_id": 1,
                "source_type": 1, "source_document_id": 1 } },
        ], "as": "_account" } },
        doc! { "$set": { "_account": { "$arrayElemAt": ["$_account", 0] } } },
    ]
}

/// 销项分配沿应收子账读取当前销售责任，业务条件仅收窄已授权份额。
///
/// # 参数
/// 已解析销售授权和业务筛选条件。
/// # 返回
/// 销售来源存在性与授权份额匹配管道。
/// # 错误
/// 无。
pub(super) fn sales_stages(
    authorization: &FundsAuthorization,
    condition: &FundsLinkedCondition,
) -> Vec<Document> {
    let mut stages =
        account_stages(<Database as ReceivableExt>::RECEIVABLE_ACCOUNTS, "$receivable_account_id");
    stages.extend(source_stages(
        <Database as SalesOrderExt>::SALES_ORDERS,
        Bson::String("$_account.sales_order_id".into()),
        "_source",
        authorization.sales.document(),
        "sales_owner_user_id",
        "order_no",
    ));
    stages.push(doc! { "$set": { "order": "$_account.sales_order_id",
        "source_exists": { "$eq": [{ "$type": "$_source" }, "object"] },
        "matched": matched_expression(condition.owner_user_ids.as_deref(), condition.org_unit_ids.as_deref()),
    } });
    stages
}

/// 进项分配严格区分采购和结算来源，同主键不得跨来源串权。
///
/// # 参数
/// 已解析采购、结算授权以及负责人、组织筛选条件。
/// # 返回
/// 区分实际来源类型的存在性与授权份额匹配管道。
/// # 错误
/// 无。
pub(super) fn purchase_stages(
    authorization: &FundsAuthorization,
    owners: Option<&[String]>,
    organizations: Option<&[String]>,
) -> Vec<Document> {
    let mut stages = account_stages(<Database as PayableExt>::PAYABLE_ACCOUNTS, "$payable_account_id");
    stages.extend(purchase_source_stages(authorization));
    stages.push(doc! { "$set": {
        "_source": { "$cond": [{ "$eq": ["$_account.source_type", "purchase_order"] },
            "$_purchase", "$_settlement"] },
        "_source_allowed": { "$cond": [{ "$eq": ["$_account.source_type", "purchase_order"] },
            "$_purchase_allowed", "$_settlement_allowed"] },
        "order": { "$cond": [{ "$eq": ["$_account.source_type", "supplier_settlement"] },
            { "$concat": ["supplier_settlement_statement:", "$_account.source_document_id"] },
            "$_account.source_document_id"] },
    } });
    stages.push(doc! { "$set": {
        "source_exists": { "$eq": [{ "$type": "$_source" }, "object"] },
        "matched": matched_expression(owners, organizations),
    } });
    stages
}

/// 两个来源分别使用已经证明的领域范围，错误类型不会命中任一集合。
fn purchase_source_stages(authorization: &FundsAuthorization) -> Vec<Document> {
    let purchase = authorization
        .purchase_scope
        .as_ref()
        .map(|scope| scope.document())
        .unwrap_or_else(|| doc! { "$expr": false });
    let settlement = authorization
        .settlement
        .as_ref()
        .map(|scope| source_scope_document(scope, "prepared_by"))
        .unwrap_or_else(|| doc! { "$expr": false });
    let mut stages = source_stages(
        <Database as PurchaseOrderExt>::PURCHASE_ORDERS,
        typed_reference("purchase_order"),
        "_purchase",
        purchase,
        "owner_user_id",
        "purchase_no",
    );
    stages.extend(source_stages(
        <Database as SupplierSettlementExt>::SUPPLIER_SETTLEMENT_STATEMENTS,
        typed_reference("supplier_settlement"),
        "_settlement",
        settlement,
        "prepared_by",
        "statement_no",
    ));
    stages
}

/// 来源类型参与引用表达式；同 ID 的另一来源不得补齐损坏引用。
fn typed_reference(source_type: &str) -> Bson {
    Bson::Document(doc! { "$cond": [
        { "$eq": ["$_account.source_type", source_type] }, "$_account.source_document_id", Bson::Null,
    ] })
}

/// 已授权来源再与负责人和组织求交，缺失事实始终失败关闭。
fn matched_expression(owners: Option<&[String]>, organizations: Option<&[String]>) -> Document {
    let mut conditions = vec![
        Bson::String("$_source_allowed".into()),
        Bson::Document(doc! { "$eq": [{ "$type": "$_source" }, "object"] }),
    ];
    if let Some(ids) = owners {
        conditions.push(Bson::Document(doc! { "$in": ["$_source.owner_user_id", ids] }));
    }
    if let Some(ids) = organizations {
        conditions.push(Bson::Document(doc! { "$in": ["$_source.business_org_unit_id", ids] }));
    }
    doc! { "$and": conditions }
}

/// 每个发票只读取自己的分配；关联装配不产生额外查询或第二份事实。
///
/// # 参数
/// 本域分配集合、来源关联管道与结果字段。
/// # 返回
/// 读取当前发票实际分配的数据库关联阶段。
/// # 错误
/// 无。
pub(super) fn allocation_lookup(collection: &str, stages: Vec<Document>, output: &str) -> Document {
    let mut pipeline = vec![doc! { "$match": { "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
    "$expr": { "$eq": ["$invoice_id", "$$invoice_id"] } } }];
    pipeline.extend(stages);
    doc! { "$lookup": { "from": collection, "let": { "invoice_id": "$id" },
    "pipeline": pipeline, "as": output } }
}
