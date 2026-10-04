//! 财务领域独立命令回执；回放结果由财务事实解释，不读取审计正文。

use application_core::{CommandReceipt, StructuredCommandReceipt, StructuredReceiptMatch};
use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::ids::{CustomerReceiptId, InvoiceId, SalesInvoiceRequestId, SupplierPaymentId};
use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// 进项发票登记命令的稳定动作。
pub const PURCHASE_INVOICE_ACTION: &str = "purchase_invoice_allocation.post";
/// 进项发票登记命令的结果类型。
pub const PURCHASE_INVOICE_RESOURCE: &str = "purchase_invoice_allocation";

/// 财务命令强类型结果，按原入口保留恢复语义。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FinanceCommandResult {
    /// 回放重新查询发票当前视图；不将当前视图替换为首次结果快照。
    PurchaseInvoiceRegistered {
        /// 已登记的发票 ID。
        invoice_id: InvoiceId,
    },
    /// 其他资金命令保留原对象当前视图的恢复语义。
    ResourceCommitted {
        /// 按命令目录确定的强类型结果对象。
        resource: FinanceCommandResource,
    },
}

/// 财务命令结果的领域对象类型。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "id", rename_all = "snake_case")]
pub enum FinanceCommandResource {
    /// 供应商付款。
    SupplierPayment(SupplierPaymentId),
    /// 客户回款。
    CustomerReceipt(CustomerReceiptId),
    /// 销项发票。
    Invoice(InvoiceId),
    /// 销项开票申请。
    SalesInvoiceRequest(SalesInvoiceRequestId),
}

impl FinanceCommandResource {
    /// 返回结果资源的业务 ID。
    fn id(&self) -> &str {
        match self {
            Self::SupplierPayment(id) => id.as_ref(),
            Self::CustomerReceipt(id) => id.as_ref(),
            Self::Invoice(id) => id.as_ref(),
            Self::SalesInvoiceRequest(id) => id.as_ref(),
        }
    }
}

/// 成功财务命令的不可变回执，与正式事实及审计同事务写入。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Entity)]
pub struct FinanceCommandReceipt {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 共享命令身份、定位及指纹算法。
    pub command: StructuredCommandReceipt,
    /// 财务结果 schema 版本。
    pub result_schema_version: u16,
    /// 已证明的提交结果引用。
    pub result: FinanceCommandResult,
    /// 关联审计号；业务恢复只读取本回执和发票事实。
    pub audit_event_id: String,
}

impl FinanceCommandReceipt {
    /// 构造本次成功登记的独立财务回执。
    ///
    /// # 参数
    /// * `command` - 原命令身份。
    /// * `invoice_id` - 已登记的发票 ID。
    /// * `audit_event_id` - 同事务关联审计 ID。
    /// # 返回
    /// 返回已校验的独立回执。
    /// # 错误
    /// 命令类型不匹配或结果/身份缺失时返回错误。
    pub fn purchase_invoice(
        command: &CommandReceipt,
        invoice_id: InvoiceId,
        audit_event_id: String,
    ) -> Result<Self> {
        Self::assemble(StructuredCommandReceipt::from_command(command)?, invoice_id, audit_event_id)
    }

    /// 按显式财务动作目录构造当前对象结果回执。
    ///
    /// # 参数
    /// * `command` - 原命令身份及载荷。
    /// * `resource_id` - 正式结果对象 ID。
    /// * `audit_event_id` - 同事务关联审计 ID。
    /// # 返回
    /// 返回强类型结果回执。
    /// # 错误
    /// 动作/资源不在目录内、关联缺失或结果为空时返回错误。
    pub fn resource(command: &CommandReceipt, resource_id: String, audit_event_id: String) -> Result<Self> {
        let result = resource_for_command(command.action(), command.resource_type(), resource_id)?;
        let value = Self {
            base: BaseModel::new(command.id().to_string()),
            command: StructuredCommandReceipt::from_command(command)?,
            result_schema_version: 1,
            result: FinanceCommandResult::ResourceCommitted { resource: result },
            audit_event_id,
        };
        value.validate()?;
        Ok(value)
    }

    /// 为当前执行构造不可变实体。
    fn assemble(
        command: StructuredCommandReceipt,
        invoice_id: InvoiceId,
        audit_event_id: String,
    ) -> Result<Self> {
        let value = Self {
            base: BaseModel::new(command.command_id.clone()),
            command,
            result_schema_version: 1,
            result: FinanceCommandResult::PurchaseInvoiceRegistered { invoice_id },
            audit_event_id,
        };
        value.validate()?;
        Ok(value)
    }

