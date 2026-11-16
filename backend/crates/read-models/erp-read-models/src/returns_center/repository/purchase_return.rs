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
use serde::Deserialize;

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
    scope.ensure_current(executor)?;
    let collection = db.collection::<PurchaseReturnOrderRow>(Database::PURCHASE_RETURN_ORDERS);
    let stages = scope.related_stages(filter.to_doc(), "$purchase_order_id");
    let items =
        aggregate::find(&collection, filter.to_doc(), page_options(filter), stages.clone(), executor).await?;
    let total = aggregate::count(&collection, filter.to_doc(), stages, executor).await?;
    scope.ensure_current(executor)?;
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
    scope.ensure_current(executor)?;
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
    scope.ensure_current(executor)?;
    Ok(rows)
}

/// 只返回已通过实际采购来源完整授权的退货身份及来源身份。
/// # 参数
/// db 为退货事实数据库；id 为精确退货身份；scope 为原 DETAIL 凭证；executor 为原事务。
/// # 返回
/// 来源存在且授权命中时返回退货身份与采购来源身份，否则返回 None。
/// # 错误
/// 凭证失效、模型不符或数据库读取失败时拒绝。
pub(in crate::returns_center) async fn identity(
    db: &Database,
    id: &str,
    scope: &PurchaseReadScope,
    executor: &mut dyn Executor,
) -> Result<Option<(String, String)>> {
    scope.ensure_current(executor)?;
    let identities = aggregate::rows(
        &db.collection::<PurchaseReturnSource>(Database::PURCHASE_RETURN_ORDERS),
        purchase_return_identity_pipeline(id, scope),
        executor,
    )
    .await?;
    scope.ensure_current(executor)?;
    Ok(identities.into_iter().next().map(|identity| (identity.id, identity.purchase_order_id)))
}

/// 在来源授权后的最小投影中交付退货身份及其实际采购来源。
#[derive(Deserialize)]
struct PurchaseReturnSource {
    id: String,
    purchase_order_id: String,
}

/// 业务详情读取前只读取已命中完整来源授权的退货身份与来源身份。
fn purchase_return_identity_pipeline(id: &str, scope: &PurchaseReadScope) -> Vec<Document> {
    let mut pipeline = scope.related_stages(doc! { "id": id, "deleted_at": 0_i64 }, "$purchase_order_id");
    pipeline
        .extend([doc! { "$limit": 1 }, doc! { "$project": { "_id": 0, "id": 1, "purchase_order_id": 1 } }]);
    pipeline
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
    use std::sync::Arc;

    use application_core::AuditActor;
    use erp_core::AccountKind;
    use erp_identity::entity::access_control::{ResolvedScope, ScopeClause};
    use erp_identity::testing::fixed_authorization;
    use erp_permissions::purchase_order as purchase_order_permissions;
    use erp_procurement::PurchaseDataScope;
    use erp_procurement::authorization::PurchaseMapping;
    use mongodb::bson::Bson;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;
    use crate::purchase_center::access::PurchaseAccess;

    /// 成功编译的详情凭证必须绑定实际来源，授权完成后只交付身份投影。
    #[tokio::test]
    async fn purchase_return_identity_applies_complete_source_authorization_before_projection() {
        let client =
            mongodb::Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1").await.unwrap();
        let db = client.database("purchase_return_identity");
        let authorization = fixed_authorization(
            &[purchase_order_permissions::DETAIL],
            ResolvedScope {
                role_clauses: vec![ScopeClause { company: true, ..Default::default() }],
                user_limit: None,
            },
            vec![],
            vec![(
                purchase_order_permissions::DETAIL,
                Arc::new(PurchaseMapping::new("purchase_order").unwrap()),
            )],
        )
        .unwrap();
        let access = PurchaseAccess::new(db.clone(), PurchaseDataScope::shared(db, authorization));
        let actor = AuditActor::new("buyer".into(), "buyer".into(), AccountKind::Admin);
        let (_, scope) = access
            .resolve(&actor, &purchase_order_permissions::DETAIL, &mut SnapshotExecutor::default())
            .await
            .unwrap();
        let pipeline = purchase_return_identity_pipeline("return", &scope);
        assert_eq!(pipeline[0], doc! { "$match": { "id": "return", "deleted_at": 0_i64 } });
        assert_eq!(
            pipeline.last(),
            Some(&doc! { "$project": { "_id": 0, "id": 1, "purchase_order_id": 1 } })
        );
        let lookup = pipeline[1].get_document("$lookup").unwrap();
        assert_eq!(lookup.get_document("let").unwrap(), &doc! { "source_id": "$purchase_order_id" });
        let source = lookup.get_array("pipeline").unwrap();
        let authorization: Vec<_> = scope.stages().iter().cloned().map(Bson::Document).collect();
        assert_eq!(&source[1..1 + authorization.len()], authorization.as_slice());
        assert_eq!(pipeline[2], doc! { "$match": { "__authorized_purchase_source.0": { "$exists": true } } });
    }

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
