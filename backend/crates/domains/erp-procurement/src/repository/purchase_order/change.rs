//! `purchase_change_order` / `purchase_change_submission`(+line) 仓储。
//!
//! 采购变更单只适用于实物与服务销售单（§6.6）；仓储影响确认与财务复核均
//! 引用不可变变更提交。变更提交/明细**不提供软删除方法**；变更单本身是
//! 可编辑单据草稿（`StableBase`），可软删除与恢复。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::ids::{PurchaseChangeOrderId, PurchaseChangeSubmissionId, PurchaseOrderId};
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Error as RepositoryError, Executor, PageResult, Result, aggregate, mongo_ops};

use super::common::in_filter;
use super::scope::PurchaseReadScope;
use super::{PURCHASE_CHANGE_ORDERS, PurchaseOrderDomainRepository};
use crate::entity::purchase_order::{
    PurchaseChangeOrder, PurchaseChangeOrderStatus, PurchaseChangeSubmission, PurchaseChangeSubmissionLine,
};

/// 已归一化的采购变更查询；授权来源条件与业务筛选分别保留。
#[derive(Debug, Clone, Copy)]
pub struct PurchaseChangeSearch<'a> {
    /// 可选来源采购单。
    pub purchase_order_id: Option<&'a str>,
    /// 可选单据状态。
    pub status: Option<&'a str>,
    /// 当前读取阶段的统一采购授权。
    pub scope: &'a PurchaseReadScope,
    /// 从一开始的页码。
    pub page: u64,
    /// 每页条数。
    pub page_size: u32,
    /// 按创建时间和稳定 ID 升序。
    pub sort_ascending: bool,
}

impl<'a> PurchaseOrderDomainRepository<'a> {
    /// 分页查询采购变更单，并按创建时间稳定排序。
    ///
    /// # 参数
    /// * `search` - 已归一化的来源单、状态、统一授权凭证及分页排序条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页采购变更单与满足条件的总数。
    ///
    /// # 错误
    /// 当 MongoDB 查询、计数或游标读取失败时返回错误。
    pub async fn search_change_orders(
        &self,
        search: PurchaseChangeSearch<'_>,
        executor: &mut dyn Executor,
    ) -> crate::Result<PageResult<PurchaseChangeOrder>> {
        let PurchaseChangeSearch { purchase_order_id, status, scope, page, page_size, sort_ascending } =
            search;
        scope.validate(executor).await?;
        let filter = change_order_filter(purchase_order_id, status);
        let stages = scope.related_stages(filter.clone(), "$purchase_order_id");
        let skip = page.saturating_sub(1).saturating_mul(u64::from(page_size));
        let direction = if sort_ascending { 1 } else { -1 };
        let options = FindOptions::builder()
            .sort(doc! { "created_at": direction, "id": direction })
            .skip(skip)
            .limit(i64::from(page_size))
            .build();
        let collection = self.db.collection::<PurchaseChangeOrder>(PURCHASE_CHANGE_ORDERS);
        let items = aggregate::find(&collection, filter.clone(), options, stages.clone(), executor).await?;
        let total = aggregate::count(&collection, filter, stages, executor).await?;
        scope.validate(executor).await?;
        let total = i64::try_from(total)
            .map_err(|_| RepositoryError::EntityMetadataOutOfRange("采购变更总数超限"))?;
        Ok(PageResult { items, total })
    }

    /// 判断采购单是否存在草稿或审批中的变更单。
    ///
    /// # 参数
    /// * `purchase_order_id` - 原采购单稳定身份
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 存在未结束变更时返回 `true`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn has_in_progress_change(
        &self,
        purchase_order_id: &PurchaseOrderId,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let filter = doc! {
            "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            "purchase_order_id": purchase_order_id.to_string(),
            "status": { "$in": [
                PurchaseChangeOrderStatus::Draft.as_str(),
                PurchaseChangeOrderStatus::Rejected.as_str(),
                PurchaseChangeOrderStatus::InApproval.as_str(),
            ] },
        };
        Ok(mongo_ops::count_documents(
            &self.db.collection::<PurchaseChangeOrder>(PURCHASE_CHANGE_ORDERS),
            filter,
            executor,
        )
        .await?
            > 0)
    }

