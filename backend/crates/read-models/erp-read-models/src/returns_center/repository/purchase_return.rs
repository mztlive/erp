//! 采购退货沿实际采购来源执行完整统一授权，分页和版本上限共享同一条件。

use erp_procurement::repository::purchase_order::scope::PurchaseReadScope;
use erp_returns::repository::ReturnsExt;
use erp_returns::repository::returns::{
    PurchaseReturnOrderFilter, PurchaseReturnOrderRow, PurchaseReturnVersion,
};
use mongodb::Database;
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Error as RepositoryError, Executor, PageResult, Pagination, QueryFilter, aggregate};

use crate::Result;

/// 页行、总数都先沿当前退货单的实际采购来源执行授权。
/// # 参数
/// db 是事实数据库；filter 是业务筛选；scope 是当前采购授权；executor 是原事务。
/// # 返回
/// 与完整授权条件一致的当前页及总数。
/// # 错误
/// 授权失效或数据库读取失败时拒绝。
pub(in crate::returns_center) async fn search(
    db: &Database,
    filter: &PurchaseReturnOrderFilter,
    scope: &PurchaseReadScope,
    executor: &mut dyn Executor,
) -> Result<PageResult<PurchaseReturnOrderRow>> {
    scope.validate(executor).await?;
    let collection = db.collection::<PurchaseReturnOrderRow>(Database::PURCHASE_RETURN_ORDERS);
    let stages = scope.related_stages(filter.to_doc(), "$purchase_order_id");
    let items =
        aggregate::find(&collection, filter.to_doc(), page_options(filter), stages.clone(), executor).await?;
    let total = aggregate::count(&collection, filter.to_doc(), stages, executor).await?;
    scope.validate(executor).await?;
    let total =
        i64::try_from(total).map_err(|_| RepositoryError::EntityMetadataOutOfRange("采购退货总数超限"))?;
    Ok(PageResult { items, total })
}

/// 仅对业务筛选与来源授权的交集应用退货版本数量上限。
/// # 参数
/// db 是事实数据库；filter 与列表一致；scope 是当前采购授权；executor 是原事务。
/// # 返回
/// 稳定 ID 顺序的最多 10001 个退货版本。
/// # 错误
/// 授权失效或数据库读取失败时拒绝。
pub(in crate::returns_center) async fn versions(
    db: &Database,
    filter: &PurchaseReturnOrderFilter,
    scope: &PurchaseReadScope,
    executor: &mut dyn Executor,
) -> Result<Vec<PurchaseReturnVersion>> {
    scope.validate(executor).await?;
    let rows = aggregate::find(
        &db.collection::<PurchaseReturnVersion>(Database::PURCHASE_RETURN_ORDERS),
        filter.to_doc(),
        FindOptions::builder()
            .projection(doc! { "id": 1, "version": 1 })
            .sort(doc! { "id": 1 })
            .limit(10001)
            .build(),
        scope.related_stages(filter.to_doc(), "$purchase_order_id"),
        executor,
    )
    .await?;
    scope.validate(executor).await?;
    Ok(rows)
}

/// 保持退货列表既有字段投影、排序白名单及稳定次键。
fn page_options(filter: &PurchaseReturnOrderFilter) -> FindOptions {
    let direction = if filter.sort_ascending { 1 } else { -1 };
    let field = filter
        .sort_by
        .as_deref()
        .filter(|field| ["purchase_return_no", "created_at"].contains(field))
        .unwrap_or("created_at");
    FindOptions::builder()
        .sort(doc! { field: direction, "id": direction })
        .skip(filter.skip())
        .limit(filter.limit())
        .projection(projection())
        .build()
}

fn projection() -> Document {
    doc! {
        "id": 1, "status": 1, "current_revision_id": 1, "created_by": 1, "updated_by": 1,
        "purchase_return_no": 1, "purchase_order_id": 1, "sales_return_case_id": 1,
        "return_mode": 1, "version": 1, "created_at": 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_preserve_requested_page_stable_sort_and_projection() {
        let options = page_options(&PurchaseReturnOrderFilter {
            page: 3,
            page_size: 5,
            sort_by: Some("purchase_return_no".into()),
            sort_ascending: true,
            ..Default::default()
        });
        assert_eq!(options.sort, Some(doc! { "purchase_return_no": 1, "id": 1 }));
        assert_eq!(options.skip, Some(10));
        assert_eq!(options.limit, Some(5));
        assert_eq!(options.projection.unwrap().get_i32("purchase_order_id").unwrap(), 1);
    }

    #[test]
    fn options_reject_unknown_sort_fields() {
        let options = page_options(&PurchaseReturnOrderFilter {
            sort_by: Some("arbitrary_field".into()),
            ..Default::default()
        });
        assert_eq!(options.sort, Some(doc! { "created_at": -1, "id": -1 }));
    }
}
