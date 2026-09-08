//! 工作项执行权限与可用账号身份。

use erp_core::AccountKind;
use erp_core::{Error, Result};

use super::WorkItemType;

/// Login/identity snapshot for work-item account checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowAccountFact {
    /// Stable account id.
    pub id: String,
    /// Frozen account kind.
    pub kind: AccountKind,
    /// Whether the account may log in.
    pub can_login: bool,
    /// Display name used by work-item projections.
    pub display_name: String,
    /// Login account used by candidate pickers.
    pub login_account: String,
}

impl WorkflowAccountFact {
    /// Construct an account fact.
    pub fn new(id: impl Into<String>, kind: AccountKind, can_login: bool) -> Self {
        Self {
            id: id.into(),
            kind,
            can_login,
            display_name: String::new(),
            login_account: String::new(),
        }
    }

    /// Attach a display name.
    pub fn with_display_name(mut self, display_name: impl Into<String>) -> Self {
        self.display_name = display_name.into();
        self
    }

    /// Attach a login account.
    pub fn with_login_account(mut self, login_account: impl Into<String>) -> Self {
        self.login_account = login_account.into();
        self
    }

    /// Whether the account may hold backoffice responsibilities.
    pub fn is_active_backoffice(&self) -> bool {
        self.kind == AccountKind::Admin && self.can_login
    }
}

/// Casbin subject key for an account.
pub fn casbin_subject(account_kind: AccountKind, account_id: &str) -> String {
    format!("user:{}:{account_id}", account_kind.as_str())
}

/// 已验证可参与工作项授权计算的账号身份。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvailableWorkItemAccount {
    account_id: String,
    kind: AccountKind,
}

impl AvailableWorkItemAccount {
    /// 从统一账号主数据形成可用工作项账号。
    ///
    /// # 参数
    /// * `account` - 当前统一账号事实
    ///
    /// # 返回
    /// 账号处于可登录状态时返回稳定身份与账号类型。
    ///
    /// # 错误
    /// 账号已停用或归档时返回错误。
    pub fn from_account(account: &WorkflowAccountFact) -> Result<Self> {
        if !account.can_login {
            return Err(Error::from("工作项账号不可登录"));
        }
        Ok(Self {
            account_id: account.id.clone(),
            kind: account.kind,
        })
    }

    /// 从统一账号主数据形成指定类型的可用工作项账号。
    ///
    /// # 参数
    /// * `account` - 当前统一账号事实
    /// * `expected_kind` - 授权快照冻结的账号类型
    ///
    /// # 返回
    /// 账号可登录且类型未变化时返回稳定身份。
    ///
    /// # 错误
    /// 账号不可登录或类型已变化时返回错误。
    pub fn from_account_kind(account: &WorkflowAccountFact, expected_kind: AccountKind) -> Result<Self> {
        let available = Self::from_account(account)?;
        if available.kind != expected_kind {
            return Err(Error::from("工作项账号类型已变化"));
        }
        Ok(available)
    }

    /// 返回稳定账号 ID。
    ///
    /// # 返回
    /// 返回统一账号主键。
    pub fn account_id(&self) -> &str {
        &self.account_id
    }

    /// 返回账号类型。
    ///
    /// # 返回
    /// 返回当前已验证账号类型。
    pub fn kind(&self) -> AccountKind {
        self.kind
    }
}

