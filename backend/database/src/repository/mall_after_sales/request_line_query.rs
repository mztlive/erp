//! `mall_after_sales_request_line` 查询能力。
//!
//! 申请行归属 D30 `mall_after_sales` 域；本文件只承载该集合的单集合
//! 查询，跨集合范围快照仍由各专用仓储负责。

use entities::ids::MallAfterSalesRequestId;
use entities::mall_after_sales::MallAfterSalesRequestLine;
use mongodb::bson::doc;

use super::super::Repository;
use crate::executor::Executor;
use crate::Result;

impl<'a> Repository<'a, MallAfterSalesRequestLine> {
    /// 按商城售后申请读取全部申请行。
    ///
    /// # 参数
    /// * `request_id` - 商城售后申请主键
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回关联该申请的未删除申请行；无申请行时返回空集合。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    ///
    /// # 约束
    /// 只查询 `mall_after_sales_request_lines` 集合；不做分页截断，不返回 Service DTO。
    pub async fn list_by_request_id(
        &self,
        request_id: &MallAfterSalesRequestId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<MallAfterSalesRequestLine>> {
        self.find_many(
            doc! { "after_sales_request_id": request_id.to_string() },
            executor,
        )
        .await
    }
}