    /// 按原采购单读取全部变更单，并按创建时间升序返回。
    ///
    /// # 参数
    /// * `purchase_order_id` - 原采购单稳定身份
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的采购变更单。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_changes_by_order(
        &self,
        purchase_order_id: &PurchaseOrderId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PurchaseChangeOrder>> {
        let options = FindOptions::builder().sort(doc! { "created_at": 1, "id": 1 }).build();
        mongo_ops::find_many(
            &self.db.collection::<PurchaseChangeOrder>(PURCHASE_CHANGE_ORDERS),
            change_order_filter(Some(purchase_order_id.as_ref()), None),
            options,
            executor,
        )
        .await
    }

    /// 装载变更单查询的有界身份与版本集合，用于跨页一致性校验。
    ///
    /// # 参数
    /// * `purchase_order_id` - 可选原采购单筛选
    /// * `status` - 可选状态代码筛选
    /// * `scope` - 当前读取阶段的统一采购授权
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 最多 10001 行；调用方必须整体拒绝超限。
    ///
    /// # 错误
    /// 数据库读取失败时返回仓储错误。
    ///
    /// # 关键业务约束
    /// 版本集合必须与列表同一授权条件和筛选快照。
    pub async fn query_change_versions(
        &self,
        purchase_order_id: Option<&str>,
        status: Option<&str>,
        scope: &PurchaseReadScope,
        executor: &mut dyn Executor,
    ) -> crate::Result<Vec<super::scope::PurchaseVersion>> {
        scope.validate(executor).await?;
        let filter = change_order_filter(purchase_order_id, status);
        let rows = aggregate::find(
            &self.db.collection::<super::scope::PurchaseVersion>(PURCHASE_CHANGE_ORDERS),
            filter.clone(),
            FindOptions::builder()
                .projection(doc! { "id": 1, "version": 1 })
                .sort(doc! { "id": 1 })
                .limit(10001)
                .build(),
            scope.related_stages(filter, "$purchase_order_id"),
            executor,
        )
        .await?;
        scope.validate(executor).await?;
        Ok(rows)
    }

    /// 按变更单读取全部提交，并按提交序号升序返回。
    ///
    /// # 参数
    /// * `change_order_id` - 采购变更单稳定身份
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的变更提交。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_change_submissions_by_order(
        &self,
        change_order_id: &PurchaseChangeOrderId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PurchaseChangeSubmission>> {
        let options = FindOptions::builder().sort(doc! { "submission_no": 1, "id": 1 }).build();
        mongo_ops::find_many(
            &self.db.collection::<PurchaseChangeSubmission>(super::PURCHASE_CHANGE_SUBMISSIONS),
            doc! { "purchase_change_order_id": change_order_id.to_string() },
            options,
            executor,
        )
        .await
    }

    /// 按变更提交读取全部明细，并按行号升序返回。
    ///
    /// # 参数
    /// * `submission_id` - 采购变更提交稳定身份
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的变更提交行。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_change_submission_lines(
        &self,
        submission_id: &PurchaseChangeSubmissionId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PurchaseChangeSubmissionLine>> {
        let options = FindOptions::builder().sort(doc! { "line_no": 1, "id": 1 }).build();
        mongo_ops::find_many(
            &self.db.collection::<PurchaseChangeSubmissionLine>(super::PURCHASE_CHANGE_SUBMISSION_LINES),
            doc! { "purchase_change_submission_id": submission_id.to_string() },
            options,
            executor,
        )
        .await
    }
}

/// 采购变更提交集合的域查询。
#[allow(async_fn_in_trait)]
pub trait PurchaseChangeSubmissionRepositoryExt {
    /// 批量读取指定变更单的全部有效提交，供审批历史摘要使用。
    ///
    /// # 参数
    /// `change_order_ids` 为父变更单身份集合；`executor` 决定事务上下文。
    /// # 返回
    /// 返回全部未删除提交；空集合返回空结果，不扫描全表。
    /// # 错误
    /// MongoDB 查询失败时返回仓储错误。
    async fn list_by_change_orders(
        &self,
        change_order_ids: &[PurchaseChangeOrderId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<PurchaseChangeSubmission>>;

    /// 按「变更单 + 提交序号」查找唯一变更提交。
    ///
    /// 唯一性由 `uk_purchase_change_submissions_order_no` 唯一索引保证。
    ///
    /// # 参数
    /// * `purchase_change_order_id` - 所属采购变更单
    /// * `submission_no` - 提交序号
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的变更提交；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    async fn find_by_order_and_submission_no(
        &self,
        purchase_change_order_id: &PurchaseChangeOrderId,
        submission_no: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<PurchaseChangeSubmission>>;
}

impl PurchaseChangeSubmissionRepositoryExt for persistence_core::Repository<'_, PurchaseChangeSubmission> {
    async fn list_by_change_orders(
        &self,
        change_order_ids: &[PurchaseChangeOrderId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<PurchaseChangeSubmission>> {
        if change_order_ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids = change_order_ids.iter().map(ToString::to_string).collect::<Vec<_>>();
        self.find_many(
            doc! { "purchase_change_order_id": { "$in": ids }, "deleted_at": NOT_DELETED_TIMESTAMP_BSON },
            executor,
        )
        .await
    }

    async fn find_by_order_and_submission_no(
        &self,
        purchase_change_order_id: &PurchaseChangeOrderId,
        submission_no: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<PurchaseChangeSubmission>> {
        self.find_one(
            doc! {
                "purchase_change_order_id": purchase_change_order_id.to_string(),
                "submission_no": submission_no,
            },
            executor,
        )
        .await
    }
}

/// 业务筛选只收窄来源授权查询，不包含人员范围推断。
fn change_order_filter(purchase_order_id: Option<&str>, status: Option<&str>) -> Document {
    let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
    if let Some(status) = status {
        filter.insert("status", status);
    }
    if let Some(purchase_order_id) = purchase_order_id {
        filter.insert("purchase_order_id", purchase_order_id);
    }
    filter
}

/// 采购变更提交行集合的域查询。
#[allow(async_fn_in_trait)]
pub trait PurchaseChangeSubmissionLineRepositoryExt {
    /// 批量取回多个变更提交的全部明细（`$in`，禁止 N+1）。
    ///
    /// 用于变更提交详情页一次取回行集合；空集合直接返回空结果。
    ///
    /// # 参数
    /// * `submission_ids` - 变更提交 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配的变更提交明细。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    async fn find_lines_by_submission_ids(
        &self,
        submission_ids: &[PurchaseChangeSubmissionId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<PurchaseChangeSubmissionLine>>;
}

impl PurchaseChangeSubmissionLineRepositoryExt
    for persistence_core::Repository<'_, PurchaseChangeSubmissionLine>
{
    async fn find_lines_by_submission_ids(
        &self,
        submission_ids: &[PurchaseChangeSubmissionId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<PurchaseChangeSubmissionLine>> {
        if submission_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many(
            in_filter("purchase_change_submission_id", submission_ids.iter().map(|id| id.to_string())),
            executor,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use entity_core::NOT_DELETED_TIMESTAMP_BSON;
    use mongodb::bson::doc;

    use super::change_order_filter;

    #[test]
    fn change_filter_preserves_requested_order_and_status() {
        assert_eq!(
            change_order_filter(Some("po-1"), Some("draft")),
            doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON, "purchase_order_id": "po-1", "status": "draft" }
        );
        assert_eq!(change_order_filter(None, None), doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON });
    }
}
