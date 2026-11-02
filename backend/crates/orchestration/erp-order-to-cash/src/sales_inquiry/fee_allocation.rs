//! 已接受费用向关联生效销售分摊；不授予普通销售详情读取权。
use std::collections::HashSet;

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_core::money::Amount;
use erp_sales::entity::sales_inquiry::SalesInquiry;
use erp_sales::repository::SalesOrderExt;
use persistence_core::{Executor, NoTransaction};
use serde::{Deserialize, Serialize};

use super::SalesInquiryProcess;
use crate::{Error, Result};

/// 明确分摊的销售和金额；业务资格由服务端重新证明。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InquiryFeeAllocationInput {
    pub allocation_id: String,
    pub fee_id: String,
    pub sales_order_id: String,
    pub feedback_round: u32,
    pub amount: Amount,
}

/// 可重复核对的费用分摊命令。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AllocateInquiryFeesCommand {
    pub expected_version: u64,
    pub command_key: String,
    pub allocations: Vec<InquiryFeeAllocationInput>,
}

/// 询价来源内的最小销售摘要，不包含价格、金额、合同或附件。
#[derive(Debug, Clone, Serialize)]
pub struct InquiryFeeSalesView {
    pub id: String,
    pub order_no: String,
    pub status_label: String,
    pub eligible_for_fee_allocation: bool,
}

impl SalesInquiryProcess {
    /// 读取本询价生成的销售摘要，用于明确费用归属。
    ///
    /// # 参数
    /// `id` 为询价，`actor` 为当前具有询价详情资格的销售。
    /// # 返回
    /// 返回来源台账内销售的业务编号、状态和分摊资格。
    /// # 错误
    /// 询价越范围、账号失效或存储失败时拒绝。
    pub async fn fee_sales(&self, id: &str, actor: &AuditActor) -> Result<Vec<InquiryFeeSalesView>> {
        let mut executor = NoTransaction;
        let inquiry = self.access().require_sales(actor, "detail", id, &mut executor).await?;
        let ids =
            inquiry.quantity_ledger.iter().map(|row| row.sales_order_id.clone()).collect::<HashSet<_>>();
        if ids.len() > 200 {
            return Err(Error::BusinessLogicError("关联销售超过单次查询限制".into()));
        }
        let mut items = Vec::new();
        for id in ids {
            if let Some(order) = self.db.sales_orders().find_by_id(&id, &mut executor).await? {
                items.push(InquiryFeeSalesView {
                    id,
                    order_no: order.order_no.clone(),
                    status_label: order.commercial_status.label().to_string(),
                    eligible_for_fee_allocation: order.can_receive_inquiry_fees(),
                });
            }
        }
        items.sort_by(|a, b| (&a.order_no, &a.id).cmp(&(&b.order_no, &b.id)));
        Ok(items)
    }

    pub(super) async fn apply_fee_allocations(
        &self,
        inquiry: &mut SalesInquiry,
        request: &AllocateInquiryFeesCommand,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        inquiry.ensure_version(request.expected_version)?;
        if request.allocations.is_empty() || request.allocations.len() > 100 {
            return Err(Error::ValidationError("费用分摊须为 1 至 100 项".into()));
        }
        let mut ids = HashSet::new();
        for allocation in &request.allocations {
            if !ids.insert(&allocation.allocation_id) {
                return Err(Error::ValidationError("费用分摊身份重复".into()));
            }
            let order = self
                .db
                .sales_orders()
                .find_by_id(&allocation.sales_order_id, executor)
                .await?
                .ok_or_else(|| Error::NotFound("关联销售单不存在".into()))?;
            if !order.can_receive_inquiry_fees() || order.customer_id.as_ref() != inquiry.customer_id {
                return Err(Error::BusinessLogicError("仅可向本询价关联的生效销售分摊费用".into()));
            }
            inquiry.ensure_fee_sales_attribution(
                allocation.feedback_round,
                &allocation.fee_id,
                &allocation.sales_order_id,
                Instant::now(),
            )?;
            inquiry.allocate_fee(
                &allocation.allocation_id,
                &allocation.sales_order_id,
                allocation.feedback_round,
                &allocation.fee_id,
                allocation.amount,
                Instant::now(),
            )?;
        }
        Ok(())
    }
}