impl WorkItemType {
    /// 判断转派候选人校验是否必须携带完整工作面执行权限快照。
    ///
    /// # 返回
    /// W01、W06、W11、W12、W13 受控执行任务返回 `true`。
    pub fn requires_full_execution_permissions(self) -> bool {
        matches!(
            self,
            Self::FulfillmentOperation
                | Self::CustomerAcceptanceRegistration
                | Self::SupplierPaymentExecution
                | Self::SalesInvoiceExecution
                | Self::CardFundsReview
                | Self::CardFundsDeltaReview
        )
    }

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
            "delivery" => Some(&[
                "delivery:list",
                "delivery:detail",
                "delivery:update",
                "delivery:post",
            ]),
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
    /// `SUPPLIER_PAYMENT_EXECUTION + payable_account` 返回完整权限；其它组合返回 `None`。
    pub fn supplier_payment_execution_permissions(
        self,
        business_object_type: &str,
    ) -> Option<&'static [&'static str]> {
        if !self.is_supplier_payment_execution() || business_object_type != "payable_account" {
            return None;
        }
        Some(&[
            "payable_account:list",
            "payable_account:detail",
            "party_bank_account:reveal",
            "supplier_payment:list",
            "supplier_payment:detail",
            "supplier_payment:commit",
        ])
    }

    /// 返回 W11 销项开票执行所需的完整权限集合。
    ///
    /// # 参数
    /// * `business_object_type` - 开票执行任务固定对象类型
    ///
    /// # 返回
    /// `SALES_INVOICE_EXECUTION + receivable_account` 返回完整权限；其它组合返回 `None`。
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
            "invoice:post",
        ])
    }

    /// 返回 W13 卡券票款复核所需的完整权限集合。
    ///
    /// # 参数
    /// * `business_object_type` - 票款任务固定对象类型
    ///
    /// # 返回
    /// 两类卡券票款任务绑定应收子账时返回读取与正式复核权限；其它组合返回 `None`。
    pub fn card_funds_review_permissions(
        self,
        business_object_type: &str,
    ) -> Option<&'static [&'static str]> {
        if !matches!(self, Self::CardFundsReview | Self::CardFundsDeltaReview)
            || business_object_type != "receivable_account"
        {
            return None;
        }
        Some(&[
            "receivable_account:list",
            "receivable_account:detail",
            "receivable_funds_review:complete",
        ])
    }

    /// 返回任务类型与业务对象组合所需的完整执行权限集合。
    ///
    /// # 返回
    /// 需要完整执行权限的已注册组合返回非空集合；普通任务返回空集合；要求
    /// 完整权限但对象组合未注册时返回 `None`。BusinessException 的对象特例
    /// 由 Service 跨聚合政策追加，不在本通用映射中解释。
    pub fn required_execution_permissions(
        self,
        business_object_type: &str,
    ) -> Option<&'static [&'static str]> {
        if self.is_fulfillment_operation() {
            self.fulfillment_execution_permissions(business_object_type)
        } else if self.is_customer_acceptance_registration() {
            self.customer_acceptance_execution_permissions(business_object_type)
        } else if self.is_supplier_payment_execution() {
            self.supplier_payment_execution_permissions(business_object_type)
        } else if self.is_sales_invoice_execution() {
            self.sales_invoice_execution_permissions(business_object_type)
        } else if matches!(self, Self::CardFundsReview | Self::CardFundsDeltaReview) {
            self.card_funds_review_permissions(business_object_type)
        } else {
            Some(&[])
        }
    }

    /// 判断任务是否以系统解析出的具体个人责任作为参与依据。
    ///
    /// # 返回
    /// 冻结单人审批、供给分配、履约操作、付款与开票执行返回 `true`；这些任务不依赖团队池或创建人回退。
    pub fn uses_explicit_owner_authorization(self) -> bool {
        matches!(
            self,
            Self::DocumentApproval
                | Self::ProcurementOrderCreation
                | Self::FulfillmentOperation
                | Self::CustomerAcceptanceRegistration
                | Self::SupplierPaymentExecution
                | Self::SalesInvoiceExecution
        )
    }
}

#[cfg(test)]
mod tests {
    use super::super::{direct_data, WorkItem, WorkItemData, WorkItemType};
    use erp_core::common::time::Instant;
    use erp_core::ids::WorkItemId;

    #[test]
    fn fulfillment_task_cannot_bypass_frozen_responsibility_key() {
        let data = WorkItemData {
            work_item_type: WorkItemType::FulfillmentOperation,
            business_object_type: "delivery".to_string(),
            owner_role: "warehouse_outbound_handler".to_string(),
            reason_code: Some("WAREHOUSE_DELIVERY_READY".to_string()),
            ..direct_data()
        };
        assert!(WorkItem::new_at(
            WorkItemId::new("wi-fulfillment-missing-key"),
            data.clone(),
            Instant::from_unix_secs(100),
        )
        .is_err());
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
        assert!(WorkItemType::FulfillmentOperation
            .fulfillment_execution_permissions("unknown")
            .is_none());
        assert!(WorkItemType::DocumentApproval
            .fulfillment_execution_permissions("delivery")
            .is_none());
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
        assert!(WorkItem::new_at(
            WorkItemId::new("wi-acceptance-missing-key"),
            data.clone(),
            Instant::from_unix_secs(100),
        )
        .is_err());
        let item = WorkItem::new_with_responsibility_key(
            WorkItemId::new("wi-acceptance"),
            data,
            "sales_order:so-1:customer_acceptance",
        )
        .unwrap();
        assert_eq!(
            item.responsibility_key(),
            Some("sales_order:so-1:customer_acceptance")
        );
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
        assert!(WorkItemType::CustomerAcceptanceRegistration.uses_explicit_owner_authorization());
        assert!(WorkItemType::CustomerAcceptanceRegistration.requires_full_execution_permissions());
        assert!(WorkItemType::CustomerAcceptanceRegistration
            .customer_acceptance_execution_permissions("unknown")
            .is_none());
    }
}
