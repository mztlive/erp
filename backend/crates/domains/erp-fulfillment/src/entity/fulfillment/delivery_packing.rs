//! 地址分组、包裹商品数量和部分发货的确定性合同。
use std::collections::{BTreeMap, HashSet};

use erp_core::money::Quantity;
use erp_core::{Error, Result};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use super::{DeliveryLine, DeliveryTrackingEntries, DeliveryTrackingEntry};

/// 一个地址下的真实发货行及其数量。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PackingItem {
    pub delivery_line_id: String,
    pub quantity: Quantity,
}

impl PackingItem {
    /// 安全汇总来源分配数量。
    /// # 参数
    /// `quantities` 为同一真实发货行的来源数量。
    /// # 返回
    /// 返回保持六位精度的数量合计。
    /// # 错误
    /// 数值溢出或超过数量精度时拒绝。
    pub fn total(quantities: impl IntoIterator<Item = Quantity>) -> Result<Quantity> {
        let total = quantities.into_iter().try_fold(Decimal::ZERO, |sum, value| {
            sum.checked_add(value.to_decimal()).ok_or("地址数量超出范围")
        })?;
        Quantity::try_from(total)
    }
}

/// 收件资料仅在授权详情中解密；持久化必须使用加密快照。
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryDestination {
    pub key: String,
    pub recipient: String,
    pub phone: String,
    pub address: String,
    pub items: Vec<PackingItem>,
}
impl std::fmt::Debug for DeliveryDestination {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeliveryDestination").field("items", &self.items).finish_non_exhaustive()
    }
}

/// 来源版本与收件分配；正式发货后冻结，不由当前销售地址重建历史。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryAddressSnapshot {
    pub source_version: String,
    pub destinations: Vec<DeliveryDestination>,
}

/// 一个包裹只属于一个收货地址，可包含多条商品明细。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DeliveryPackage {
    pub id: String,
    pub destination_key: String,
    pub carrier: String,
    pub tracking_no: String,
    pub items: Vec<PackingItem>,
}

/// 地址本次发货数量；允许零，未发部分留在后续任务。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DeliveryDispatch {
    pub destination_key: String,
    pub items: Vec<PackingItem>,
}

/// 完整草稿。来源版本必须与服务端当前来源一致。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DeliveryPacking {
    pub source_version: String,
    pub dispatch: Vec<DeliveryDispatch>,
    pub packages: Vec<DeliveryPackage>,
}

type Quantities = BTreeMap<(String, String), Decimal>;

impl DeliveryPacking {
    /// 校验地址、明细、数量、包裹归属和确认前的覆盖关系。
    /// # 参数
    /// `source` 为服务端收件分配，`lines` 为真实发货行，`posting` 表示正式确认。
    /// # 返回
    /// 草稿允许尚未登记物流；正式确认要求每项本次数量恰好分配到包裹。
    /// # 错误
    /// 来源过期、跨地址、重复包裹、数量超额或物流未完整时拒绝。
    pub fn validate(
        &self,
        source: &DeliveryAddressSnapshot,
        lines: &[DeliveryLine],
        posting: bool,
    ) -> Result<()> {
        if self.source_version != source.source_version {
            return Err(Error::from("来源交付安排已变化，请刷新后重新核对"));
        }
        if self.packages.len() > 100
            || self.dispatch.len() > 100
            || self.packages.iter().map(|row| row.items.len()).sum::<usize>() > 100
        {
            return Err(Error::from("一次发货最多100个地址、包裹或包裹商品关联"));
        }
        source.ensure_lines(lines)?;
        let allowed = source.quantities()?;
        let dispatched = dispatch_quantities(&self.dispatch, &allowed)?;
        if dispatched.len() != allowed.len() {
            return Err(Error::from("本次发货分配须列出所有地址商品，未发数量请填写零"));
        }
        if self
            .packages
            .iter()
            .any(|package| !source.destinations.iter().any(|row| row.key == package.destination_key))
        {
            return Err(Error::from("包裹收货地址不属于本次任务"));
        }
        let packed = package_quantities(&self.packages, &dispatched, posting)?;
        for (key, quantity) in &dispatched {
            if posting && packed.get(key).copied().unwrap_or_default() != *quantity {
                return Err(Error::from("包裹商品数量合计必须等于该地址本次发货数量"));
            }
        }
        let totals = self.line_totals()?;
        if posting && !totals.values().any(|value| *value > Decimal::ZERO) {
            return Err(Error::from("本次发货数量必须大于0"));
        }
        for (id, quantity) in totals {
            let line = lines.iter().find(|line| line.base.id == id).ok_or("包裹明细不属于本次发货")?;
            if quantity > line.quantity.to_decimal() {
                return Err(Error::from("本次发货数量超过任务剩余数量"));
            }
        }
        Ok(())
    }

