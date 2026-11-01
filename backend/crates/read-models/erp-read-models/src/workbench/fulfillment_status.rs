//! 履约行的付款及放行状态：只补齐授权分页结果，沿用队列读取事务。

use std::collections::HashMap;

use erp_core::money::Amount;
use erp_workflow::{WorkItemStatus, WorkItemType, WorkflowAuthorizationPort};
use persistence_core::Executor;
use serde::Serialize;

use super::{FulfillmentQueueGateState, WorkItemView, WorkbenchReadService};
use crate::errors::{Error, Result};
use crate::fulfillment_queue::{FulfillmentPaymentStatusRow, FulfillmentQueueRepository};

/// 采购整单实际付款进度，与付款申请审批状态分离。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FulfillmentPaymentState {
    Unpaid,
    Partial,
    Paid,
    NotRequired,
}

/// 列表行只返回状态，不展开付款申请、金额、回单或来源单据。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WorkItemFulfillmentStatus {
    /// 缺少应付子账或不适用采购付款时为空，不视为未付款。
    pub payment_state: Option<FulfillmentPaymentState>,
    pub gate_state: FulfillmentQueueGateState,
}

impl WorkItemFulfillmentStatus {
    /// 将同一读取快照的余额与付款门槛分别映射，未知门槛拒绝放行。
    fn from_row(row: &FulfillmentPaymentStatusRow) -> Result<Self> {
        let gate_state = FulfillmentQueueGateState::parse(&row.gate_state)
            .ok_or_else(|| Error::ValidationError("履约付款条件状态无效".into()))?;
        let payment_state = row.paid_amount.zip(row.open_amount).map(|(paid, open)| {
            match (paid > Amount::zero(), open > Amount::zero()) {
                (true, false) => FulfillmentPaymentState::Paid,
                (false, false) => FulfillmentPaymentState::NotRequired,
                (true, true) => FulfillmentPaymentState::Partial,
                (false, true) => FulfillmentPaymentState::Unpaid,
            }
        });
        Ok(Self { payment_state, gate_state })
    }
}

impl<A: WorkflowAuthorizationPort> WorkbenchReadService<A> {
    /// 为当前页已授权的开放履约任务补齐付款与发货条件。
    ///
    /// # 参数
    /// `items` 为完成对象授权的列表或详情；`executor` 为原读取事务执行器。
    /// # 返回
    /// 就地补齐匹配的履约行；无履约任务时不发起查询，缺失事实保持空值。
    /// # 错误
    /// 批量查询失败或未知放行状态时返回错误。
    pub(super) async fn apply_fulfillment_statuses(
        &self,
        items: &mut [WorkItemView],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = items
            .iter()
            .filter(|item| {
                item.work_item_type == WorkItemType::FulfillmentOperation
                    && item.status == WorkItemStatus::Open
                    && matches!(
                        item.business_object_type.as_str(),
                        "delivery" | "electronic_delivery" | "service_fulfillment"
                    )
            })
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>();
        let rows = FulfillmentQueueRepository::new(&self.db).payment_statuses(&ids, executor).await?;
        let mut statuses = rows
            .into_iter()
            .map(|row| Ok((row.work_item_id.clone(), WorkItemFulfillmentStatus::from_row(&row)?)))
            .collect::<Result<HashMap<_, _>>>()?;
        for item in items {
            item.fulfillment_status = statuses.remove(&item.id);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(paid: Option<&str>, open: Option<&str>, gate: &str) -> FulfillmentPaymentStatusRow {
        FulfillmentPaymentStatusRow {
            work_item_id: "task-1".into(),
            paid_amount: paid.map(|value| value.parse().unwrap()),
            open_amount: open.map(|value| value.parse().unwrap()),
            gate_state: gate.into(),
        }
    }

    #[test]
    fn payment_progress_does_not_imply_full_payment_is_required_for_shipping() {
        for (paid, open, gate, expected) in [
            ("0", "100", "BLOCKED", FulfillmentPaymentState::Unpaid),
            ("0", "100", "NOT_APPLICABLE", FulfillmentPaymentState::Unpaid),
            ("30", "70", "SATISFIED", FulfillmentPaymentState::Partial),
            ("29.99", "70.01", "BLOCKED", FulfillmentPaymentState::Partial),
            ("100", "0", "SATISFIED", FulfillmentPaymentState::Paid),
            ("0", "0", "NOT_APPLICABLE", FulfillmentPaymentState::NotRequired),
        ] {
            let status = WorkItemFulfillmentStatus::from_row(&row(Some(paid), Some(open), gate)).unwrap();
            assert_eq!(status.payment_state, Some(expected));
            assert_eq!(status.gate_state, FulfillmentQueueGateState::parse(gate).unwrap());
        }
    }

    #[test]
    fn missing_payment_facts_are_unknown_and_unknown_gate_is_rejected() {
        for (paid, open) in [(None, None), (Some("0"), None), (None, Some("100"))] {
            let status = WorkItemFulfillmentStatus::from_row(&row(paid, open, "BLOCKED")).unwrap();
            assert_eq!(status.payment_state, None);
            assert_eq!(status.gate_state, FulfillmentQueueGateState::Blocked);
        }
        assert!(WorkItemFulfillmentStatus::from_row(&row(Some("100"), Some("0"), "UNKNOWN")).is_err());
    }

    #[test]
    fn serializes_only_the_two_list_states() {
        let status = WorkItemFulfillmentStatus::from_row(&row(Some("30"), Some("70"), "SATISFIED")).unwrap();
        assert_eq!(
            serde_json::to_value(status).unwrap(),
            serde_json::json!({
                "payment_state": "PARTIAL", "gate_state": "SATISFIED"
            })
        );
    }
}
