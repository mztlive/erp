//! 工作项完成、关闭、转派与终态。

use serde::{Deserialize, Serialize};

use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::validation::normalize_required_text;

use super::validation::{CLOSE_REASON_MAX_LEN, USER_ID_MAX_LEN};
use super::{AssignmentSource, WorkItem, WorkItemStatus, WorkItemType};

/// 受控关闭任务的数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkItemCloseData {
    /// 不可为空的关闭原因。
    pub close_reason: String,
}

impl WorkItem {
    /// 在业务需求已由系统事实完全满足时自动完成开放任务。
    ///
    /// # 参数
    /// * `at` - 系统确认需求归零的时间
    ///
    /// # 返回
    /// 自动完成成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 单据审批任务或非开放任务不能使用本入口。
    pub fn complete_when_requirement_satisfied(&mut self, at: Instant) -> Result<()> {
        self.ensure_generic_mutation()?;
        if self.work_item_type != WorkItemType::ProcurementOrderCreation {
            return Err(Error::from("只有供给分配任务可以按需求归零自动完成"));
        }
        self.ensure_open()?;
        self.status = WorkItemStatus::Completed;
        self.completed_at = Some(at);
        self.completed_by = Some("__system__".to_string());
        self.last_activity_at = Some(at);
        Ok(())
    }

    /// 在应付开放余额归零时自动完成付款执行任务。
    ///
    /// # 参数
    /// * `at` - 系统确认应付结清的时间
    ///
    /// # 返回
    /// 自动完成成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 非付款执行任务或任务已不是开放状态时返回错误。
    pub fn complete_when_payable_settled(&mut self, at: Instant) -> Result<()> {
        self.ensure_generic_mutation()?;
        if self.work_item_type != WorkItemType::SupplierPaymentExecution {
            return Err(Error::from("只有付款执行任务可以按应付结清自动完成"));
        }
        self.ensure_open()?;
        self.status = WorkItemStatus::Completed;
        self.completed_at = Some(at);
        self.completed_by = Some("__system__".to_string());
        self.last_activity_at = Some(at);
        Ok(())
    }

    /// 在应收剩余可开票额度归零时自动完成销项开票执行任务。
    ///
    /// # 参数
    /// * `at` - 系统确认可开票额度归零的时间
    ///
    /// # 返回
    /// 自动完成成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 非销项开票执行任务或任务已不是开放状态时返回错误。
    pub fn complete_when_fully_invoiced(&mut self, at: Instant) -> Result<()> {
        self.ensure_generic_mutation()?;
        if self.work_item_type != WorkItemType::SalesInvoiceExecution {
            return Err(Error::from("只有销项开票执行任务可以按可开票额度归零自动完成"));
        }
        self.ensure_open()?;
        self.status = WorkItemStatus::Completed;
        self.completed_at = Some(at);
        self.completed_by = Some("__system__".to_string());
        self.last_activity_at = Some(at);
        Ok(())
    }

    /// 记录当前责任人的首次处理或后续非终结活动。
    ///
    /// `started_at` 只在第一次调用时写入；后续调用仅推进 `last_activity_at`。
    ///
    /// # 错误
    /// 任务非开放、没有个人责任或操作人不是当前责任人时返回错误。
    pub fn record_activity(&mut self, actor_id: &str, at: Instant) -> Result<()> {
        self.ensure_current_owner(actor_id)?;
        self.started_at.get_or_insert(at);
        self.last_activity_at = Some(at);
        Ok(())
    }

    /// 转交开放任务的当前个人责任。
    ///
    /// 首次分派时间只在此前从未形成个人责任时写入；首次处理时间保持不变。
    /// 调用方必须在应用层重新校验目标任职、角色、数据范围与岗位分离。
    ///
    /// # 错误
    /// 任务非开放或目标用户为空、超长时返回错误。
    pub fn reassign(&mut self, target_user_id: impl Into<String>, at: Instant) -> Result<()> {
        self.ensure_generic_mutation()?;
        self.assign_to(target_user_id, AssignmentSource::AdminReassign, at)
    }

    /// 由强类型领域命令完成当前开放任务。
    ///
    /// 本方法只形成任务事实；调用方必须把正式领域事实、审批推进和本实体写入
    /// 放在同一事务。完成动作同时按 `if_null` 语义形成首次处理时间。
    ///
    /// # 错误
    /// 任务非开放、没有个人责任或执行人不是当前责任人时返回错误。
    pub fn complete_by_domain_command(&mut self, completed_by: impl Into<String>, at: Instant) -> Result<()> {
        self.ensure_generic_mutation()?;
        self.complete_open(completed_by, at)
    }

