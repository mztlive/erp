//! 销售选品集合索引。

use mongodb::bson::{Document, doc};
use mongodb::options::IndexOptions;
use mongodb::{Database, IndexModel};
use persistence_core::Result;

use crate::repository::extensions::SalesSelectionExt;

/// 创建选品集合索引。
///
/// # 参数
/// * `db` - 数据库
///
/// # 返回
/// 成功创建。
///
/// # 错误
/// 唯一约束冲突或 MongoDB 失败。
pub(crate) async fn ensure(db: &Database) -> Result<()> {
    create_indexes(db, <Database as SalesSelectionExt>::SALES_SELECTION_BOOKLETS, booklet_indexes()).await?;
    create_indexes(db, <Database as SalesSelectionExt>::SALES_SELECTION_DISPLAY_ITEMS, display_indexes())
        .await?;
    create_indexes(db, <Database as SalesSelectionExt>::SALES_SELECTION_POOL_MEMBERS, pool_indexes()).await?;
    create_indexes(db, <Database as SalesSelectionExt>::SALES_SELECTION_PREPARE_TASKS, task_indexes())
        .await?;
    create_indexes(db, <Database as SalesSelectionExt>::SALES_SELECTION_SESSIONS, session_indexes()).await?;
    create_indexes(db, <Database as SalesSelectionExt>::SALES_SELECTION_PROPOSALS, proposal_indexes())
        .await?;
    create_indexes(
        db,
        <Database as SalesSelectionExt>::SALES_SELECTION_PROPOSAL_DISPLAY_LINES,
        proposal_display_indexes(),
    )
    .await?;
    create_indexes(
        db,
        <Database as SalesSelectionExt>::SALES_SELECTION_PROPOSAL_SKU_LINES,
        proposal_sku_indexes(),
    )
    .await?;
    create_indexes(db, <Database as SalesSelectionExt>::SALES_SELECTION_IDEMPOTENCY, idempotency_indexes())
        .await?;
    create_indexes(db, <Database as SalesSelectionExt>::SALES_SELECTION_RATE_WINDOWS, rate_indexes()).await
}

/// 为集合创建索引。
///
/// # 参数
/// * `db` - 数据库
/// * `collection` - 集合名
/// * `indexes` - 索引
///
/// # 返回
/// 成功创建。
///
/// # 错误
/// MongoDB 失败。
async fn create_indexes(db: &Database, collection: &str, indexes: Vec<IndexModel>) -> Result<()> {
    db.collection::<Document>(collection).create_indexes(indexes).await?;
    Ok(())
}

/// 选品册索引：列表筛选、责任范围、令牌查找、一册一方案。
fn booklet_indexes() -> Vec<IndexModel> {
    vec![
        named_index(
            "idx_sales_selection_booklets_list",
            doc! { "customer_id": 1, "status": 1, "form": 1, "submit_mode": 1, "created_at": -1 },
        ),
        named_index(
            "idx_sales_selection_booklets_owner_org",
            doc! { "sales_owner_user_id": 1, "business_org_unit_id": 1, "created_at": -1 },
        ),
        unique_partial_index(
            "uk_sales_selection_booklets_token_hash",
            doc! { "link_token_hash": 1 },
            doc! { "link_token_hash": { "$type": "string" } },
        ),
        unique_partial_index(
            "uk_sales_selection_booklets_proposal",
            doc! { "proposal_id": 1 },
            doc! { "proposal_id": { "$type": "string" } },
        ),
    ]
}

/// 陈列项按册与批次查询。
fn display_indexes() -> Vec<IndexModel> {
    vec![named_index(
        "idx_sales_selection_display_items_booklet_batch",
        doc! { "booklet_id": 1, "batch_id": 1, "effective": 1, "removed": 1 },
    )]
}

/// 商品池成员按批次查询。
fn pool_indexes() -> Vec<IndexModel> {
    vec![named_index(
        "idx_sales_selection_pool_members_batch",
        doc! { "booklet_id": 1, "batch_id": 1, "sku.sku_id": 1 },
    )]
}

