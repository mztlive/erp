//! 自动记录读取授权的稳定规则及来源元数据。
use serde::{Deserialize, Serialize};

use super::{ResourceKind, SourceKey};

/// 自动读取授权支持的正式业务触发类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrantRule {
    InvoiceTaskReassigned,
    InvoiceApproved,
    SalesOrderApprovalAssigned,
    PurchaseOrderApprovalAssigned,
}

impl GrantRule {
    /// 全部自动规则，供队列统计和按读取目标重算使用。
    pub const ALL: &'static [Self] = &[
        Self::InvoiceApproved,
        Self::InvoiceTaskReassigned,
        Self::SalesOrderApprovalAssigned,
        Self::PurchaseOrderApprovalAssigned,
    ];

    /// 返回持久化来源及事件幂等键使用的稳定规则代码。
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvoiceTaskReassigned => "invoice_task_reassigned",
            Self::InvoiceApproved => "invoice_approved",
            Self::SalesOrderApprovalAssigned => "sales_order_approval_assigned",
            Self::PurchaseOrderApprovalAssigned => "purchase_order_approval_assigned",
        }
    }

    /// 返回来源业务主键所属的记录类型。
    pub const fn business_type(self) -> &'static str {
        match self {
            Self::InvoiceTaskReassigned => "work_item",
            Self::InvoiceApproved => "sales_invoice_request",
            Self::SalesOrderApprovalAssigned | Self::PurchaseOrderApprovalAssigned => "approval_instance",
        }
    }

    /// 返回该规则生成读取授权的单据类型。
    pub const fn target_resource(self) -> ResourceKind {
        match self {
            Self::InvoiceTaskReassigned | Self::InvoiceApproved | Self::SalesOrderApprovalAssigned => {
                ResourceKind::SalesOrder
            },
            Self::PurchaseOrderApprovalAssigned => ResourceKind::PurchaseOrder,
        }
    }

    /// 返回可为该资源生成读取授权的规则代码，供队列重算筛选。
    pub fn codes_for_resource(resource: ResourceKind) -> Vec<&'static str> {
        Self::ALL.iter().copied().filter(|rule| rule.target_resource() == resource).map(Self::code).collect()
    }

    /// 由规则和正式业务标识构造来源键；调用方可通过来源键校验输入。
    pub fn source_key(self, business_id: impl Into<String>, recipient: impl Into<String>) -> SourceKey {
        SourceKey {
            rule: self.code().into(),
            business_type: self.business_type().into(),
            business_id: business_id.into(),
            recipient: recipient.into(),
        }
    }
}