    /// 以受控原因关闭开放任务。
    ///
    /// 调用方必须先执行任务类型关闭策略与专门权限校验；关闭不会完成业务动作。
    ///
    /// # 错误
    /// 任务非开放、操作人或关闭原因为空、超长时返回错误。
    pub fn close(
        &mut self,
        closed_by: impl Into<String>,
        data: WorkItemCloseData,
        at: Instant,
    ) -> Result<()> {
        self.ensure_generic_mutation()?;
        self.close_open(closed_by, data, at)
    }

    /// 应用开放任务的受控关闭字段。
    ///
    /// # 参数
    /// * `closed_by` - 关闭操作人
    /// * `data` - 已选择的关闭原因
    /// * `at` - 关闭时间
    ///
    /// # 返回
    /// 成功时无返回值。
    ///
    /// # 错误
    /// 任务非开放、操作人或关闭原因非法时返回错误。
    pub(super) fn close_open(
        &mut self,
        closed_by: impl Into<String>,
        data: WorkItemCloseData,
        at: Instant,
    ) -> Result<()> {
        self.ensure_open()?;
        let closed_by = normalize_required_text(
            closed_by.into(),
            "关闭操作人不能为空",
            USER_ID_MAX_LEN,
            "关闭操作人过长",
        )?;
        let close_reason = normalize_required_text(
            data.close_reason,
            "关闭原因不能为空",
            CLOSE_REASON_MAX_LEN,
            "关闭原因过长",
        )?;
        self.status = WorkItemStatus::Closed;
        self.closed_at = Some(at);
        self.closed_by = Some(closed_by);
        self.close_reason = Some(close_reason);
        Ok(())
    }

    /// 返回任务是否已进入不可逆终态。
    ///
    /// # 返回
    /// `COMPLETED` 或 `CLOSED` 时返回 `true`。
    pub fn is_terminal(&self) -> bool {
        matches!(self.status, WorkItemStatus::Completed | WorkItemStatus::Closed)
    }

    /// 判断给定用户是否是开放任务的当前个人责任人。
    ///
    /// # 返回
    /// 任务开放且责任人与给定用户相同时返回 `true`。
    pub fn is_owned_by(&self, user_id: &str) -> bool {
        self.status == WorkItemStatus::Open && self.owner_user_id.as_deref() == Some(user_id)
    }

    /// 校验任务可以进入通用责任变更或关闭入口。
    ///
    /// # 返回
    /// 非单据审批任务返回 `Ok(())`。
    ///
    /// # 错误
    /// 单据审批任务或带审批节点执行引用时返回固定禁止错误。
    pub fn ensure_generic_responsibility_mutation(&self) -> Result<()> {
        if self.approval_node_execution_id.is_some() || self.work_item_type.is_document_approval() {
            return Err(Error::from("APPROVAL_GENERIC_WORK_ITEM_MUTATION_FORBIDDEN"));
        }
        Ok(())
    }

    pub(super) fn complete_open(&mut self, completed_by: impl Into<String>, at: Instant) -> Result<()> {
        let completed_by = normalize_required_text(
            completed_by.into(),
            "完成执行人不能为空",
            USER_ID_MAX_LEN,
            "完成执行人过长",
        )?;
        self.ensure_current_owner(&completed_by)?;
        self.started_at.get_or_insert(at);
        self.status = WorkItemStatus::Completed;
        self.completed_at = Some(at);
        self.completed_by = Some(completed_by);
        Ok(())
    }

    fn ensure_generic_mutation(&self) -> Result<()> {
        self.ensure_generic_responsibility_mutation()
    }

    pub(super) fn ensure_open(&self) -> Result<()> {
        if self.status == WorkItemStatus::Open {
            return Ok(());
        }
        Err(Error::from("只有开放任务可以执行责任动作"))
    }

    fn ensure_current_owner(&self, actor_id: &str) -> Result<()> {
        self.ensure_open()?;
        if self.owner_user_id.as_deref() == Some(actor_id) {
            return Ok(());
        }
        Err(Error::from("只有当前责任人可以处理任务"))
    }