    /// 汇总本次各真实发货行数量，不依赖商品名称或列表序号。
    /// # 参数
    /// 无。
    /// # 返回
    /// 发货行身份到数量的映射。
    /// # 错误
    /// 数量求和溢出时拒绝。
    pub fn line_totals(&self) -> Result<BTreeMap<String, Decimal>> {
        let mut totals = BTreeMap::new();
        for item in self.dispatch.iter().flat_map(|row| &row.items) {
            let total = totals.entry(item.delivery_line_id.clone()).or_insert(Decimal::ZERO);
            *total = total.checked_add(item.quantity.to_decimal()).ok_or("发货数量超出范围")?;
        }
        Ok(totals)
    }

    /// 为既有物流查询派生兼容的商品物流关系，包裹数量以 packing 为准。
    /// # 参数
    /// `lines` 为已核验的发货行。
    /// # 返回
    /// 已登记非空单号的去重物流关系。
    /// # 错误
    /// 来源明细缺失或物流字段非法时拒绝。
    pub fn tracking(&self, lines: &[DeliveryLine]) -> Result<Vec<DeliveryTrackingEntry>> {
        let mut entries = Vec::new();
        for package in &self.packages {
            if package.tracking_no.trim().is_empty() {
                continue;
            }
            for item in &package.items {
                let line = lines
                    .iter()
                    .find(|line| line.base.id == item.delivery_line_id)
                    .ok_or("包裹明细不存在")?;
                entries.push(DeliveryTrackingEntry {
                    sales_order_line_id: line.sales_order_line_id.clone(),
                    tracking_no: package.tracking_no.trim().into(),
                    carrier: Some(package.carrier.trim().into()),
                });
            }
        }
        Ok(DeliveryTrackingEntries::new(entries)?.into_entries())
    }
}

impl DeliveryAddressSnapshot {
    fn ensure_lines(&self, lines: &[DeliveryLine]) -> Result<()> {
        let mut totals = BTreeMap::new();
        let mut keys = HashSet::new();
        for destination in &self.destinations {
            if destination.key.is_empty() || !keys.insert(&destination.key) {
                return Err(Error::from("收货地址身份缺失或重复"));
            }
            for item in &destination.items {
                let total = totals.entry(item.delivery_line_id.as_str()).or_insert(Decimal::ZERO);
                *total = total.checked_add(item.quantity.to_decimal()).ok_or("地址数量超出范围")?;
            }
        }
        if totals.len() != lines.len()
            || lines.iter().any(|line| totals.get(line.base.id.as_str()) != Some(&line.quantity.to_decimal()))
        {
            return Err(Error::from("收货地址分配必须完整覆盖任务的真实商品数量"));
        }
        Ok(())
    }

    fn quantities(&self) -> Result<Quantities> {
        let mut values = BTreeMap::new();
        for destination in &self.destinations {
            for item in &destination.items {
                let key = (destination.key.clone(), item.delivery_line_id.clone());
                let total = values.entry(key).or_insert(Decimal::ZERO);
                *total = total.checked_add(item.quantity.to_decimal()).ok_or("地址数量超出范围")?;
            }
        }
        Ok(values)
    }
}