    /// 校验持久化身份与结果；损坏回执必须明确失败。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 合法时返回空结果。
    /// # 错误
    /// schema、关联、动作、资源或结果无效时返回内部错误。
    pub fn validate(&self) -> Result<()> {
        self.command.validate().map_err(|_| corrupted_receipt())?;
        if self.result_schema_version != 1
            || self.base.id != self.command.command_id
            || self.base.is_deleted()
            || self.audit_event_id.trim().is_empty()
        {
            return Err(corrupted_receipt());
        }
        self.validate_result()?;
        Ok(())
    }

    /// 校验动作目录及强类型对象。
    fn validate_result(&self) -> Result<()> {
        match &self.result {
            FinanceCommandResult::PurchaseInvoiceRegistered { invoice_id }
                if self.command.action == PURCHASE_INVOICE_ACTION
                    && self.command.resource_type == PURCHASE_INVOICE_RESOURCE
                    && !invoice_id.as_ref().trim().is_empty() =>
            {
                Ok(())
            },
            FinanceCommandResult::ResourceCommitted { resource } => {
                let expected = resource_for_command(
                    &self.command.action,
                    &self.command.resource_type,
                    resource.id().to_string(),
                )?;
                if &expected != resource
                    || self.command.scope_id.as_deref().is_some_and(|id| id != resource.id())
                {
                    return Err(corrupted_receipt());
                }
                Ok(())
            },
            _ => Err(corrupted_receipt()),
        }
    }

    /// 比对请求并返回当前视图恢复所需的正式对象 ID。
    ///
    /// # 参数
    /// * `command` - 原请求身份及载荷。
    /// # 返回
    /// 返回命令原结果对象 ID。
    /// # 错误
    /// 异载荷返回稳定冲突，身份/结果损坏返回内部错误。
    pub fn resource_id(&self, command: &CommandReceipt) -> Result<String> {
        self.validate()?;
        match command.match_structured(&self.command) {
            StructuredReceiptMatch::SamePayload => match &self.result {
                FinanceCommandResult::PurchaseInvoiceRegistered { invoice_id } => Ok(invoice_id.to_string()),
                FinanceCommandResult::ResourceCommitted { resource } => Ok(resource.id().to_string()),
            },
            StructuredReceiptMatch::DifferentPayload => {
                Err(Error::ConflictError("同一操作号已用于不同提交，请重新发起操作".to_string()))
            },
            StructuredReceiptMatch::Corrupted => Err(corrupted_receipt()),
        }
    }

    /// 比对请求并读取已登记发票的引用，调用方再读取当前授权视图。
    ///
    /// # 参数
    /// * `command` - 原请求身份与载荷。
    /// # 返回
    /// 返回原已登记发票 ID。
    /// # 错误
    /// 异载荷返回稳定冲突，身份/结果损坏返回内部错误。
    pub fn purchase_invoice_id(&self, command: &CommandReceipt) -> Result<InvoiceId> {
        let resource_id = self.resource_id(command)?;
        if !matches!(&self.result, FinanceCommandResult::PurchaseInvoiceRegistered { .. }) {
            return Err(corrupted_receipt());
        }
        Ok(InvoiceId::new(resource_id))
    }
}

/// 读取当前命令唯一命中的财务回执，禁止任意挑选多个结果。
///
/// # 参数
/// * `command` - 原请求身份与载荷。
/// * `receipts` - 仓储批量读取结果。
/// # 返回
/// 未命中返回 None，命中返回已登记发票 ID。
/// # 错误
/// 重复身份、损坏或异载荷返回明确错误。
pub fn pick_purchase_invoice_id(
    command: &CommandReceipt,
    receipts: &[FinanceCommandReceipt],
) -> Result<Option<InvoiceId>> {
    let matching: Vec<_> = receipts.iter().filter(|receipt| receipt.base.id == command.id()).collect();
    if matching.len() > 1 {
        return Err(corrupted_receipt());
    }
    match matching.first() {
        Some(receipt) => receipt.purchase_invoice_id(command).map(Some),
        None => Ok(None),
    }
}

/// 保留现有通用回执损坏错误口径。
fn corrupted_receipt() -> Error {
    Error::Internal("业务命令收据格式无效".to_string())
}

/// 财务动作与结果对象目录。
fn resource_for_command(action: &str, resource_type: &str, id: String) -> Result<FinanceCommandResource> {
    if id.trim().is_empty() {
        return Err(corrupted_receipt());
    }
    let result = match (action, resource_type) {
        ("supplier_payment.commit", "supplier_payment") => {
            FinanceCommandResource::SupplierPayment(SupplierPaymentId::new(id))
        },
        ("customer_receipt.commit" | "customer_receipt.submit", "customer_receipt") => {
            FinanceCommandResource::CustomerReceipt(CustomerReceiptId::new(id))
        },
        ("invoice.commit", "invoice") => FinanceCommandResource::Invoice(InvoiceId::new(id)),
        ("sales_invoice_request.submit" | "sales_invoice_request.cancel", "sales_invoice_request") => {
            FinanceCommandResource::SalesInvoiceRequest(SalesInvoiceRequestId::new(id))
        },
        _ => return Err(corrupted_receipt()),
    };
    Ok(result)
}