    fn assign_to(
        &mut self,
        target_user_id: impl Into<String>,
        source: AssignmentSource,
        at: Instant,
    ) -> Result<()> {
        self.ensure_open()?;
        let target_user_id = normalize_required_text(
            target_user_id.into(),
            "目标责任人不能为空",
            USER_ID_MAX_LEN,
            "目标责任人过长",
        )?;
        self.assigned_at.get_or_insert(at);
        self.record_responsibility_actor(&target_user_id);
        self.owner_user_id = Some(target_user_id);
        self.current_assignment_at = Some(at);
        self.assignment_source = source;
        self.last_activity_at = Some(at);
        Ok(())
    }

    /// 追加首次出现的个人责任人，保留稳定的责任形成顺序。
    fn record_responsibility_actor(&mut self, actor_id: &str) {
        if !self.responsibility_actor_ids.iter().any(|id| id == actor_id) {
            self.responsibility_actor_ids.push(actor_id.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{direct_data, AssignmentSource, WorkItem, WorkItemData, WorkItemStatus, WorkItemType};
    use crate::common::time::Instant;
    use crate::ids::WorkItemId;

    #[test]
    fn reassign_and_complete_preserve_first_times() {
        let first = Instant::from_unix_secs(100);
        let mut item = WorkItem::new_at(WorkItemId::new("wi-1"), direct_data(), first).unwrap();
        item.record_activity("alice", Instant::from_unix_secs(110))
            .unwrap();
        item.reassign("bob", Instant::from_unix_secs(130)).unwrap();
        item.complete_by_domain_command("bob", Instant::from_unix_secs(150))
            .unwrap();
        assert_eq!(item.assigned_at, Some(first));
        assert_eq!(item.started_at, Some(Instant::from_unix_secs(110)));
        assert_eq!(item.owner_user_id.as_deref(), Some("bob"));
        assert_eq!(item.assignment_source, AssignmentSource::AdminReassign);
        assert!(item.is_terminal());
    }

    #[test]
    fn payment_task_requires_key_and_completes_after_settlement() {
        let data = WorkItemData {
            work_item_type: WorkItemType::SupplierPaymentExecution,
            business_object_type: "payable_account".to_string(),
            owner_role: "role-finance".to_string(),
            reason_code: Some("PAYABLE_PAYMENT_REQUIRED".to_string()),
            ..direct_data()
        };
        assert!(WorkItem::new_at(
            WorkItemId::new("wi-payment-missing-key"),
            data.clone(),
            Instant::from_unix_secs(100),
        )
        .is_err());
        let mut task = WorkItem::new_with_responsibility_key(
            WorkItemId::new("wi-payment"),
            data,
            "finance:SUPPLIER_PAYMENT:rule-1",
        )
        .unwrap();
        task.complete_when_payable_settled(Instant::from_unix_secs(120))
            .unwrap();

        assert_eq!(task.status, WorkItemStatus::Completed);
        assert_eq!(task.completed_by.as_deref(), Some("__system__"));
        assert_eq!(
            WorkItemType::SupplierPaymentExecution
                .supplier_payment_execution_permissions("payable_account")
                .unwrap()
                .last()
                .copied(),
            Some("supplier_payment:commit")
        );
        assert!(WorkItemType::SupplierPaymentExecution.uses_explicit_owner_authorization());
    }

    #[test]
    fn sales_invoice_task_requires_key_and_completes_when_fully_invoiced() {
        let data = WorkItemData {
            work_item_type: WorkItemType::SalesInvoiceExecution,
            business_object_type: "receivable_account".to_string(),
            owner_role: "role-finance".to_string(),
            reason_code: Some("RECEIVABLE_INVOICE_REQUIRED".to_string()),
            ..direct_data()
        };
        assert!(WorkItem::new_at(
            WorkItemId::new("wi-invoice-missing-key"),
            data.clone(),
            Instant::from_unix_secs(100),
        )
        .is_err());
        let mut task = WorkItem::new_with_responsibility_key(
            WorkItemId::new("wi-invoice"),
            data,
            "finance:SALES_INVOICE:rule-2",
        )
        .unwrap();

        task.complete_when_fully_invoiced(Instant::from_unix_secs(120))
            .unwrap();

        assert_eq!(task.status, WorkItemStatus::Completed);
        assert_eq!(task.completed_by.as_deref(), Some("__system__"));
        assert!(WorkItemType::SalesInvoiceExecution.uses_explicit_owner_authorization());
        assert_eq!(
            WorkItemType::SalesInvoiceExecution
                .sales_invoice_execution_permissions("receivable_account")
                .unwrap()
                .last()
                .copied(),
            Some("invoice:post")
        );
    }
}
