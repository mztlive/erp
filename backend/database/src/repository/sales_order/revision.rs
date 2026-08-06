//! `sales_order_revision` 及版本行、两个版本行子类型仓储。
//!
//! 正式版本是不可变修订（事实类），**不提供软删除方法**；生效版本业务字段
//! 不可更新（数据模型 §6.4）。`previous_revision_id` 必须属于同一销售单由
//! P3 在形成版本时校验。

use entities::sales_order::{
    SalesOrderGoodsServiceLineRevision, SalesOrderId, SalesOrderRevision, SalesOrderRevisionId,
    SalesOrderRevisionLine, SalesOrderRevisionLineId, SalesOrderVoucherLineRevision,
};
use mongodb::bson::doc;

use super::super::Repository;
use super::{
    SalesOrderRepository, SALES_ORDERS, SALES_ORDER_GOODS_SERVICE_LINE_REVISIONS, SALES_ORDER_REVISIONS,
    SALES_ORDER_REVISION_LINES, SALES_ORDER_VOUCHER_LINE_REVISIONS,
};
use crate::executor::Executor;
use crate::{mongo_ops, Result};

impl<'a> Repository<'a, SalesOrderRevision> {
    /// 按销售单与版本号查找正式版本。
    ///
    /// 唯一性由 `uk_sales_order_revisions_order_revision_no` 唯一索引保证。
    ///
    /// # 参数
    /// * `sales_order_id` - 稳定销售单
    /// * `revision_no` - 聚合内版本号
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的正式版本；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_order_and_no(
        &self,
        sales_order_id: &SalesOrderId,
        revision_no: u32,
        executor: &mut dyn Executor,
    ) -> Result<Option<SalesOrderRevision>> {
        self.find_one(
            doc! {
                "sales_order_id": sales_order_id.to_string(),
                "revision_no": revision_no as i32,
            },
            executor,
        )
        .await
    }

