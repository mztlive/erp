//! 已由 identity 查询得到的客户主键与业务筛选。
#![allow(async_fn_in_trait)]
use erp_identity::CompiledAccess;
use mongodb::Database;
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{
    Executor, PageResult, Pagination, QueryFilter, Repository, Result as RepositoryResult, aggregate,
    mongo_ops,
};
use serde::Deserialize;

use super::CustomerExt;
use super::customer::{CustomerAccountFilter, CustomerAccountRow};
use super::customer_shared::{customer_account_projection, sort_doc};
use crate::entity::customer::{CustomerAccount, CustomerAssignment};
use crate::ports::{CustomerDataScopePort, CustomerResolvedScope};
use crate::{Error, Result};

/// 完整授权查询结果；不含角色、范围条款或个人上限。
#[derive(Debug, Clone, Default)]
pub struct CustomerReadScope {
    pub authorized_customer_ids: Option<Vec<String>>,
    pub owned_customer_ids: Vec<String>,
    pub collaborative_customer_ids: Vec<String>,
}
/// 跨页校验使用的客户身份和版本，不包含展示字段。
#[derive(Debug, Deserialize)]
pub struct CustomerVersion {
    /// 客户稳定主键。
    pub id: String,
    /// 客户乐观锁版本。
    pub version: u64,
}

impl CustomerReadScope {
    /// 将已执行 identity 查询得到的客户集合用于业务筛选。
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回完整查询结果的主键条件。
    /// # 错误
    /// 无。
    pub fn document(&self) -> Document {
        match &self.authorized_customer_ids {
            Some(ids) => doc! { "id": { "$in": ids } },
            None => doc! { "$expr": false },
        }
    }
    /// 判断已经查询的授权结果是否为空。
    /// # 参数
    /// 无。
    /// # 返回
    /// 空结果为 true。
    /// # 错误
    /// 无。
    pub fn is_empty(&self) -> bool {
        self.authorized_customer_ids.as_ref().is_none_or(Vec::is_empty)
    }
}

/// 客户账户集合上的授权范围查询。
#[allow(async_fn_in_trait)]
pub trait CustomerAccountRepositoryScopeExt {
    /// 按明确授权条件读取单个客户。
    ///
    /// # 参数
    /// * `id` - 客户稳定主键
    /// * `scope` - 已证明的授权条件
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 不存在或不在范围内均返回 `None`。
    ///
    /// # 错误
    /// 数据库读取失败时返回仓储错误。
    ///
    /// # 关键业务约束
    /// ID 条件不能替换范围交集；空授权不得返回文档。
    async fn find_authorized(
        &self,
        id: &str,
        scope: &CustomerReadScope,
        executor: &mut dyn Executor,
    ) -> RepositoryResult<Option<CustomerAccount>>;

    /// 装载查询的有界身份与版本集合，用于跨页及导出一致性校验。
    ///
    /// # 参数
    /// * `filter` - 已与授权条件求交的列表筛选
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 最多 10001 行；调用方必须整体拒绝超限。
    ///
    /// # 错误
    /// 数据库读取失败时返回仓储错误。
    ///
    /// # 关键业务约束
    /// 不得截断版本集合后继续拼接跨页结果。
    async fn query_versions(
        &self,
        filter: &CustomerAccountFilter,
        executor: &mut dyn Executor,
    ) -> RepositoryResult<Vec<CustomerVersion>>;
}

impl CustomerAccountRepositoryScopeExt for Repository<'_, CustomerAccount> {
    async fn find_authorized(
        &self,
        id: &str,
        scope: &CustomerReadScope,
        executor: &mut dyn Executor,
    ) -> RepositoryResult<Option<CustomerAccount>> {
        self.find_one(doc! { "$and": [{ "id": id }, scope.document()] }, executor).await
    }

    async fn query_versions(
        &self,
        filter: &CustomerAccountFilter,
        executor: &mut dyn Executor,
    ) -> RepositoryResult<Vec<CustomerVersion>> {
        mongo_ops::find_many(
            &self.collection().clone_with_type::<CustomerVersion>(),
            filter.to_doc(),
            FindOptions::builder()
                .projection(doc! { "id": 1, "version": 1 })
                .sort(doc! { "id": 1 })
                .limit(10001)
                .build(),
            executor,
        )
        .await
    }
}