#[cfg(test)]
mod tests {
    use std::slice::from_ref;

    use super::*;

    fn command(amount: u32) -> CommandReceipt {
        CommandReceipt::from_payload(
            "purchase-invoice-register-",
            "actor",
            PURCHASE_INVOICE_ACTION,
            PURCHASE_INVOICE_RESOURCE,
            "key",
            &amount,
        )
        .unwrap()
    }

    fn receipt() -> FinanceCommandReceipt {
        let command = command(10);
        FinanceCommandReceipt::purchase_invoice(
            &command,
            InvoiceId::new("invoice-1"),
            command.id().to_string(),
        )
        .unwrap()
    }

    #[test]
    fn current_result_keeps_invoice_reference_and_same_payload_replays() {
        let receipt = receipt();
        assert_eq!(pick_purchase_invoice_id(&command(10), &[]).unwrap(), None);
        assert_eq!(
            pick_purchase_invoice_id(&command(10), from_ref(&receipt)).unwrap(),
            Some(InvoiceId::new("invoice-1"))
        );
        let encoded = serde_json::to_string(&receipt).unwrap();
        assert_eq!(serde_json::from_str::<FinanceCommandReceipt>(&encoded).unwrap(), receipt);
        assert!(
            matches!(receipt.purchase_invoice_id(&command(20)), Err(Error::ConflictError(message)) if message == "同一操作号已用于不同提交，请重新发起操作")
        );
    }

    #[test]
    fn corrupted_results_and_duplicate_candidates_are_rejected() {
        let valid = receipt();
        assert!(matches!(
            pick_purchase_invoice_id(&command(10), &[valid.clone(), valid.clone()]),
            Err(Error::Internal(_))
        ));
        let mut changed = valid.clone();
        changed.result_schema_version = 2;
        assert!(changed.purchase_invoice_id(&command(10)).is_err());
        let mut changed = valid.clone();
        changed.result = FinanceCommandResult::PurchaseInvoiceRegistered { invoice_id: InvoiceId::new("") };
        assert!(changed.purchase_invoice_id(&command(10)).is_err());
        let mut changed = valid;
        changed.command.actor_id = "other".to_string();
        assert!(matches!(changed.purchase_invoice_id(&command(10)), Err(Error::Internal(_))));
    }

    #[test]
    fn financial_action_catalog_binds_result_types() {
        for (action, resource_type) in [
            ("supplier_payment.commit", "supplier_payment"),
            ("customer_receipt.commit", "customer_receipt"),
            ("customer_receipt.submit", "customer_receipt"),
            ("invoice.commit", "invoice"),
            ("sales_invoice_request.submit", "sales_invoice_request"),
            ("sales_invoice_request.cancel", "sales_invoice_request"),
        ] {
            let command =
                CommandReceipt::from_payload("finance-", "actor", action, resource_type, "key", &10).unwrap();
            let mut receipt =
                FinanceCommandReceipt::resource(&command, "target".to_string(), "audit".to_string()).unwrap();
            assert_eq!(receipt.resource_id(&command).unwrap(), "target");
            let changed =
                CommandReceipt::from_payload("finance-", "actor", action, resource_type, "key", &20).unwrap();
            assert!(matches!(receipt.resource_id(&changed), Err(Error::ConflictError(_))));
            receipt.result = FinanceCommandResult::ResourceCommitted {
                resource: FinanceCommandResource::Invoice(InvoiceId::new("wrong")),
            };
            if action != "invoice.commit" {
                assert!(matches!(receipt.resource_id(&command), Err(Error::Internal(_))));
            }
        }
        let unsupported = CommandReceipt::from_payload(
            "finance-",
            "actor",
            "invoice.commit",
            "customer_receipt",
            "key",
            &10,
        )
        .unwrap();
        assert!(
            FinanceCommandReceipt::resource(&unsupported, "target".to_string(), "audit".to_string()).is_err()
        );
    }

    #[test]
    fn positioned_financial_commands_reject_a_different_result_target() {
        let command = CommandReceipt::from_resource_parts(
            "finance-",
            "actor",
            "sales_invoice_request.cancel",
            "sales_invoice_request",
            "target",
            "key",
            ["v1".to_string()],
        )
        .unwrap();
        assert!(FinanceCommandReceipt::resource(&command, "other".to_string(), "audit".to_string()).is_err());
        let mut receipt =
            FinanceCommandReceipt::resource(&command, "target".to_string(), "audit".to_string()).unwrap();
        receipt.result = FinanceCommandResult::ResourceCommitted {
            resource: FinanceCommandResource::SalesInvoiceRequest(SalesInvoiceRequestId::new("other")),
        };
        assert!(matches!(receipt.resource_id(&command), Err(Error::Internal(_))));
    }
}