/// 准备任务领取与幂等。
fn task_indexes() -> Vec<IndexModel> {
    vec![
        named_index(
            "idx_sales_selection_prepare_tasks_status_deadline",
            doc! { "status": 1, "deadline_at": 1 },
        ),
        named_index("idx_sales_selection_prepare_tasks_booklet", doc! { "booklet_id": 1, "created_at": -1 }),
    ]
}

/// 一册一份会话。
fn session_indexes() -> Vec<IndexModel> {
    vec![unique_index("uk_sales_selection_sessions_booklet", doc! { "booklet_id": 1 })]
}

/// 方案编号、一册一份方案、责任范围与客户查询。
fn proposal_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_sales_selection_proposals_no", doc! { "proposal_no": 1 }),
        unique_index("uk_sales_selection_proposals_booklet", doc! { "booklet_id": 1 }),
        named_index("idx_sales_selection_proposals_customer", doc! { "customer_id": 1, "submitted_at": -1 }),
        named_index(
            "idx_sales_selection_proposals_owner_org",
            doc! { "sales_owner_user_id": 1, "business_org_unit_id": 1, "submitted_at": -1 },
        ),
    ]
}

/// 方案陈列行。
fn proposal_display_indexes() -> Vec<IndexModel> {
    vec![named_index("idx_sales_selection_proposal_display_lines_proposal", doc! { "proposal_id": 1 })]
}

/// 方案 SKU 行。
fn proposal_sku_indexes() -> Vec<IndexModel> {
    vec![named_index(
        "idx_sales_selection_proposal_sku_lines_proposal",
        doc! { "proposal_id": 1, "display_item_id": 1 },
    )]
}

/// 幂等唯一约束。
fn idempotency_indexes() -> Vec<IndexModel> {
    vec![unique_index(
        "uk_sales_selection_idempotency_scope_key",
        doc! { "operation": 1, "scope_id": 1, "idempotency_key": 1 },
    )]
}

/// 公开限流窗口。
fn rate_indexes() -> Vec<IndexModel> {
    vec![
        IndexModel::builder()
            .keys(doc! { "expires_at": 1 })
            .options(
                IndexOptions::builder()
                    .name("ttl_sales_selection_rate_windows".to_string())
                    .expire_after(std::time::Duration::ZERO)
                    .build(),
            )
            .build(),
    ]
}

/// 命名普通索引。
fn named_index(name: impl Into<String>, keys: Document) -> IndexModel {
    IndexModel::builder().keys(keys).options(IndexOptions::builder().name(name.into()).build()).build()
}

/// 命名唯一索引。
fn unique_index(name: impl Into<String>, keys: Document) -> IndexModel {
    IndexModel::builder()
        .keys(keys)
        .options(IndexOptions::builder().name(name.into()).unique(true).build())
        .build()
}

/// 命名部分唯一索引。
fn unique_partial_index(name: impl Into<String>, keys: Document, filter: Document) -> IndexModel {
    IndexModel::builder()
        .keys(keys)
        .options(
            IndexOptions::builder().name(name.into()).unique(true).partial_filter_expression(filter).build(),
        )
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owner_org_indexes_cover_scope_queries() {
        let booklets = booklet_indexes();
        assert!(booklets.iter().any(|index| index.keys
            == doc! { "sales_owner_user_id": 1, "business_org_unit_id": 1, "created_at": -1 }));
        let proposals = proposal_indexes();
        assert!(proposals.iter().any(|index| index.keys
            == doc! { "sales_owner_user_id": 1, "business_org_unit_id": 1, "submitted_at": -1 }));
    }

    #[test]
    fn rate_windows_use_ttl_without_redeclaring_builtin_id() {
        let indexes = rate_indexes();
        assert_eq!(indexes.len(), 1);
        assert_eq!(indexes[0].keys, doc! { "expires_at": 1 });
        let options = indexes[0].options.as_ref().unwrap();
        assert_eq!(options.expire_after, Some(std::time::Duration::ZERO));
        assert_ne!(options.unique, Some(true));
    }
}
