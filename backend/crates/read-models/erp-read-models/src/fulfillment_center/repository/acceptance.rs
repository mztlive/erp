//! 验收业务筛选与来源销售完整授权在数据库内求交。
use application_core::{SortDir, normalize_paging};
use erp_fulfillment::dto::{CustomerAcceptanceListParams, CustomerAcceptanceView, PageView};
use erp_fulfillment::repository::FulfillmentExt;
use erp_fulfillment::repository::fulfillment::{CustomerAcceptanceFilter, CustomerAcceptanceRow};
use erp_sales::repository::sales_order::scope::SalesReadScope;
use mongodb::Database;
use mongodb::bson::{Document, doc};
use persistence_core::{Executor, Pagination, QueryFilter, aggregate};
use serde::Deserialize;
use validator::Validate;

use crate::{Error, Result};

/// 在同一读取阶段查询完整授权后的验收分页和总数。
/// # 参数
/// db 为数据库；params 为业务筛选；scope 为来源销售凭证；executor 为原读取事务。
/// # 返回
/// 业务筛选与完整来源授权交集中的分页结果。
/// # 错误
/// 参数非法、凭证失效或数据库读取失败时返回错误。
pub(crate) async fn acceptance_list(
    db: &Database,
    params: &CustomerAcceptanceListParams,
    scope: &SalesReadScope,
    executor: &mut dyn Executor,
) -> Result<PageView<CustomerAcceptanceView>> {
    let filter = acceptance_filter(params)?;
    scope.ensure_current(executor)?;
    let prefix = acceptance_prefix(&filter, scope.related_stages("sales_order_id"));
    let collection = db.collection::<CustomerAcceptanceRow>(Database::CUSTOMER_ACCEPTANCES);
    let rows = aggregate::rows(&collection, page_pipeline(prefix.clone(), &filter)?, executor).await?;
    let total = aggregate::count(&collection, doc! {}, prefix, executor).await?;
    scope.ensure_current(executor)?;
    let total = i64::try_from(total).map_err(|_| Error::ValidationError("验收总数超过上限".into()))?;
    Ok(PageView {
        items: rows.into_iter().map(acceptance_view).collect(),
        total,
        page: filter.page,
        page_size: filter.page_size,
    })
}

/// 只返回已通过实际销售来源完整授权的验收身份。
/// # 参数
/// db 为验收事实数据库；id 为精确验收身份；scope 为原 DETAIL 凭证；executor 为原事务。
/// # 返回
/// 来源存在且授权命中时返回验收身份，否则返回 None；不装载验收业务字段。
/// # 错误
/// 凭证失效、模型不符或数据库读取失败时拒绝。
pub(crate) async fn acceptance_identity(
    db: &Database,
    id: &str,
    scope: &SalesReadScope,
    executor: &mut dyn Executor,
) -> Result<Option<String>> {
    scope.ensure_current(executor)?;
    let identities = aggregate::rows(
        &db.collection::<AcceptanceIdentity>(Database::CUSTOMER_ACCEPTANCES),
        acceptance_identity_pipeline(id, scope),
        executor,
    )
    .await?;
    scope.ensure_current(executor)?;
    Ok(identities.into_iter().next().map(|identity| identity.id))
}

/// 只在来源对象授权命中后交付验收身份，不携带验收业务字段。
#[derive(Deserialize)]
struct AcceptanceIdentity {
    id: String,
}

/// 绑定精确验收身份及真实来源，完整授权先于身份投影。
fn acceptance_identity_pipeline(id: &str, scope: &SalesReadScope) -> Vec<Document> {
    let mut pipeline = vec![doc! { "$match": { "id": id, "deleted_at": 0_i64 } }];
    pipeline.extend(scope.related_stages("sales_order_id"));
    pipeline.extend([doc! { "$limit": 1 }, doc! { "$project": { "_id": 0, "id": 1 } }]);
    pipeline
}

fn acceptance_filter(params: &CustomerAcceptanceListParams) -> Result<CustomerAcceptanceFilter> {
    params.validate()?;
    let paging = normalize_paging(
        &params.sort_by,
        &params.sort_dir,
        params.page,
        params.page_size,
        &["accepted_at", "created_at"],
    )?;
    Ok(CustomerAcceptanceFilter {
        sales_order_id: params.sales_order_id.clone(),
        status: params.status,
        page: paging.page,
        page_size: paging.page_size,
        sort_by: Some(paging.sort_by.into()),
        sort_ascending: matches!(paging.sort_dir, SortDir::Asc),
    })
}

fn acceptance_prefix(filter: &CustomerAcceptanceFilter, authorization: Vec<Document>) -> Vec<Document> {
    let mut pipeline = vec![doc! { "$match": filter.to_doc() }];
    pipeline.extend(authorization);
    pipeline
}

fn page_pipeline(mut prefix: Vec<Document>, filter: &CustomerAcceptanceFilter) -> Result<Vec<Document>> {
    let skip =
        i64::try_from(filter.skip()).map_err(|_| Error::ValidationError("验收分页偏移超过上限".into()))?;
    prefix.extend([
        doc! { "$sort": { filter.sort_by.as_deref().unwrap_or("created_at"): if filter.sort_ascending {1} else {-1} } },
        doc! { "$skip": skip },
        doc! { "$limit": filter.limit() },
        doc! { "$project": CustomerAcceptanceRow::projection() },
    ]);
    Ok(prefix)
}

