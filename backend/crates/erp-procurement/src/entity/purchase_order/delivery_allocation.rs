//! 采购行交付安排：仅接受销售冻结地址，并按本次承担数量保存。
use std::collections::BTreeMap;
use std::fmt;

use erp_core::money::Quantity;
use erp_core::{Error, Result};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use super::{FulfillmentResponsibility, PurchaseLineType, ensure_sourcing_quantity};

/// 本采购行承担的收件资料及基础单位数量；不代表已发货。
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PurchaseDeliveryAllocation {
    pub recipient: String,
    pub phone: String,
    pub address: String,
    pub quantity: Quantity,
}

impl fmt::Debug for PurchaseDeliveryAllocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PurchaseDeliveryAllocation").field("quantity", &self.quantity).finish_non_exhaustive()
    }
}

impl PurchaseDeliveryAllocation {
    /// 返回收件资料身份：收货人、电话与地址。
    pub(super) fn key(&self) -> (&str, &str, &str) {
        (&self.recipient, &self.phone, &self.address)
    }

    /// 校验冻结采购行的交付数量和字段归属。
    /// # 参数
    /// `rows` 为交付安排，`kind`、`quantity` 为采购行内容。
    /// # 返回
    /// 空安排或交付合计等于采购数量时成功。
    /// # 错误
    /// 资料非法、数量非正、重复地址或数量不守恒时拒绝。
    pub fn validate(rows: &[Self], kind: PurchaseLineType, quantity: Option<Quantity>) -> Result<()> {
        if rows.is_empty() {
            return Ok(());
        }
        if kind != PurchaseLineType::ItemService || rows.len() > 100 {
            return Err(Error::from("交付安排仅适用于商品/服务行，最多100个地址"));
        }
        let mut total = Decimal::ZERO;
        let mut seen = BTreeMap::new();
        for row in rows {
            if row.recipient.trim().is_empty()
                || row.recipient.chars().count() > 64
                || row.phone.trim().is_empty()
                || row.phone.chars().count() > 64
                || row.address.trim().is_empty()
                || row.address.chars().count() > 512
                || row.quantity.to_decimal() <= Decimal::ZERO
            {
                return Err(Error::from("交付安排须填写有效收货人、电话、地址和正数数量"));
            }
            if seen.insert(row.key(), ()).is_some() {
                return Err(Error::from("同一收货地址请合并填写数量"));
            }
            total = total.checked_add(row.quantity.to_decimal()).ok_or("交付数量合计超出范围")?;
        }
        if quantity.map(|value| value.to_decimal()) != Some(total) {
            return Err(Error::from("各地址交付数量合计必须等于本次采购数量"));
        }
        Ok(())
    }

    /// 以授权销售要求和已有采购安排核验本次分配。
    /// # 参数
    /// `rows` 为本次分配，`source` 为销售冻结要求，`assigned` 为已有采购承担数量；其余参数为采购数量、履约方式和单位精度。
    /// # 返回
    /// 来源、合计和地址剩余量全部符合时成功。
    /// # 错误
    /// 仓发泄露客户地址、伪造地址、分配缺失或超量时拒绝。
    pub fn validate_source(
        rows: &[Self],
        source: &[Self],
        assigned: &[Self],
        quantity: Quantity,
        responsibility: FulfillmentResponsibility,
        scale: Option<u8>,
        unit: &str,
    ) -> Result<()> {
        if responsibility == FulfillmentResponsibility::Warehouse {
            return if rows.is_empty() {
                Ok(())
            } else {
                Err(Error::from("仓库履约采购只登记入库目标仓，不得携带客户地址"))
            };
        }
        if source.is_empty() {
            return if rows.is_empty() {
                Ok(())
            } else {
                Err(Error::from("销售单未指定交付地址，请先补齐销售交付要求"))
            };
        }
        if rows.is_empty() {
            return Err(Error::from("请确认本次采购承担的地址及交付数量"));
        }
        Self::validate(rows, PurchaseLineType::ItemService, Some(quantity))?;
        let mut caps = BTreeMap::new();
        for row in source {
            *caps.entry(row.key()).or_insert(Decimal::ZERO) += row.quantity.to_decimal();
        }
        for row in assigned {
            if let Some(cap) = caps.get_mut(&row.key()) {
                *cap -= row.quantity.to_decimal();
            }
        }
        for row in rows {
            ensure_sourcing_quantity(row.quantity, scale, unit)
                .map_err(|error| Error::from(error.to_string()))?;
            let cap = caps.get_mut(&row.key()).ok_or("交付地址与销售要求不一致，请刷新后重新分配")?;
            *cap -= row.quantity.to_decimal();
            if *cap < Decimal::ZERO {
                return Err(Error::from("该地址交付数量超过剩余需求，请调整分配数量"));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;
    fn row(address: &str, quantity: &str) -> PurchaseDeliveryAllocation {
        PurchaseDeliveryAllocation {
            recipient: "张三".into(),
            phone: "13800000000".into(),
            address: address.into(),
            quantity: Quantity::from_str(quantity).unwrap(),
        }
    }
    fn check(
        rows: &[PurchaseDeliveryAllocation],
        assigned: &[PurchaseDeliveryAllocation],
        quantity: &str,
    ) -> Result<()> {
        PurchaseDeliveryAllocation::validate_source(
            rows,
            &[row("地址A", "6"), row("地址B", "4")],
            assigned,
            Quantity::from_str(quantity).unwrap(),
            FulfillmentResponsibility::SupplierDirect,
            Some(0),
            "件",
        )
    }
    #[test]
    fn partial_and_subsequent_batches_respect_address_capacity() {
        assert!(check(&[row("地址A", "4")], &[], "4").is_ok());
        assert!(check(&[row("地址A", "2"), row("地址B", "4")], &[row("地址A", "4")], "6").is_ok());
        assert!(check(&[row("地址A", "3")], &[row("地址A", "4")], "3").is_err());
    }
    #[test]
    fn forged_missing_duplicate_and_nonconserving_plans_are_rejected() {
        for rows in [
            vec![],
            vec![row("伪造地址", "2")],
            vec![row("地址A", "1")],
            vec![row("地址A", "1"), row("地址A", "1")],
            vec![row("地址A", "0.5"), row("地址B", "1.5")],
        ] {
            assert!(check(&rows, &[], "2").is_err());
        }
    }
    #[test]
    fn unspecified_and_warehouse_plans_remain_empty() {
        let q = Quantity::from_str("2").unwrap();
        assert!(
            PurchaseDeliveryAllocation::validate_source(
                &[],
                &[],
                &[],
                q,
                FulfillmentResponsibility::SupplierDirect,
                Some(0),
                "件"
            )
            .is_ok()
        );
        assert!(
            PurchaseDeliveryAllocation::validate_source(
                &[row("地址A", "2")],
                &[row("地址A", "2")],
                &[],
                q,
                FulfillmentResponsibility::Warehouse,
                Some(0),
                "件"
            )
            .is_err()
        );
        assert!(!format!("{:?}", row("秘密地址", "2")).contains("秘密地址"));
    }
}
