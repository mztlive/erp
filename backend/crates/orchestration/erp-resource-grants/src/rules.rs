//! 纯规则；事实读取失败不得构造空目标结果。
use erp_identity::entity::resource_access::ResourceKind;
use erp_identity::{Error, Result};

pub(crate) struct InvoiceEvidence<'a> {
    pub approved: bool,
    pub request_approved: bool,
    pub approval_kind: &'a str,
    pub approval_request: &'a str,
    pub request_id: &'a str,
    pub approval_version: u32,
    pub request_version: u32,
    pub request_task: Option<&'a str>,
    pub task_id: &'a str,
    pub invoice_task: bool,
    pub task_kind: &'a str,
    pub task_account: &'a str,
    pub request_account: &'a str,
    pub account_id: &'a str,
    pub request_sales: &'a str,
    pub account_sales: &'a str,
    pub historical_recipient: bool,
}
impl InvoiceEvidence<'_> {
    // 核对开票审批、申请版本、应收待办、销售关联和历史接收人；任一关联错配即返回冲突，不能作为开票读取授权证据。
    pub(crate) fn validate(&self) -> Result<()> {
        if !self.approved
            || !self.request_approved
            || self.approval_kind != "sales_invoice_request"
            || self.approval_request != self.request_id
            || self.approval_version != self.request_version
            || self.request_task != Some(self.task_id)
            || !self.invoice_task
            || self.task_kind != "receivable_account"
            || self.task_account != self.account_id
            || self.request_account != self.account_id
            || self.request_sales != self.account_sales
            || !self.historical_recipient
        {
            return Err(Error::ConflictError(
                "开票申请的审批、待办或人员分派记录不一致，请核对开票申请".into(),
            ));
        }
        Ok(())
    }
}
pub(crate) struct AssignmentEvidence<'a> {
    pub subject_kind: &'a str,
    pub subject_id: &'a str,
    pub resource: ResourceKind,
    pub target_id: &'a str,
    pub instance_id: &'a str,
    pub execution_instance: &'a str,
    pub node: &'a str,
    pub execution_node: &'a str,
    pub blocked: bool,
    pub participant: bool,
}
impl AssignmentEvidence<'_> {
    // 核对销售或采购订单与审批实例、节点执行及参与人是否一致；不接受受阻节点或未实际分派的人，避免生成错误的订单读取授权。
    pub(crate) fn validate(&self) -> Result<()> {
        if self.subject_kind != self.resource.code()
            || self.subject_id != self.target_id
            || self.instance_id != self.execution_instance
            || self.node != self.execution_node
            || self.blocked
            || !self.participant
        {
            return Err(Error::ConflictError("审批记录与该步骤的人员分派不一致，请核对审批待办".into()));
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn invoice() -> InvoiceEvidence<'static> {
        InvoiceEvidence {
            approved: true,
            request_approved: true,
            approval_kind: "sales_invoice_request",
            approval_request: "r",
            request_id: "r",
            approval_version: 1,
            request_version: 1,
            request_task: Some("t"),
            task_id: "t",
            invoice_task: true,
            task_kind: "receivable_account",
            task_account: "a",
            request_account: "a",
            account_id: "a",
            request_sales: "s",
            account_sales: "s",
            historical_recipient: true,
        }
    }
    #[test]
    fn invoice_requires_final_approval_and_all_associations() {
        invoice().validate().unwrap();

        let changes: [fn(&mut InvoiceEvidence); 13] = [
            |f| f.approved = false,
            |f| f.request_approved = false,
            |f| f.approval_kind = "sales_order",
            |f| f.approval_request = "other",
            |f| f.approval_version = 2,
            |f| f.request_task = None,
            |f| f.request_task = Some("other"),
            |f| f.invoice_task = false,
            |f| f.task_kind = "other",
            |f| f.task_account = "other",
            |f| f.request_account = "other",
            |f| f.account_sales = "other",
            |f| f.historical_recipient = false,
        ];
        for change in changes {
            let mut fact = invoice();
            change(&mut fact);
            assert!(fact.validate().is_err());
        }
    }
    #[test]
    fn both_order_rules_require_formal_execution_and_actual_participant() {
        for resource in [ResourceKind::SalesOrder, ResourceKind::PurchaseOrder] {
            let make = || AssignmentEvidence {
                resource,
                subject_kind: resource.code(),
                subject_id: "o",
                target_id: "o",
                instance_id: "i",
                execution_instance: "i",
                node: "n",
                execution_node: "n",
                blocked: false,
                participant: true,
            };
            make().validate().unwrap();

            let changes: [fn(&mut AssignmentEvidence); 6] = [
                |f| f.subject_kind = "other",
                |f| f.subject_id = "other",
                |f| f.execution_instance = "other",
                |f| f.execution_node = "other",
                |f| f.blocked = true,
                |f| f.participant = false,
            ];
            for change in changes {
                let mut fact = make();
                change(&mut fact);
                assert!(fact.validate().is_err());
            }
        }
    }
}