fn acceptance_view(row: CustomerAcceptanceRow) -> CustomerAcceptanceView {
    CustomerAcceptanceView {
        id: row.id,
        acceptance_no: row.acceptance_no,
        sales_order_id: row.sales_order_id.to_string(),
        accepted_at: row.accepted_at.unix_secs(),
        result: row.result,
        evidence_attachment_id: row.evidence_attachment_id.map(|id| id.to_string()),
        status: row.status,
        reversal_of_acceptance_id: row.reversal_of_acceptance_id.map(|id| id.to_string()),
        version: row.version,
        created_at: row.created_at,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use application_core::AuditActor;
    use erp_core::AccountKind;
    use erp_core::ids::SalesOrderId;
    use erp_fulfillment::entity::fulfillment::CustomerAcceptanceState;
    use erp_identity::entity::access_control::{ResolvedScope, ScopeClause};
    use erp_identity::testing::fixed_authorization;
    use erp_permissions::sales_order as sales_order_permissions;
    use mongodb::bson::{Bson, deserialize_from_document};
    use persistence_core::testing::SnapshotExecutor;

    use super::*;
    use crate::sales_center::access::SalesAccess;
    use crate::sales_center::repository::authorization::SalesMapping;

    /// 成功编译的详情凭证只连接当前验收的真实销售来源，授权后仅投影身份。
    #[tokio::test]
    async fn acceptance_identity_applies_complete_detail_authorization_before_projection() {
        let client =
            mongodb::Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1").await.unwrap();
        let db = client.database("acceptance_query");
        let authorization = fixed_authorization(
            &[sales_order_permissions::DETAIL],
            ResolvedScope {
                role_clauses: vec![ScopeClause { self_owned: true, ..Default::default() }],
                user_limit: None,
            },
            vec![],
            vec![(
                sales_order_permissions::DETAIL,
                Arc::new(SalesMapping::new(db.clone(), "sales_order", "acceptance_sales").unwrap()),
            )],
        )
        .unwrap();
        let actor = AuditActor::new("sales".into(), "sales".into(), AccountKind::Admin);
        let (_, scope) = SalesAccess::new(db, authorization)
            .resolve(&actor, &sales_order_permissions::DETAIL, &mut SnapshotExecutor::default())
            .await
            .unwrap();
        let pipeline = acceptance_identity_pipeline("acceptance", &scope);
        assert_eq!(pipeline[0], doc! { "$match": { "id": "acceptance", "deleted_at": 0_i64 } });
        assert_eq!(&pipeline[1..pipeline.len() - 2], scope.related_stages("sales_order_id").as_slice());
        assert_eq!(pipeline.last(), Some(&doc! { "$project": { "_id": 0, "id": 1 } }));
        let lookup = pipeline[1].get_document("$lookup").unwrap();
        assert_eq!(lookup.get_document("let").unwrap(), &doc! { "sales_source_id": "$sales_order_id" });
        let source = lookup.get_array("pipeline").unwrap();
        let authorization: Vec<_> = scope.stages().into_iter().map(Bson::Document).collect();
        assert_eq!(&source[1..1 + authorization.len()], authorization.as_slice());
    }

    fn params() -> CustomerAcceptanceListParams {
        CustomerAcceptanceListParams {
            sales_order_id: Some(SalesOrderId::new("sales-one")),
            status: Some(CustomerAcceptanceState::Posted),
            page: Some(2),
            page_size: Some(20),
            sort_by: Some("accepted_at".into()),
            sort_dir: Some("asc".into()),
        }
    }

    #[test]
    fn acceptance_query_filters_actual_source_before_authorization_and_pagination() {
        let filter = acceptance_filter(&params()).unwrap();
        let authorization = vec![doc! {"$match": {"authorized_source": true}}];
        let prefix = acceptance_prefix(&filter, authorization.clone());
        assert_eq!(
            prefix[0],
            doc! { "$match": { "deleted_at": 0_i64, "sales_order_id": "sales-one", "status": "POSTED" } }
        );
        assert_eq!(&prefix[1..], authorization.as_slice());
        let page = page_pipeline(prefix.clone(), &filter).unwrap();
        assert_eq!(&page[..prefix.len()], prefix.as_slice());
        assert_eq!(page[page.len() - 3], doc! { "$skip": 20_i64 });
        assert_eq!(page[page.len() - 2], doc! { "$limit": 20_i64 });
        assert_eq!(page.last().unwrap(), &doc! { "$project": CustomerAcceptanceRow::projection() });
    }

    #[test]
    fn acceptance_filter_keeps_requested_order_and_rejects_invalid_paging() {
        let filter = acceptance_filter(&params()).unwrap();
        assert_eq!(filter.sales_order_id, Some(SalesOrderId::new("sales-one")));
        let mut invalid = params();
        invalid.page_size = Some(101);
        assert!(acceptance_filter(&invalid).is_err());
        invalid.page_size = Some(20);
        invalid.sort_by = Some("unknown".into());
        assert!(acceptance_filter(&invalid).is_err());
    }

    #[test]
    fn acceptance_list_decodes_only_the_existing_projection_contract() {
        let mut projected = doc! {
            "id": "acceptance-1",
            "acceptance_no": "CA-1",
            "sales_order_id": "sales-one",
            "accepted_at": 1_700_000_000_i64,
            "result": "PASSED",
            "status": "POSTED",
            "version": 2_i64,
            "created_at": 1_699_999_999_i64,
        };
        let view =
            acceptance_view(deserialize_from_document::<CustomerAcceptanceRow>(projected.clone()).unwrap());
        assert_eq!(view.id, "acceptance-1");
        assert_eq!(view.sales_order_id, "sales-one");
        assert_eq!(view.accepted_at, 1_700_000_000);
        assert_eq!(view.version, 2);
        assert_eq!(view.created_at, 1_699_999_999);
        assert!(view.evidence_attachment_id.is_none());
        assert!(view.reversal_of_acceptance_id.is_none());
        projected.remove("id");
        assert!(deserialize_from_document::<CustomerAcceptanceRow>(projected).is_err());
    }
}
