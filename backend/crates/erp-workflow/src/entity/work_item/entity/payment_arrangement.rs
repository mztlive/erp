//! 付款安排的跟进日期与系统完成规则。
use erp_core::common::time::{BusinessDate, Instant};
use erp_core::{Error, Result};

use super::{WorkItem, WorkItemStatus, WorkItemType};
use crate::entity::work_item::payment_due_at;
impl WorkItem {
    /// 当前负责人登记下一次付款跟进日，保留开放责任。
    /// # 参数
    /// 当前处理人、跟进日期、业务当天及活动时间。
    /// # 返回
    /// 保存跟进时间；当天起在工作台到期筛选中出现。
    /// # 错误
    /// 非付款安排任务、非当前负责人或日期早于当天时拒绝。
    pub fn schedule_payment_follow_up(
        &mut self,
        actor: &str,
        date: BusinessDate,
        today: BusinessDate,
        now: Instant,
    ) -> Result<()> {
        self.ensure_payment_arrangement()?;
        if date < today {
            return Err(Error::from("跟进日期不能早于今天"));
        }
        self.record_activity(actor, now)?;
        self.due_at = Some(payment_due_at(date)?);
        Ok(())
    }
    /// 全部余额已申请、结清或采购终止时结束本轮安排。
    /// # 参数
    /// 系统确认时间。
    /// # 返回
    /// 记录完成状态及系统处理人。
    /// # 错误
    /// 非开放付款安排任务时拒绝。
    pub fn complete_payment_arrangement(&mut self, now: Instant) -> Result<()> {
        self.ensure_payment_arrangement()?;
        self.ensure_open()?;
        self.status = WorkItemStatus::Completed;
        self.completed_by = Some("__system__".into());
        self.completed_at = Some(now);
        self.last_activity_at = Some(now);
        Ok(())
    }
    /// 防止一般任务或审批任务使用付款安排命令。
    fn ensure_payment_arrangement(&self) -> Result<()> {
        self.ensure_generic_mutation()?;
        if self.work_item_type != WorkItemType::PurchasePaymentArrangement
            || self.business_object_type != "purchase_order"
        {
            return Err(Error::from("仅采购付款安排任务支持此操作"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use erp_core::ids::WorkItemId;

    use super::super::{WorkItemData, direct_data};
    use super::*;

    /// 使用正式构造器生成采购经办人任务。
    fn task() -> WorkItem {
        WorkItem::new_with_responsibility_key(
            WorkItemId::new("arrangement"),
            WorkItemData {
                work_item_type: WorkItemType::PurchasePaymentArrangement,
                business_object_type: "purchase_order".into(),
                business_object_id: "purchase".into(),
                due_at: None,
                ..direct_data()
            },
            "purchase_payment_arrangement:purchase",
        )
        .unwrap()
    }
    #[test]
    fn follow_up_preserves_open_responsibility_and_rejects_other_owner_or_past_date() {
        let today = BusinessDate::from_ymd(2026, 10, 19).unwrap();
        let past = BusinessDate::from_ymd(2026, 10, 18).unwrap();
        let now = Instant::from_unix_secs(100);
        let mut item = task();
        let before = item.clone();
        assert!(item.schedule_payment_follow_up("bob", today, today, now).is_err());
        assert!(item.schedule_payment_follow_up("alice", past, today, now).is_err());
        assert_eq!(item, before);
        item.schedule_payment_follow_up("alice", today, today, now).unwrap();
        assert_eq!(item.status, WorkItemStatus::Open);
        assert_eq!(item.due_at, Some(payment_due_at(today).unwrap()));
        assert_eq!(item.started_at, Some(now));
        item.complete_payment_arrangement(now).unwrap();
        assert_eq!(item.status, WorkItemStatus::Completed);
        assert_eq!(item.completed_by.as_deref(), Some("__system__"));
        assert!(item.schedule_payment_follow_up("alice", today, today, now).is_err());
        assert!(item.complete_payment_arrangement(now).is_err());
    }
    #[test]
    fn arrangement_requires_exact_purchase_identity_and_execution_permissions() {
        let item = task();
        let mut invalid = item.clone();
        invalid.work_item_type = WorkItemType::SupplierPaymentExecution;
        assert!(invalid.complete_payment_arrangement(Instant::now()).is_err());
        let data = WorkItemData { work_item_type: WorkItemType::PurchasePaymentArrangement, ..direct_data() };
        assert!(
            WorkItem::new_with_responsibility_key(
                WorkItemId::new("bad"),
                data,
                "purchase_payment_arrangement:purchase"
            )
            .is_err()
        );
        assert_eq!(
            item.work_item_type.required_execution_permissions("purchase_order"),
            Some(&[
                "purchase_order:detail",
                "purchase_payment_request:create",
                "purchase_payment_request:submit"
            ] as &[&str])
        );
        assert!(item.work_item_type.required_execution_permissions("sales_order").is_none());
        assert!(item.work_item_type.uses_explicit_owner_authorization());
    }
}
