//! `payable_account` 应付往来子账（数据模型 §6.9）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::stable::StableBase;
use crate::errors::{Error, Result};
use crate::ids::{PayableAccountId, SupplierAccountId};
use crate::money::Amount;
use crate::validation::normalize_required_text;

/// 来源单据 ID 最大长度。
const DOCUMENT_ID_MAX_LEN: usize = 128;

/// 应付来源类型（数据模型 §6.9：`PURCHASE_ORDER` 或 `SUPPLIER_SETTLEMENT`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PayableSourceType {
    /// 采购单。
    PurchaseOrder,
    /// 第二期供应商结算单。
    SupplierSettlement,
}

impl PayableSourceType {
    /// 返回类型的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::PurchaseOrder => "采购单",
            Self::SupplierSettlement => "供应商结算单",
        }
    }

    /// 返回类型的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PurchaseOrder => "purchase_order",
            Self::SupplierSettlement => "supplier_settlement",
        }
    }
}

/// 应付子账状态（数据模型 §6.9：未结、部分结清、已结清）。
///
/// 由开放余额派生：`open_total == 0` 为已结清，`settled_total == 0` 为未结，
/// 其余为部分结清；过账事务通过 [`PayableAccount::sync_totals`] 维护。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PayableAccountStatus {
    /// 未结。
    Open,
    /// 部分结清。
    PartiallySettled,
    /// 已结清。
    Settled,
}

impl PayableAccountStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Open => "未结",
            Self::PartiallySettled => "部分结清",
            Self::Settled => "已结清",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::PartiallySettled => "partially_settled",
            Self::Settled => "settled",
        }
    }
}

/// 应付往来子账创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PayableAccountData {
    /// 来源单据（采购单或第二期供应商结算单）。
    pub source_document_id: String,
    /// 往来供应商。
    pub supplier_id: SupplierAccountId,
    /// 来源类型。
    pub source_type: PayableSourceType,
    /// 含税应付总额。
    pub gross_total: Amount,
    /// 已核销含税总额。
    pub settled_total: Amount,
    /// 可收票含税总额。
    pub invoiceable_total: Amount,
    /// 净已收票含税总额。
    pub invoiced_total: Amount,
}

/// 应付往来子账更新数据（当前仅占位，保持接口一致）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct PayableAccountUpdate {}

/// 应付往来子账实体（稳定主表，数据模型 §6.9）。
///
/// `(source_type, source_document_id)` 唯一；`open_total` / `open_invoiceable_total`
/// 为派生汇总，均不得为负；`status` 由开放余额派生。
/// `StableBase` 是 P0 冻结基元且未派生 `PartialEq`，因此本实体手工实现
/// `PartialEq`/`Eq`（全字段语义相等）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity)]
pub struct PayableAccount {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub stable: StableBase<PayableAccountStatus>,
    /// 来源单据 ID。
    pub source_document_id: String,
    /// 往来供应商。
    pub supplier_id: SupplierAccountId,
    /// 来源类型。
    pub source_type: PayableSourceType,
    /// 含税应付总额。
    pub gross_total: Amount,
    /// 已核销含税总额。
    pub settled_total: Amount,
    /// 剩余开放含税余额（`gross_total - settled_total`）。
    pub open_total: Amount,
    /// 可收票含税总额。
    pub invoiceable_total: Amount,
    /// 净已收票含税总额。
    pub invoiced_total: Amount,
    /// 剩余可收票含税额度（`invoiceable_total - invoiced_total`）。
    pub open_invoiceable_total: Amount,
}

impl PartialEq for PayableAccount {
    /// 全字段语义相等。
    fn eq(&self, other: &Self) -> bool {
        self.base == other.base
            && self.stable.status == other.stable.status
            && self.stable.current_revision_id == other.stable.current_revision_id
            && self.stable.created_by == other.stable.created_by
            && self.stable.updated_by == other.stable.updated_by
            && self.source_document_id == other.source_document_id
            && self.supplier_id == other.supplier_id
            && self.source_type == other.source_type
            && self.gross_total == other.gross_total
            && self.settled_total == other.settled_total
            && self.open_total == other.open_total
            && self.invoiceable_total == other.invoiceable_total
            && self.invoiced_total == other.invoiced_total
            && self.open_invoiceable_total == other.open_invoiceable_total
    }
}

impl Eq for PayableAccount {}