/// 客户归属集合上的授权范围查询。
#[allow(async_fn_in_trait)]
pub trait CustomerAssignmentRepositoryScopeExt {
    /// 读取某账号全部未删除归属，供读取动作补充历史参与。
    ///
    /// # 参数
    /// * `user_id` - 销售人员
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回该人员全部归属行，含已结束窗口。
    ///
    /// # 错误
    /// 数据库读取失败时返回仓储错误。
    ///
    /// # 关键业务约束
    /// 历史参与只用于读取；调用方不得把本结果当作修改资格。
    async fn list_for_user(
        &self,
        user_id: &str,
        executor: &mut dyn Executor,
    ) -> RepositoryResult<Vec<CustomerAssignment>>;
}

impl CustomerAssignmentRepositoryScopeExt for Repository<'_, CustomerAssignment> {
    async fn list_for_user(
        &self,
        user_id: &str,
        executor: &mut dyn Executor,
    ) -> RepositoryResult<Vec<CustomerAssignment>> {
        self.find_many(doc! { "user_id": user_id }, executor).await
    }
}

/// 执行 identity 的完整查询阶段，返回有界客户主键。
/// # 参数
/// db 为客户仓储，access 为本阶段凭证，id 可收窄单一目标，executor 为原事务。
/// # 返回
/// 已授权且实际存在的客户主键。
/// # 错误
/// 超限或数据库读取失败时拒绝，不截断授权集合。
pub async fn authorized_ids(
    db: &Database,
    access: &CompiledAccess,
    id: Option<&str>,
    executor: &mut dyn Executor,
) -> Result<Vec<String>> {
    let mut pipeline = Vec::new();
    if let Some(id) = id {
        pipeline.push(doc! {"$match": {"id": id}});
    }
    pipeline.extend_from_slice(access.stages());
    pipeline.extend([doc! {"$project": {"id": 1, "version": 1}}, doc! {"$limit": 10001}]);
    let collection = db.customer_accounts().collection().clone_with_type::<CustomerVersion>();
    let rows = aggregate::rows(&collection, pipeline, executor).await?;
    if rows.len() > 10000 {
        return Err(Error::ValidationError("客户查询超过上限，请收窄授权范围".into()));
    }
    Ok(rows.into_iter().map(|row| row.id).collect())
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
    port: &dyn CustomerDataScopePort,
    executor: &mut dyn Executor,
) -> Result<PageResult<CustomerAccountRow>> {
    let compiled = compiled(context)?;
    port.validate(context, executor).await?;
    let collection = db.customer_accounts().collection().clone_with_type::<CustomerAccountRow>();
    let options = page_options(filter);
    let items =
        aggregate::find(&collection, filter.to_doc(), options, compiled.stages().to_vec(), executor).await?;
    let total = aggregate::count(&collection, filter.to_doc(), compiled.stages().to_vec(), executor).await?;
    port.validate(context, executor).await?;
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
    port: &dyn CustomerDataScopePort,
    executor: &mut dyn Executor,
) -> Result<Vec<CustomerVersion>> {
    let compiled = compiled(context)?;
    port.validate(context, executor).await?;
    let mut stages = vec![doc! {"$match": filter.to_doc()}];
    stages.extend_from_slice(compiled.stages());
    stages.extend([
        doc! {"$sort": {"id": 1}},
        doc! {"$project": {"id": 1, "version": 1}},
        doc! {"$limit": 10001},
    ]);
    let collection = db.customer_accounts().collection().clone_with_type::<CustomerVersion>();
    let rows = aggregate::rows(&collection, stages, executor).await?;
    port.validate(context, executor).await?;
    Ok(rows)
}

fn compiled(context: &CustomerResolvedScope) -> Result<&CompiledAccess> {
    context.compiled.as_deref().ok_or_else(|| Error::Internal("客户查询缺少凭证".into()))
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::Instant;

    use super::*;

    #[test]
    fn unmaterialized_or_empty_customer_results_never_become_an_unrestricted_filter() {
        let mut scope = CustomerReadScope::default();
        assert!(scope.is_empty());
        assert_eq!(scope.document(), doc! {"$expr": false});
        scope.authorized_customer_ids = Some(Vec::new());
        assert!(scope.is_empty());
        assert_eq!(scope.document(), doc! {"id": {"$in": Vec::<String>::new()}});
        scope.authorized_customer_ids = Some(vec!["customer".into()]);
        assert!(!scope.is_empty());
        assert_eq!(scope.document(), doc! {"id": {"$in": ["customer"]}});
    }

    #[test]
    fn missing_compiled_access_is_an_explicit_error() {
        let context = CustomerResolvedScope {
            user_id: "sales".into(),
            current: false,
            scope_version: String::new(),
            as_of: Instant::from_unix_secs(0),
            compiled: None,
        };
        assert!(matches!(compiled(&context), Err(Error::Internal(_))));
    }

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
}
