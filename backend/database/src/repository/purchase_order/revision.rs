//! `purchase_order_revision`(+line) 仓储：生效版本与版本行批量取回。
//!
//! 采购生效版本是不可变修订（§6.6/§4.4）：财务审核通过时由已通过提交原样复制，
//! 修订一经形成不得修改内容。版本与版本行**不提供软删除方法**。

use entities::ids::{PurchaseOrderId, PurchaseOrderRevisionId};
use entities::purchase_order::{PurchaseOrderRevision, PurchaseOrderRevisionLine};
use mongodb::bson::doc;
use mongodb::options::FindOptions;

use super::common::in_filter;
use super::{PurchaseOrderRepository, PURCHASE_ORDER_REVISIONS, PURCHASE_ORDER_REVISION_LINES};
use crate::executor::Executor;
use crate::{mongo_ops, Repository, Result};

impl<'a> PurchaseOrderRepository<'a> {
    /// 按采购单读取全部生效版本，并按版本号升序返回。
    ///
    /// # 参数
    /// * `purchase_order_id` - 采购单稳定身份
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的采购生效版本。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_revisions_by_order(
        &self,
        purchase_order_id: &PurchaseOrderId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PurchaseOrderRevision>> {
        let options = FindOptions::builder()
            .sort(doc! { "revision_no": 1, "id": 1 })
            .build();
        mongo_ops::find_many(
            &self
                .db
                .collection::<PurchaseOrderRevision>(PURCHASE_ORDER_REVISIONS),
            doc! { "purchase_order_id": purchase_order_id.to_string() },
            options,
            executor,
        )
        .await
    }

    /// 批量读取采购生效版本头。
    ///
    /// # 参数
    /// * `revision_ids` - 采购生效版本稳定身份集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回已存在的采购生效版本；空输入直接返回空集合。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_revisions_by_ids(
        &self,
        revision_ids: &[PurchaseOrderRevisionId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<PurchaseOrderRevision>> {
        if revision_ids.is_empty() {
            return Ok(Vec::new());
        }
        let options = FindOptions::builder().sort(doc! { "id": 1 }).build();
        mongo_ops::find_many(
            &self
                .db
                .collection::<PurchaseOrderRevision>(PURCHASE_ORDER_REVISIONS),
            in_filter("id", revision_ids.iter().map(ToString::to_string)),
            options,
            executor,
        )
        .await
    }

    /// 按采购生效版本读取全部明细，并按行号升序返回。
    ///
    /// # 参数
    /// * `revision_id` - 采购生效版本稳定身份
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的采购版本行。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_revision_lines(
        &self,
        revision_id: &PurchaseOrderRevisionId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PurchaseOrderRevisionLine>> {
        let options = FindOptions::builder()
            .sort(doc! { "line_no": 1, "id": 1 })
            .build();
        mongo_ops::find_many(
            &self
                .db
                .collection::<PurchaseOrderRevisionLine>(PURCHASE_ORDER_REVISION_LINES),
            doc! { "purchase_order_revision_id": revision_id.to_string() },
            options,
            executor,
        )
        .await
    }
}

impl<'a> Repository<'a, PurchaseOrderRevision> {}

impl<'a> Repository<'a, PurchaseOrderRevisionLine> {
    /// 批量取回多个版本的全部明细（`$in`，禁止 N+1）。
    ///
    /// 用于版本详情页一次取回行集合；空集合直接返回空结果。
    ///
    /// # 参数
    /// * `revision_ids` - 生效版本 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配的版本明细。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_lines_by_revision_ids(
        &self,
        revision_ids: &[PurchaseOrderRevisionId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<PurchaseOrderRevisionLine>> {
        if revision_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many(
            in_filter(
                "purchase_order_revision_id",
                revision_ids.iter().map(|id| id.to_string()),
            ),
            executor,
        )
        .await
    }
}
