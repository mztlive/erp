//! 工作项执行权限与可用账号身份。

use erp_core::{Error, Result};
use erp_identity::Permission;

use super::{WorkItem, WorkItemType};
use crate::entity::work_item::required_execution_permissions;

impl WorkItemType {
    /// 返回具体履约对象在 W01 完整执行所需的权限集合。
    ///
    /// # 参数
    /// * `business_object_type` - 履约任务固定对象类型
    ///
    /// # 返回
    /// 已注册履约对象返回完整权限；非履约任务或未知对象返回 `None`。
    ///
    /// # 错误
    /// 无；调用方必须将 `None` 作为未注册合同并失败关闭。
    pub fn fulfillment_execution_permissions(
        self,
        business_object_type: &str,
    ) -> Option<&'static [&'static str]> {
        if !self.is_fulfillment_operation() {
            return None;
        }
        match business_object_type {
            "purchase_receipt" => Some(&[
                "purchase_receipt:list",
                "purchase_receipt:detail",
                "purchase_receipt:update",
                "purchase_receipt:post",
            ]),
            "delivery" => Some(&["delivery:list", "delivery:detail", "delivery:update", "delivery:post"]),
            "electronic_delivery" => Some(&["electronic_delivery:list", "electronic_delivery:confirm"]),
            "service_fulfillment" => Some(&["service_fulfillment:list", "service_fulfillment:confirm"]),
            _ => None,
        }
    }

    /// 返回 W06 客户验收登记所需的完整权限集合。
    ///
    /// # 参数
    /// * `business_object_type` - 验收任务固定对象类型
    ///
    /// # 返回
    /// `CUSTOMER_ACCEPTANCE_REGISTRATION + sales_order` 返回完整权限；其它组合返回 `None`。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn customer_acceptance_execution_permissions(
        self,
        business_object_type: &str,
    ) -> Option<&'static [&'static str]> {
        if !self.is_customer_acceptance_registration() || business_object_type != "sales_order" {
            return None;
        }
        Some(&[
            "customer_acceptance:list",
            "customer_acceptance:detail",
            "customer_acceptance:create",
            "customer_acceptance:post",
            "sales_order:detail",
        ])
    }

    /// 返回 W12 供应商付款执行所需的完整权限集合。
    ///
    /// # 参数
    /// * `business_object_type` - 付款执行任务固定对象类型
    ///
    /// # 返回
    /// `SUPPLIER_PAYMENT_EXECUTION + purchase_payment_request` 返回完整权限；其它组合返回 `None`。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn supplier_payment_execution_permissions(
        self,
        business_object_type: &str,
    ) -> Option<&'static [&'static str]> {
        if !self.is_supplier_payment_execution() || business_object_type != "purchase_payment_request" {
            return None;
        }
        Some(&["purchase_payment_request:detail", "purchase_payment_request:pay"])
    }

    /// 返回 W11 销项开票执行所需的完整权限集合。
    ///
    /// # 参数
    /// * `business_object_type` - 开票执行任务固定对象类型
    ///
    /// # 返回
    /// `SALES_INVOICE_EXECUTION + receivable_account` 返回完整权限；其它组合返回 `None`。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn sales_invoice_execution_permissions(
        self,
        business_object_type: &str,
    ) -> Option<&'static [&'static str]> {
        if !self.is_sales_invoice_execution() || business_object_type != "receivable_account" {
            return None;
        }
        Some(&[
            "receivable_account:list",
            "receivable_account:detail",
            "invoice:list",
            "invoice:detail",
            "invoice:create",
            "sales_invoice_request:list",
            "sales_invoice_request:detail",
            "invoice:post",
        ])
    }

    /// 返回任务类型与业务对象组合所需的完整执行权限集合。
    ///
    /// # 参数
    /// * `business_object_type` - 任务固定对象类型。
    ///
    /// # 返回
    /// 需要完整执行权限的已注册组合返回非空集合；普通任务返回空集合；要求
    /// 完整权限但对象组合未注册时返回 `None`。BusinessException 的对象特例
    /// 由 Service 跨聚合政策追加，不在本通用映射中解释。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn required_execution_permissions(
        self,
        business_object_type: &str,
    ) -> Option<&'static [&'static str]> {
        if self == Self::ApprovalRework {
            self.brief_relation(business_object_type).map(|_| &["work_item:detail"] as &[&str])
        } else if self.is_fulfillment_operation() {
            self.fulfillment_execution_permissions(business_object_type)
        } else if self.is_customer_acceptance_registration() {
            self.customer_acceptance_execution_permissions(business_object_type)
        } else if self.is_supplier_payment_execution() {
            self.supplier_payment_execution_permissions(business_object_type)
        } else if self.is_sales_invoice_execution() {
            self.sales_invoice_execution_permissions(business_object_type)
        } else if self == Self::SalesInquiryProcessing {
            (business_object_type == "sales_inquiry").then_some(&[
                "procurement_inquiry:detail",
                "procurement_inquiry:save_feedback",
                "procurement_inquiry:publish",
            ] as &[&str])
        } else if self == Self::ProcurementOrderCreation {
            (business_object_type == "sales_order").then_some(&["purchase_order:create"] as &[&str])
        } else if self == Self::PurchasePaymentArrangement {
            (business_object_type == "purchase_order").then_some(&[
                "purchase_order:detail",
                "purchase_payment_request:create",
                "purchase_payment_request:submit",
            ] as &[&str])
        } else if self == Self::PurchaseReturnRefund {
            (business_object_type == "purchase_order").then_some(&[
                "purchase_order:detail",
                "supplier_payment:list",
                "supplier_payment:detail",
                "supplier_refund:submit",
            ] as &[&str])
        } else if self == Self::PurchaseReturnInvoice {
            (business_object_type == "purchase_order").then_some(&[
                "purchase_order:detail",
                "supplier:detail",
                "purchase_invoice_allocation:list",
                "invoice:list",
                "invoice:detail",
                "invoice:reverse",
            ] as &[&str])
        } else if self == Self::SupplierPortalReview {
            (business_object_type == "supplier_portal_request")
                .then_some(&["supplier_portal_request:detail", "supplier_portal_request:review"] as &[&str])
        } else if matches!(self, Self::CardFundsReview | Self::CardFundsDeltaReview) {
            None
        } else {
            Some(&[])
        }
    }
}