impl PayableAccount {
    /// 创建应付往来子账。
    ///
    /// 完成来源单据 ID 的 trim/非空/长度校验、金额非负与上限校验，并派生
    /// `open_total`、`open_invoiceable_total` 与 `status`。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::PayableAccountId`）
    /// * `data` - 创建数据
    /// * `created_by` - 创建人（账号或系统身份）
    ///
    /// # 返回
    /// 返回新建的子账实体。
    ///
    /// # 错误
    /// 当来源单据 ID 为空/超长或金额为负/已核销/已收票超过对应总额时返回错误。
    pub fn new(
        id: PayableAccountId,
        data: PayableAccountData,
        created_by: impl Into<String>,
    ) -> Result<Self> {
        let source_document_id = normalize_required_text(
            data.source_document_id,
            "来源单据ID不能为空",
            DOCUMENT_ID_MAX_LEN,
            "来源单据ID过长",
        )?;
        let (open_total, open_invoiceable) = validate_totals(
            data.gross_total,
            data.settled_total,
            data.invoiceable_total,
            data.invoiced_total,
        )?;
        let status = derive_status(open_total, data.settled_total);

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            stable: StableBase::new(status, created_by),
            source_document_id,
            supplier_id: data.supplier_id,
            source_type: data.source_type,
            gross_total: data.gross_total,
            settled_total: data.settled_total,
            open_total,
            invoiceable_total: data.invoiceable_total,
            invoiced_total: data.invoiced_total,
            open_invoiceable_total: open_invoiceable,
        })
    }

    /// 更新应付往来子账。
    ///
    /// 子账的业务字段（来源单据、供应商、来源类型）是固定字段不允许修改；
    /// 汇总由过账事务维护。当前更新无可用字段，保留接口以保持一致性。
    ///
    /// # 参数
    /// * `update` - 更新数据
    /// * `updated_by` - 本次更新执行人
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    pub fn update(&mut self, update: PayableAccountUpdate, updated_by: impl Into<String>) -> Result<()> {
        let _ = update;
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 过账后同步汇总并派生状态。
    ///
    /// 由付款/进项票分配过账事务在锁定子账后调用，重算 `open_total`、
    /// `open_invoiceable_total` 并派生 `status`（数据模型 §6.9「事务内同步汇总」）。
    ///
    /// # 参数
    /// * `settled_total` - 重算后的已核销含税总额
    /// * `invoiced_total` - 重算后的净已收票含税总额
    /// * `updated_by` - 本次更新执行人
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 当金额为负或已核销/已收票超过对应总额时返回错误。
    pub fn sync_totals(
        &mut self,
        settled_total: Amount,
        invoiced_total: Amount,
        updated_by: impl Into<String>,
    ) -> Result<()> {
        let (open_total, open_invoiceable) = validate_totals(
            self.gross_total,
            settled_total,
            self.invoiceable_total,
            invoiced_total,
        )?;
        self.settled_total = settled_total;
        self.invoiced_total = invoiced_total;
        self.open_total = open_total;
        self.open_invoiceable_total = open_invoiceable;
        self.stable.status = derive_status(open_total, settled_total);
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 判断子账是否已结清。
    ///
    /// # 返回
    /// 开放余额为零时返回 `true`。
    pub fn is_settled(&self) -> bool {
        self.stable.status() == PayableAccountStatus::Settled
    }
}

/// 校验子账汇总四金额并派生开放余额。
///
/// # 参数
/// * `gross_total` - 含税总额
/// * `settled_total` - 已核销总额
/// * `invoiceable_total` - 可收票总额
/// * `invoiced_total` - 净已收票总额
///
/// # 返回
/// 返回 `(open_total, open_invoiceable_total)`，均不得为负。
///
/// # 错误
/// 金额为负或已核销/已收票超过对应总额时返回错误。
fn validate_totals(
    gross_total: Amount,
    settled_total: Amount,
    invoiceable_total: Amount,
    invoiced_total: Amount,
) -> Result<(Amount, Amount)> {
    if gross_total.to_decimal().is_sign_negative()
        || settled_total.to_decimal().is_sign_negative()
        || invoiceable_total.to_decimal().is_sign_negative()
        || invoiced_total.to_decimal().is_sign_negative()
    {
        return Err(Error::from("子账汇总金额不得为负"));
    }
    if settled_total > gross_total {
        return Err(Error::from("已核销总额不得超过含税应付总额"));
    }
    if invoiced_total > invoiceable_total {
        return Err(Error::from("净已收票金额不得超过可收票总额"));
    }
    Ok((
        gross_total.checked_sub(settled_total),
        invoiceable_total.checked_sub(invoiced_total),
    ))
}

/// 由开放余额派生子账状态。
///
/// # 参数
/// * `open_total` - 开放余额
/// * `settled_total` - 已核销总额
///
/// # 返回
/// 开放余额为零时为已结清，已核销为零时为未结，其余为部分结清。
fn derive_status(open_total: Amount, settled_total: Amount) -> PayableAccountStatus {
    if open_total.to_decimal().is_zero() {
        PayableAccountStatus::Settled
    } else if settled_total.to_decimal().is_zero() {
        PayableAccountStatus::Open
    } else {
        PayableAccountStatus::PartiallySettled
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::money::Amount;
    use std::str::FromStr;

    fn data() -> PayableAccountData {
        PayableAccountData {
            source_document_id: " PO-2026-001 ".to_string(),
            supplier_id: SupplierAccountId::new("sup-1"),
            source_type: PayableSourceType::PurchaseOrder,
            gross_total: Amount::from_str("1000.00").unwrap(),
            settled_total: Amount::from_str("0.00").unwrap(),
            invoiceable_total: Amount::from_str("1000.00").unwrap(),
            invoiced_total: Amount::from_str("0.00").unwrap(),
        }
    }

    #[test]
    fn new_trims_and_derives_open_totals() {
        let account = PayableAccount::new(PayableAccountId::new("pa-1"), data(), "admin-1").unwrap();

        assert_eq!(account.source_document_id, "PO-2026-001");
        assert_eq!(account.open_total, Amount::from_str("1000.00").unwrap());
        assert_eq!(
            account.open_invoiceable_total,
            Amount::from_str("1000.00").unwrap()
        );
        assert_eq!(account.stable.status(), PayableAccountStatus::Open);
    }

    #[test]
    fn new_rejects_blank_source_and_invalid_totals() {
        let blank = PayableAccountData {
            source_document_id: "   ".to_string(),
            ..data()
        };
        assert!(PayableAccount::new(PayableAccountId::new("pa-2"), blank, "admin").is_err());

        let overlong = PayableAccountData {
            source_document_id: "x".repeat(129),
            ..data()
        };
        assert!(PayableAccount::new(PayableAccountId::new("pa-3"), overlong, "admin").is_err());

        let over_settled = PayableAccountData {
            settled_total: Amount::from_str("1001.00").unwrap(),
            ..data()
        };
        assert!(PayableAccount::new(PayableAccountId::new("pa-4"), over_settled, "admin").is_err());

        let negative = PayableAccountData {
            invoiced_total: Amount::from_str("-1.00").unwrap(),
            ..data()
        };
        assert!(PayableAccount::new(PayableAccountId::new("pa-5"), negative, "admin").is_err());
    }

    #[test]
    fn sync_totals_recomputes_balance_and_status() {
        let mut account = PayableAccount::new(PayableAccountId::new("pa-1"), data(), "admin-1").unwrap();

        account
            .sync_totals(
                Amount::from_str("400.00").unwrap(),
                Amount::from_str("1000.00").unwrap(),
                "system",
            )
            .unwrap();
        assert_eq!(account.stable.status(), PayableAccountStatus::PartiallySettled);
        assert_eq!(account.open_total, Amount::from_str("600.00").unwrap());
        assert_eq!(account.open_invoiceable_total, Amount::from_str("0.00").unwrap());

        account
            .sync_totals(
                Amount::from_str("1000.00").unwrap(),
                Amount::from_str("1000.00").unwrap(),
                "system",
            )
            .unwrap();
        assert!(account.is_settled());

        assert!(account
            .sync_totals(
                Amount::from_str("1001.00").unwrap(),
                Amount::from_str("0.00").unwrap(),
                "system",
            )
            .is_err());
    }

    #[test]
    fn update_touches_auditor_without_changing_keys() {
        let mut account = PayableAccount::new(PayableAccountId::new("pa-1"), data(), "admin-1").unwrap();
        account.update(PayableAccountUpdate {}, "admin-2").unwrap();
        assert_eq!(account.stable.updated_by, "admin-2");
        assert_eq!(account.source_document_id, "PO-2026-001");
        assert_eq!(account.supplier_id, SupplierAccountId::new("sup-1"));
    }

    #[test]
    fn enums_serialize_with_stable_codes_and_labels() {
        assert_eq!(
            serde_json::to_string(&PayableSourceType::SupplierSettlement).unwrap(),
            "\"supplier_settlement\""
        );
        assert_eq!(
            serde_json::to_string(&PayableAccountStatus::PartiallySettled).unwrap(),
            "\"partially_settled\""
        );
        assert_eq!(PayableSourceType::PurchaseOrder.label(), "采购单");
        assert_eq!(PayableAccountStatus::Settled.label(), "已结清");
        assert_eq!(
            PayableSourceType::SupplierSettlement.as_str(),
            "supplier_settlement"
        );
    }
}
