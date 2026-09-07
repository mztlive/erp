//! 销售变更命令的版本与状态读取守卫，保留调用方安排的回执前读取时点。

use super::SalesReviewService;
use crate::entity::sales_review::SalesChangeOrder;
use crate::repository::SalesReviewExt;
use crate::{Error, Result};
use persistence_core::NoTransaction;

impl SalesReviewService {
    /// 读取提交命令的销售状态并检查版本和草稿；必须在流程回执读取之前调用。
    ///
    /// # 错误
    /// 缺单、过期版本及非草稿状态保留原错误类别与文案。
    pub async fn load_for_submission(&self, id: &str, expected_version: u64) -> Result<SalesChangeOrder> {
        let change_order = self
            .db
            .sales_change_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售变更单不存在".to_string()))?;
        if !change_order.matches_version(expected_version) {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        if !change_order.is_draft() {
            return Err(Error::ConflictError(
                "只有草稿状态的销售变更单可以提交审批".to_string(),
            ));
        }
        Ok(change_order)
    }
    /// 读取撤回命令的销售状态并检查客户端版本，后续状态动作在工作流计划完成后执行。
    ///
    /// # 错误
    /// 缺单或过期版本保持原错误；本方法不修改状态。
    pub async fn load_for_cancellation(&self, id: &str, expected_version: u64) -> Result<SalesChangeOrder> {
        let change_order = self
            .db
            .sales_change_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售变更单不存在".to_string()))?;
        if !change_order.matches_version(expected_version) {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        Ok(change_order)
    }
}