fn dispatch_quantities(dispatch: &[DeliveryDispatch], allowed: &Quantities) -> Result<Quantities> {
    let mut values = BTreeMap::new();
    let mut addresses = HashSet::new();
    for row in dispatch {
        if !addresses.insert(&row.destination_key) || row.items.len() > 200 {
            return Err(Error::from("收货地址重复或明细超过200项"));
        }
        for item in &row.items {
            let key = (row.destination_key.clone(), item.delivery_line_id.clone());
            let limit = allowed.get(&key).ok_or("发货明细不属于该收货地址")?;
            let quantity = item.quantity.to_decimal();
            if quantity < Decimal::ZERO || quantity > *limit || values.insert(key, quantity).is_some() {
                return Err(Error::from("地址发货数量超额、为负或商品重复"));
            }
        }
    }
    Ok(values)
}

fn package_quantities(
    packages: &[DeliveryPackage],
    dispatched: &Quantities,
    posting: bool,
) -> Result<Quantities> {
    let mut totals = BTreeMap::new();
    let mut ids = HashSet::new();
    let mut tracking = HashSet::new();
    for package in packages {
        if package.id.trim().is_empty() || package.id.len() > 128 || !ids.insert(&package.id) {
            return Err(Error::from("包裹标识为空、超长或重复"));
        }
        let carrier = package.carrier.trim();
        let number = package.tracking_no.trim();
        if carrier.chars().count() > 64 || number.chars().count() > 128 || package.items.len() > 200 {
            return Err(Error::from("包裹物流或商品明细超过限制"));
        }
        if posting && (carrier.is_empty() || number.is_empty() || package.items.is_empty()) {
            return Err(Error::from("请填写包裹物流公司、单号及商品数量"));
        }
        if !number.is_empty() && !tracking.insert((carrier, number)) {
            return Err(Error::from("同一物流包裹不能重复登记或发往不同地址"));
        }
        add_package_items(package, dispatched, &mut totals)?;
    }
    Ok(totals)
}

fn add_package_items(
    package: &DeliveryPackage,
    dispatched: &Quantities,
    totals: &mut Quantities,
) -> Result<()> {
    let mut seen = HashSet::new();
    for item in &package.items {
        let key = (package.destination_key.clone(), item.delivery_line_id.clone());
        let limit = dispatched.get(&key).ok_or("包裹商品不属于该收货地址本次发货")?;
        if !seen.insert(&item.delivery_line_id) || item.quantity.to_decimal() <= Decimal::ZERO {
            return Err(Error::from("包裹商品重复或数量不为正"));
        }
        let total = totals.entry(key).or_insert(Decimal::ZERO);
        *total = total.checked_add(item.quantity.to_decimal()).ok_or("包裹数量超出范围")?;
        if *total > *limit {
            return Err(Error::from("包裹数量超过该地址本次发货数量"));
        }
    }
    Ok(())
}

#[cfg(test)]
pub(super) mod tests {
    use erp_core::ids::{DeliveryId, DeliveryLineId, PurchaseLineSalesAllocationId, SalesOrderLineId};

    use super::*;
    use crate::entity::fulfillment::{DeliveryLineData, DeliveryType};

