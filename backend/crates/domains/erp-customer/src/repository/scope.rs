//! 客户业务归属筛选与 identity 授权查询执行。
#![allow(async_fn_in_trait)]
use erp_identity::entity::authorization::{QueryModel, QueryOutput};
use mongodb::Database;
use mongodb::bson::doc;
use mongodb::options::FindOptions;
use persistence_core::{Executor, PageResult, Pagination, QueryFilter, aggregate};
use serde::Deserialize;

use super::CustomerExt;
use super::customer::{CustomerAccountFilter, CustomerAccountRow};
use super::customer_shared::{customer_account_projection, sort_doc};
use crate::entity::customer::CustomerAccount;
use crate::{CustomerAccess, CustomerResolvedScope, Error, Result};

/// 在完整授权条件内读取指定客户，缺失与不可读使用相同错误。
/// # 参数
/// db、id 为实际客户集合与主键；context、executor 属于当前读取阶段。
/// # 返回
/// 本次完整数据规则允许读取的客户实体。
/// # 错误
/// 模型、阶段或有效期不符时拒绝；无可见记录返回 NotFound。
pub async fn authorized_customer(
    db: &Database,
    id: &str,
    context: &CustomerResolvedScope,
    executor: &mut dyn Executor,
) -> Result<CustomerAccount> {
    let access = &context.compiled;
    ensure_model(access.model(), "customer", Database::CUSTOMER_ACCOUNTS)?;
    access.ensure_current(executor)?;
    let collection = db.customer_accounts().collection();
    let options = FindOptions::builder().limit(1).build();
    let rows =
        aggregate::find(&collection, doc! {"id":id}, options, access.stages().to_vec(), executor).await?;
    access.ensure_current(executor)?;
    rows.into_iter().next().ok_or_else(|| Error::NotFound("客户不存在".into()))
}

/// 客户仓储只消费本集合、主键和完整对象粒度的固定查询模型。
fn ensure_model(model: &QueryModel, resource: &str, collection: &str) -> Result<()> {
    if model.resource != resource
        || model.collection != collection
        || model.id_field != "id"
        || model.output != QueryOutput::Object
    {
        return Err(Error::ValidationError("客户查询模型不匹配".into()));
    }
    Ok(())
}

/// 当前主责客户业务筛选；授权条件由 identity 凭证提供。
#[derive(Debug, Clone, Default)]
pub struct CustomerReadScope {
    pub owned_customer_ids: Vec<String>,
}
/// 跨页校验使用的客户身份和版本，不包含展示字段。
#[derive(Debug, Deserialize)]
pub struct CustomerVersion {
    /// 客户稳定主键。
    pub id: String,
    /// 客户乐观锁版本。
    pub version: u64,
}

/// 在相同完整授权管道内执行客户分页与计数。
/// # 参数
/// db、filter 为客户仓储及业务筛选，context、port 为本阶段授权，executor 为原事务。
/// # 返回
/// 完整授权范围内的客户页。
/// # 错误
/// 凭证失效、数据库失败或数值越界时拒绝。
pub async fn authorized_page(
    db: &Database,
    filter: &CustomerAccountFilter,
    context: &CustomerResolvedScope,
    _port: &CustomerAccess,
    executor: &mut dyn Executor,
) -> Result<PageResult<CustomerAccountRow>> {
    let compiled = &context.compiled;
    ensure_model(compiled.model(), "customer", Database::CUSTOMER_ACCOUNTS)?;
    compiled.ensure_current(executor)?;
    let collection = db.customer_accounts().collection().clone_with_type::<CustomerAccountRow>();
    let options = page_options(filter);
    let items =
        aggregate::find(&collection, filter.to_doc(), options, compiled.stages().to_vec(), executor).await?;
    let total = aggregate::count(&collection, filter.to_doc(), compiled.stages().to_vec(), executor).await?;
    compiled.ensure_current(executor)?;
    let total = i64::try_from(total).map_err(|_| Error::ValidationError("客户总数超限".into()))?;
    Ok(PageResult { items, total })
}

fn page_options(filter: &CustomerAccountFilter) -> FindOptions {
    FindOptions::builder()
        .sort(sort_doc(
            filter.sort_by.as_deref(),
            filter.sort_ascending,
            &["created_at", "updated_at", "customer_no", "status"],
        ))
        .skip(filter.skip())
        .limit(filter.limit())
        .projection(customer_account_projection())
        .build()
}

/// 在业务条件收窄后读取完整授权客户身份版本。
/// # 参数
/// db、filter 为客户仓储及筛选，context、port 为本阶段授权，executor 为原事务。
/// # 返回
/// 有界、稳定排序的授权客户版本。
/// # 错误
/// 数据库或凭证复验失败时拒绝；超限由调用方整体拒绝。
pub async fn authorized_versions(
    db: &Database,
    filter: &CustomerAccountFilter,
    context: &CustomerResolvedScope,
    _port: &CustomerAccess,
    executor: &mut dyn Executor,
) -> Result<Vec<CustomerVersion>> {
    let compiled = &context.compiled;
    ensure_model(compiled.model(), "customer", Database::CUSTOMER_ACCOUNTS)?;
    compiled.ensure_current(executor)?;
    let mut stages = vec![doc! {"$match": filter.to_doc()}];
    stages.extend_from_slice(compiled.stages());
    stages.extend([
        doc! {"$sort": {"id": 1}},
        doc! {"$project": {"id": 1, "version": 1}},
        doc! {"$limit": 10001},
    ]);
    let collection = db.customer_accounts().collection().clone_with_type::<CustomerVersion>();
    let rows = aggregate::rows(&collection, stages, executor).await?;
    compiled.ensure_current(executor)?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use erp_identity::entity::authorization::QueryModelKey;

    use super::*;

    #[test]
    fn authorized_page_uses_bounded_paging_and_stable_sort_fallback() {
        let filter = CustomerAccountFilter {
            page: 3,
            page_size: 20,
            sort_by: Some("untrusted_field".into()),
            sort_ascending: false,
            ..Default::default()
        };
        let options = page_options(&filter);
        assert_eq!(options.skip, Some(40));
        assert_eq!(options.limit, Some(20));
        assert_eq!(options.sort, Some(doc! {"created_at": -1, "id": -1}));
        assert!(options.projection.unwrap().contains_key("id"));
    }

    #[test]
    fn customer_repository_rejects_foreign_collection_and_partial_output() {
        let model = QueryModel {
            key: QueryModelKey::parse("customer_accounts").unwrap(),
            resource: "customer".into(),
            collection: Database::CUSTOMER_ACCOUNTS.into(),
            id_field: "id".into(),
            output: QueryOutput::Object,
            facts: Default::default(),
        };
        assert!(ensure_model(&model, "customer", Database::CUSTOMER_ACCOUNTS).is_ok());
        for wrong in [
            QueryModel { resource: "supplier".into(), ..model.clone() },
            QueryModel { collection: Database::CUSTOMER_ASSIGNMENTS.into(), ..model.clone() },
            QueryModel { id_field: "customer_id".into(), ..model.clone() },
            QueryModel { output: QueryOutput::Existence { fields: ["id".into()].into() }, ..model },
        ] {
            assert!(matches!(
                ensure_model(&wrong, "customer", Database::CUSTOMER_ACCOUNTS),
                Err(Error::ValidationError(_))
            ));
        }
    }
}