    /// 列出销售单的版本历史（新版本在前）。
    ///
    /// # 参数
    /// * `sales_order_id` - 稳定销售单
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回按版本号倒序的正式版本列表。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_by_order(
        &self,
        sales_order_id: &SalesOrderId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SalesOrderRevision>> {
        self.find_many_sorted(
            doc! { "sales_order_id": sales_order_id.to_string() },
            doc! { "revision_no": -1 },
            executor,
        )
        .await
    }

    /// 按「销售单 + 内容指纹」查找版本（幂等与历史查询，数据模型 §6.4）。
    ///
    /// 指纹相同的内容可能被合法重建（如同一内容重新提交），因此索引
    /// `idx_sales_order_revisions_order_content_hash` 是普通索引而非唯一索引；
    /// 幂等判定由 P3 结合来源快照与业务上下文完成。
    ///
    /// # 参数
    /// * `sales_order_id` - 稳定销售单
    /// * `content_hash` - 本版全部商业字段的规范化指纹
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回最新匹配版本；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_content_hash(
        &self,
        sales_order_id: &SalesOrderId,
        content_hash: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SalesOrderRevision>> {
        mongo_ops::find_one(
            &self.collection(),
            doc! {
                "sales_order_id": sales_order_id.to_string(),
                "content_hash": content_hash,
                "deleted_at": 0,
            },
            executor,
        )
        .await
    }
}

impl<'a> Repository<'a, SalesOrderRevisionLine> {
    /// 列出版本的全部公共行版本（按行号升序）。
    ///
    /// # 参数
    /// * `revision_id` - 所属销售版本
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回按行号升序的公共行版本列表。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_lines_by_revision(
        &self,
        revision_id: &SalesOrderRevisionId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SalesOrderRevisionLine>> {
        self.find_many_sorted(
            doc! { "sales_order_revision_id": revision_id.to_string() },
            doc! { "line_no": 1 },
            executor,
        )
        .await
    }
}

impl<'a> Repository<'a, SalesOrderGoodsServiceLineRevision> {
    /// 按公共行版本 ID 集合批量取回实物及服务行（`$in` 一次取回，禁止 N+1）。
    ///
    /// # 参数
    /// * `revision_line_ids` - 公共行版本 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配子类型行（未排序，调用方按需分组）。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_by_revision_line_ids(
        &self,
        revision_line_ids: &[SalesOrderRevisionLineId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SalesOrderGoodsServiceLineRevision>> {
        if revision_line_ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids = revision_line_ids
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>();
        self.find_many(doc! { "revision_line_id": { "$in": ids } }, executor)
            .await
    }
}

impl<'a> Repository<'a, SalesOrderVoucherLineRevision> {
    /// 按公共行版本 ID 集合批量取回卡券行（`$in` 一次取回，禁止 N+1）。
    ///
    /// # 参数
    /// * `revision_line_ids` - 公共行版本 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配子类型行（未排序，调用方按需分组）。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_by_revision_line_ids(
        &self,
        revision_line_ids: &[SalesOrderRevisionLineId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SalesOrderVoucherLineRevision>> {
        if revision_line_ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids = revision_line_ids
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>();
        self.find_many(doc! { "revision_line_id": { "$in": ids } }, executor)
            .await
    }
}

impl<'a> SalesOrderRepository<'a> {
    /// 生效提交：把提交快照原样写成正式版本及版本行，并把销售单推进到生效态。
    ///
    /// 依次写入 `sales_order_revision`、`sales_order_revision_line` 与两个子类型
    /// 集合，再 CAS 更新销售单（数据模型 §6.4/§6.5：不改写旧版本，卡券单恰好
    /// 一条卡券行的断言由 P3 在形成版本时校验）。调用方须先在 `SalesOrder`
    /// 实体上完成 `approve` + `attach_revision` 状态迁移（本层不做业务判定）。
    /// **必须收到事务执行器**：本方法不构成原子边界，传入 `NoTransaction` 时
    /// 中途失败会留下缺明细的版本或状态未推进的销售单；Service 必须通过
    /// `database::Transactional::with_transaction` 传入事务会话。
    ///
    /// # 参数
    /// * `order` - 已迁移到生效态并绑定版本指针的销售单（成功后内存版本递增）
    /// * `revision` - 待写入的正式版本头
    /// * `revision_lines` - 待写入的公共行版本
    /// * `goods_lines` - 待写入的实物及服务行版本
    /// * `voucher_lines` - 待写入的卡券行版本
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`crate::Error::DuplicateKey`]）、乐观锁冲突或
    /// MongoDB 写入失败时返回错误。
    pub async fn formalize_submission(
        &self,
        order: &mut entities::sales_order::SalesOrder,
        revision: &SalesOrderRevision,
        revision_lines: &[SalesOrderRevisionLine],
        goods_lines: &[SalesOrderGoodsServiceLineRevision],
        voucher_lines: &[SalesOrderVoucherLineRevision],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self.db.collection::<SalesOrderRevision>(SALES_ORDER_REVISIONS),
            revision,
            executor,
        )
        .await?;
        mongo_ops::insert_many(
            &self
                .db
                .collection::<SalesOrderRevisionLine>(SALES_ORDER_REVISION_LINES),
            revision_lines.to_vec(),
            executor,
        )
        .await?;
        mongo_ops::insert_many(
            &self
                .db
                .collection::<SalesOrderGoodsServiceLineRevision>(SALES_ORDER_GOODS_SERVICE_LINE_REVISIONS),
            goods_lines.to_vec(),
            executor,
        )
        .await?;
        mongo_ops::insert_many(
            &self
                .db
                .collection::<SalesOrderVoucherLineRevision>(SALES_ORDER_VOUCHER_LINE_REVISIONS),
            voucher_lines.to_vec(),
            executor,
        )
        .await?;
        Repository::new(self.db, SALES_ORDERS)
            .update(order, executor)
            .await
    }
}
