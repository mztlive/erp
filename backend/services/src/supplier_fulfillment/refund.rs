use entities::supplier_fulfillment::SupplierOrderActionType;

use super::dto::{SubmitActionResultView, SubmitAfterSalesActionRequest};
use super::SupplierFulfillmentService;
use crate::errors::Result;
use application_core::AuditActor;

impl SupplierFulfillmentService {
    /// 提交供应商退款（幂等键：「订单号 + REFUND」，§6.19）。
    ///
    /// 同事务创建 `REFUND` 动作头/行并把 `refund_status` 推进到 `REFUND_PENDING`；
    /// 事务外派发供应商 API。重复提交（同一幂等键）返回原动作结果，不再次调用。
    ///
    /// # 参数
    /// * `id` - 供应商子订单 ID
    /// * `req` - 退款动作提交请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回动作与动作后订单视图。
    ///
    /// # 错误
    /// * `NotFound` - 订单不存在
    /// * `BusinessLogicError` - 动作范围非法或连接缺少退款能力
    /// * `ConflictError` - 唯一键冲突（并发重复提交）
    pub async fn submit_refund(
        &self,
        id: &str,
        req: SubmitAfterSalesActionRequest,
        actor: &AuditActor,
    ) -> Result<SubmitActionResultView> {
        self.submit_after_sales_action(id, req, SupplierOrderActionType::Refund, actor)
            .await
    }
}
