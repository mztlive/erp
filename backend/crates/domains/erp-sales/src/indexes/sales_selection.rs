//! 销售选品集合索引。

use mongodb::bson::{Document, doc};
use mongodb::options::IndexOptions;
use mongodb::{Database, IndexModel};
use persistence_core::Result;
use persistence_core::indexes::{named_index, unique_index, unique_partial_index};

use crate::repository::extensions::SalesSelectionExt;

// 创建选品集合索引。
// 结果：成功创建。
// 唯一约束冲突或 MongoDB 失败。
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

// 为集合创建索引。
// 结果：成功创建。
// MongoDB 失败。
//
// 输入约定：
// - `collection`：集合名
// - `indexes`：索引
async fn create_indexes(db: &Database, collection: &str, indexes: Vec<IndexModel>) -> Result<()> {
    db.collection::<Document>(collection).create_indexes(indexes).await?;
    Ok(())
}

// 注册选品册的列表、责任范围及令牌检索索引，并限制同一非空方案引用只能绑定到一本册。
fn booklet_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_sales_selection_booklets_id", doc! { "id": 1 }),
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

// 为列出某册某批次的有效陈列建立索引，同时支持过滤已移除的陈列。
fn display_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_sales_selection_display_items_id", doc! { "id": 1 }),
        named_index(
            "idx_sales_selection_display_items_booklet_batch",
            doc! { "booklet_id": 1, "batch_id": 1, "effective": 1, "removed": 1 },
        ),
    ]
}

// 为准备过程按册和批次装载冻结商品池成员建立索引。
fn pool_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_sales_selection_pool_members_id", doc! { "id": 1 }),
        named_index(
            "idx_sales_selection_pool_members_batch",
            doc! { "booklet_id": 1, "batch_id": 1, "sku.sku_id": 1 },
        ),
    ]
}

// 为后台按状态和截止时间找准备任务、以及管理端读同册任务历史建立索引，任务身份保持唯一。
fn task_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_sales_selection_prepare_tasks_id", doc! { "id": 1 }),
        named_index(
            "idx_sales_selection_prepare_tasks_status_deadline",
            doc! { "status": 1, "deadline_at": 1 },
        ),
        named_index("idx_sales_selection_prepare_tasks_booklet", doc! { "booklet_id": 1, "created_at": -1 }),
    ]
}

// 确保每本选品册至多一个会话。
fn session_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_sales_selection_sessions_id", doc! { "id": 1 }),
        unique_index("uk_sales_selection_sessions_booklet", doc! { "booklet_id": 1 }),
    ]
}

// 为按客户或责任范围读取提交方案建立索引，并约束方案编号与选品册唯一。
fn proposal_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_sales_selection_proposals_id", doc! { "id": 1 }),
        unique_index("uk_sales_selection_proposals_no", doc! { "proposal_no": 1 }),
        unique_index("uk_sales_selection_proposals_booklet", doc! { "booklet_id": 1 }),
        named_index("idx_sales_selection_proposals_customer", doc! { "customer_id": 1, "submitted_at": -1 }),
        named_index(
            "idx_sales_selection_proposals_owner_org",
            doc! { "sales_owner_user_id": 1, "business_org_unit_id": 1, "submitted_at": -1 },
        ),
    ]
}

// 为读取已提交方案的冻结陈列行建立按方案关联的索引。
fn proposal_display_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_sales_selection_proposal_display_lines_id", doc! { "id": 1 }),
        named_index("idx_sales_selection_proposal_display_lines_proposal", doc! { "proposal_id": 1 }),
    ]
}

// 为读取已提交方案的商品明细建立按方案关联的索引。
fn proposal_sku_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_sales_selection_proposal_sku_lines_id", doc! { "id": 1 }),
        named_index(
            "idx_sales_selection_proposal_sku_lines_proposal",
            doc! { "proposal_id": 1, "display_item_id": 1 },
        ),
    ]
}

// 发布或管理命令查找旧结果时，靠唯一索引阻止同一作用域、动作及键重复建回执。
fn idempotency_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_sales_selection_idempotency_id", doc! { "id": 1 }),
        unique_index(
            "uk_sales_selection_idempotency_scope_key",
            doc! { "operation": 1, "scope_id": 1, "idempotency_key": 1 },
        ),
    ]
}

// 为公开访问限流窗口安装到期索引，让 MongoDB 自动清理已过期的计数记录。
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

#[cfg(test)]
mod tests {
    // 检查各集合是否声明稳定 `id` 的唯一索引，防止主键读取和乐观锁写入退化为集合扫描。
    #[test]
    fn collections_look_up_by_unique_stable_id() {
        for indexes in [
            super::booklet_indexes(),
            super::display_indexes(),
            super::idempotency_indexes(),
            super::pool_indexes(),
            super::task_indexes(),
            super::proposal_display_indexes(),
            super::proposal_sku_indexes(),
            super::proposal_indexes(),
            super::session_indexes(),
        ] {
            assert!(indexes.iter().any(|index| {
                index.keys == mongodb::bson::doc! { "id": 1 }
                    && index.options.as_ref().and_then(|options| options.unique) == Some(true)
            }));
        }
    }

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
    #[test]
    fn booklet_indexes_enforce_single_session_and_proposal_per_booklet() {
        for indexes in [session_indexes(), proposal_indexes()] {
            let booklet = indexes.iter().find(|index| index.keys == doc! { "booklet_id": 1 }).unwrap();
            assert_eq!(booklet.options.as_ref().unwrap().unique, Some(true));
        }
        assert!(!session_indexes().iter().any(|index| index.keys == doc! { "voucher_code_hash": 1 }));
    }
}