impl WorkItem {
    /// 收集当前任务办理所需的读取与执行权限。
    /// # 参数
    /// 无。
    /// # 返回
    /// 当前已登记任务的完整具体权限，由 identity 证明同一有效角色覆盖。
    /// # 错误
    /// 任务关系、执行权限未登记或权限格式错误时拒绝。
    pub fn qualification_permissions(&self) -> Result<Vec<Permission>> {
        let relation = self
            .work_item_type
            .brief_relation(&self.business_object_type)
            .ok_or_else(|| Error::from("任务类型未注册责任策略"))?;
        let execution = required_execution_permissions(self.work_item_type, &self.business_object_type)
            .ok_or_else(|| Error::from("任务类型未注册完整执行权限"))?;
        execution.into_iter().chain([relation.read_permission]).map(Permission::parse).collect()
    }
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::Instant;
    use erp_core::ids::WorkItemId;

    use super::super::{WorkItem, WorkItemData, WorkItemType, direct_data};

    #[test]
    fn fulfillment_task_cannot_bypass_frozen_responsibility_key() {
        let data = WorkItemData {
            work_item_type: WorkItemType::FulfillmentOperation,
            business_object_type: "delivery".to_string(),
            owner_role: "warehouse_outbound_handler".to_string(),
            reason_code: Some("WAREHOUSE_DELIVERY_READY".to_string()),
            ..direct_data()
        };
        assert!(
            WorkItem::new_at(
                WorkItemId::new("wi-fulfillment-missing-key"),
                data.clone(),
                Instant::from_unix_secs(100),
            )
            .is_err()
        );
        let item = WorkItem::new_with_responsibility_key(
            WorkItemId::new("wi-fulfillment"),
            data,
            "warehouse:wh-1:warehouse_ship",
        )
        .unwrap();
        assert_eq!(item.responsibility_key(), Some("warehouse:wh-1:warehouse_ship"));
        assert_eq!(
            WorkItemType::FulfillmentOperation.fulfillment_execution_permissions("electronic_delivery"),
            Some(&["electronic_delivery:list", "electronic_delivery:confirm"] as &[&str])
        );
        assert!(WorkItemType::FulfillmentOperation.fulfillment_execution_permissions("unknown").is_none());
        assert!(WorkItemType::DocumentApproval.fulfillment_execution_permissions("delivery").is_none());
    }

    #[test]
    fn customer_acceptance_task_requires_key_and_full_execution_permissions() {
        let data = WorkItemData {
            work_item_type: WorkItemType::CustomerAcceptanceRegistration,
            business_object_type: "sales_order".to_string(),
            owner_role: "sales_order_owner".to_string(),
            reason_code: Some("CUSTOMER_ACCEPTANCE_REQUIRED".to_string()),
            ..direct_data()
        };
        assert!(
            WorkItem::new_at(
                WorkItemId::new("wi-acceptance-missing-key"),
                data.clone(),
                Instant::from_unix_secs(100),
            )
            .is_err()
        );
        let item = WorkItem::new_with_responsibility_key(
            WorkItemId::new("wi-acceptance"),
            data,
            "sales_order:so-1:customer_acceptance",
        )
        .unwrap();
        assert_eq!(item.responsibility_key(), Some("sales_order:so-1:customer_acceptance"));
        assert_eq!(
            WorkItemType::CustomerAcceptanceRegistration
                .customer_acceptance_execution_permissions("sales_order"),
            Some(&[
                "customer_acceptance:list",
                "customer_acceptance:detail",
                "customer_acceptance:create",
                "customer_acceptance:post",
                "sales_order:detail",
            ] as &[&str])
        );
        assert!(
            WorkItemType::CustomerAcceptanceRegistration
                .customer_acceptance_execution_permissions("unknown")
                .is_none()
        );
    }
}