    fn item(id: &str, qty: &str) -> PackingItem {
        PackingItem { delivery_line_id: id.into(), quantity: qty.parse().unwrap() }
    }
    pub(in crate::entity::fulfillment) fn fixture()
    -> (DeliveryAddressSnapshot, Vec<DeliveryLine>, DeliveryPacking) {
        let source = DeliveryAddressSnapshot {
            source_version: "sales:purchase".into(),
            destinations: vec![
                DeliveryDestination {
                    key: "a".into(),
                    recipient: "甲".into(),
                    phone: "13800000001".into(),
                    address: "地址甲".into(),
                    items: vec![item("l1", "2"), item("l2", "1")],
                },
                DeliveryDestination {
                    key: "b".into(),
                    recipient: "乙".into(),
                    phone: "13800000002".into(),
                    address: "地址乙".into(),
                    items: vec![item("l1", "3"), item("l2", "2")],
                },
            ],
        };
        let lines = [("l1", "5"), ("l2", "3")]
            .into_iter()
            .map(|(id, quantity)| {
                DeliveryLine::new(
                    DeliveryLineId::new(id),
                    DeliveryLineData {
                        delivery_id: DeliveryId::new("delivery"),
                        line_no: if id == "l1" { 1 } else { 2 },
                        sales_order_line_id: SalesOrderLineId::new(id),
                        quantity: quantity.parse().unwrap(),
                        stock_reservation_id: None,
                        purchase_line_sales_allocation_id: Some(PurchaseLineSalesAllocationId::new(id)),
                    },
                    DeliveryType::SupplierDirect,
                )
                .unwrap()
            })
            .collect();
        let packing = DeliveryPacking {
            source_version: source.source_version.clone(),
            dispatch: source
                .destinations
                .iter()
                .map(|row| DeliveryDispatch { destination_key: row.key.clone(), items: row.items.clone() })
                .collect(),
            packages: source
                .destinations
                .iter()
                .map(|row| DeliveryPackage {
                    id: row.key.clone(),
                    destination_key: row.key.clone(),
                    carrier: "顺丰".into(),
                    tracking_no: format!("SF{}", row.key),
                    items: row.items.clone(),
                })
                .collect(),
        };
        (source, lines, packing)
    }
    #[test]
    fn multi_address_multi_product_and_split_packages_conserve_quantity() {
        let (source, lines, mut packing) = fixture();
        packing.validate(&source, &lines, true).unwrap();
        packing.packages[1].items[0].quantity = "1".parse().unwrap();
        packing.packages.push(DeliveryPackage {
            id: "third".into(),
            destination_key: "b".into(),
            carrier: "中通".into(),
            tracking_no: "ZT3".into(),
            items: vec![item("l1", "2")],
        });
        packing.validate(&source, &lines, true).unwrap();
        assert_eq!(packing.line_totals().unwrap()["l1"], Decimal::from(5));
        assert_eq!(packing.tracking(&lines).unwrap().len(), 5);
    }
    #[test]
    fn draft_can_be_incomplete_but_confirmation_requires_exact_coverage() {
        let (source, lines, mut packing) = fixture();
        packing.packages.clear();
        packing.validate(&source, &lines, false).unwrap();
        assert!(packing.validate(&source, &lines, true).is_err());
        packing.dispatch[1].items.iter_mut().for_each(|row| row.quantity = "0".parse().unwrap());
        packing.packages.push(DeliveryPackage {
            id: "first".into(),
            destination_key: "a".into(),
            carrier: "顺丰".into(),
            tracking_no: "SF1".into(),
            items: source.destinations[0].items.clone(),
        });
        packing.validate(&source, &lines, true).unwrap();
        assert_eq!(packing.line_totals().unwrap()["l1"], Decimal::from(2));
    }
    #[test]
    fn rejects_cross_address_duplicates_stale_source_and_overallocation() {
        let (source, lines, packing) = fixture();
        let mut invalid = packing.clone();
        invalid.packages[0].destination_key = "missing".into();
        assert!(invalid.validate(&source, &lines, false).is_err());
        let mut invalid = packing.clone();
        invalid.packages[1].tracking_no = invalid.packages[0].tracking_no.clone();
        assert!(invalid.validate(&source, &lines, true).is_err());
        let mut invalid = packing.clone();
        invalid.packages[1].id = invalid.packages[0].id.clone();
        assert!(invalid.validate(&source, &lines, false).is_err());
        let mut invalid = packing.clone();
        invalid.source_version = "outdated".into();
        assert!(invalid.validate(&source, &lines, false).is_err());
        for value in ["-1", "6"] {
            let mut invalid = packing.clone();
            invalid.dispatch[0].items[0].quantity = value.parse().unwrap();
            assert!(invalid.validate(&source, &lines, false).is_err());
        }
        let mut invalid = packing.clone();
        invalid.packages[0].items.push(item("l1", "1"));
        assert!(invalid.validate(&source, &lines, false).is_err());
    }
    #[test]
    fn snapshots_never_log_recipient_phone_or_address() {
        let (source, _, _) = fixture();
        let debug = format!("{source:?}");
        for secret in ["13800000001", "地址甲", "recipient"] {
            assert!(!debug.contains(secret));
        }
    }
}
