//! 独立收货安排：允许待补地址，已发数量不可挪用，全部计算使用定点数量。
use std::collections::{BTreeMap, HashSet};
use std::{fmt, mem};

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::money::Quantity;
use erp_core::region::RegionCode;
use erp_core::validation::normalize_required_text;
use erp_core::{Error, Result};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use super::fulfillment::{Delivery, DeliveryAddressSnapshot, DeliveryDestination, DeliveryLine, PackingItem};

/// 以销售单身份唯一保存；收件资料仅存密文，原销售修订保持不变。
#[derive(Clone, Serialize, Deserialize, Entity)]
pub struct AddressArrangement {
    #[serde(flatten)]
    pub base: BaseModel,
    pub source_revision: String,
    pub content_encrypted: String,
    pub updated_by: String,
    pub reason: String,
}
impl fmt::Debug for AddressArrangement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AddressArrangement").field("id", &self.base.id).finish_non_exhaustive()
    }
}

/// 一个商品分配给一个收件人的数量；地区名称由服务端生成。
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AddressAllocation {
    pub recipient: String,
    pub phone: String,
    pub region_code: String,
    #[serde(default)]
    pub region_name: String,
    pub address: String,
    pub quantity: Quantity,
}
impl fmt::Debug for AddressAllocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AddressAllocation").field("quantity", &self.quantity).finish_non_exhaustive()
    }
}
impl AddressAllocation {
    /// 规范化收件信息并生成地区名称。
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回完整地址。
    /// # 错误
    /// 必填信息、地区或数量非法时拒绝。
    pub fn normalize(&mut self) -> Result<()> {
        self.recipient =
            normalize_required_text(mem::take(&mut self.recipient), "请填写收货人", 64, "收货人过长")?;
        self.phone =
            normalize_required_text(mem::take(&mut self.phone), "请填写联系电话", 32, "联系电话过长")?;
        self.address =
            normalize_required_text(mem::take(&mut self.address), "请填写详细地址", 512, "详细地址过长")?;
        self.region_name = RegionCode::parse_leaf(&self.region_code)?.full_name();
        if self.quantity.to_decimal() <= Decimal::ZERO {
            return Err(Error::from("地址分配数量必须大于0"));
        }
        Ok(())
    }
    /// 返回包含地区的收件地址。
    /// # 参数
    /// 无。
    /// # 返回
    /// 完整地址。
    /// # 错误
    /// 不返回错误。
    pub fn full_address(&self) -> String {
        format!("{}{}", self.region_name, self.address)
    }
}

/// 销售入口使用稳定销售行身份，任务入口使用当前真实发货行身份。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ArrangementLine {
    pub line_id: String,
    pub allocations: Vec<AddressAllocation>,
}
/// 服务端核定的可分配商品数量，不接受客户端修改。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArrangementCapacity {
    pub line_id: String,
    pub item_name: String,
    pub quantity: Quantity,
    pub unit: String,
}

/// 校验所有商品的地址分配；未分配数量保留为待补地址。
/// # 参数
/// `lines` 是完整输入，`capacities` 是真实来源商品与数量。
/// # 返回
/// 规范化地址后成功。
/// # 错误
/// 非法身份、重复、超量或不完整商品集合时拒绝。
pub fn validate_arrangement(lines: &mut [ArrangementLine], capacities: &[ArrangementCapacity]) -> Result<()> {
    if lines.len() != capacities.len() || lines.len() > 200 {
        return Err(Error::from("商品范围已变化，请刷新后补充地址"));
    }
    let mut seen = HashSet::new();
    if lines.iter().map(|line| line.allocations.len()).sum::<usize>() > 100 {
        return Err(Error::from("一次最多登记100条地址分配"));
    }
    for line in lines {
        let cap =
            capacities.iter().find(|cap| cap.line_id == line.line_id).ok_or("商品不属于当前收货安排")?;
        if !seen.insert(&line.line_id) {
            return Err(Error::from("商品重复"));
        }
        for row in &mut line.allocations {
            row.normalize()?;
        }
        let mut merged = BTreeMap::<(String, String, String, String), AddressAllocation>::new();
        for row in mem::take(&mut line.allocations) {
            let key =
                (row.recipient.clone(), row.phone.clone(), row.region_code.clone(), row.address.clone());
            if let Some(existing) = merged.get_mut(&key) {
                existing.quantity = PackingItem::total([existing.quantity, row.quantity])?;
            } else {
                merged.insert(key, row);
            }
        }
        line.allocations = merged.into_values().collect();
        if PackingItem::total(line.allocations.iter().map(|row| row.quantity))? > cap.quantity {
            return Err(Error::from("收货地址分配数量超过商品数量"));
        }
    }
    Ok(())
}

/// 待分配地址余量；扣除已发和其他冻结任务后才能分配给新任务。
#[derive(Debug, Clone)]
pub struct AddressPool(pub Vec<ArrangementLine>);
impl AddressPool {
    /// 从地址池扣除已经冻结的数量，保护已发包裹及其他任务。
    /// # 参数
    /// `snapshot` 为冻结快照，`lines` 为该单真实行。
    /// # 返回
    /// 扣减成功。
    /// # 错误
    /// 已发地址缺失、被删或数量被减少时拒绝。
    pub fn consume(&mut self, snapshot: &DeliveryAddressSnapshot, lines: &[DeliveryLine]) -> Result<()> {
        for destination in &snapshot.destinations {
            if destination.pending_address {
                continue;
            }
            for item in &destination.items {
                let line = lines
                    .iter()
                    .find(|line| line.base.id == item.delivery_line_id)
                    .ok_or("已发商品无法核对")?;
                self.subtract(line.sales_order_line_id.as_ref(), destination, item.quantity)?;
            }
        }
        Ok(())
    }
    fn subtract(&mut self, id: &str, destination: &DeliveryDestination, quantity: Quantity) -> Result<()> {
        let mut left = quantity.to_decimal();
        if let Some(line) = self.0.iter_mut().find(|line| line.line_id == id) {
            for row in &mut line.allocations {
                if row.recipient != destination.recipient
                    || row.phone != destination.phone
                    || row.full_address() != destination.address
                {
                    continue;
                }
                let taken = row.quantity.to_decimal().min(left);
                row.quantity = Quantity::try_from(row.quantity.to_decimal() - taken)?;
                left -= taken;
            }
        }
        if left > Decimal::ZERO {
            return Err(Error::from("已发货或其他任务已分配的地址数量不能减少，请只调整未发部分"));
        }
        Ok(())
    }
    /// 按当前任务真实数量分配地址；地址未确定的余量明确归入待补分组。
    /// # 参数
    /// `lines` 是任务真实行，`version` 是来源版本，`key` 是地址密钥。
    /// # 返回
    /// 返回覆盖全部任务商品的可信快照。
    /// # 错误
    /// 数量精度或溢出时拒绝。
    pub fn assign(
        &mut self,
        lines: &[DeliveryLine],
        version: String,
        key: &[u8],
    ) -> Result<DeliveryAddressSnapshot> {
        let mut destinations = BTreeMap::<String, DeliveryDestination>::new();
        for line in lines {
            let mut left = line.quantity.to_decimal();
            if let Some(plan) = self.0.iter_mut().find(|row| row.line_id == line.sales_order_line_id.as_ref())
            {
                for row in &mut plan.allocations {
                    let taken = left.min(row.quantity.to_decimal());
                    if taken <= Decimal::ZERO {
                        continue;
                    }
                    add_destination(&mut destinations, row, line, Quantity::try_from(taken)?, key)?;
                    row.quantity = Quantity::try_from(row.quantity.to_decimal() - taken)?;
                    left -= taken;
                }
            }
            if left > Decimal::ZERO {
                destinations
                    .entry("pending-address".into())
                    .or_insert_with(|| DeliveryDestination {
                        key: "pending-address".into(),
                        recipient: String::new(),
                        phone: String::new(),
                        address: String::new(),
                        pending_address: true,
                        items: vec![],
                    })
                    .items
                    .push(PackingItem {
                        delivery_line_id: line.base.id.clone(),
                        quantity: Quantity::try_from(left)?,
                    });
            }
        }
        Ok(DeliveryAddressSnapshot {
            source_version: version,
            destinations: destinations.into_values().collect(),
        })
    }
}
fn add_destination(
    destinations: &mut BTreeMap<String, DeliveryDestination>,
    row: &AddressAllocation,
    line: &DeliveryLine,
    quantity: Quantity,
    key: &[u8],
) -> Result<()> {
    let address = row.full_address();
    let identity = serde_json::to_string(&(&row.recipient, &row.phone, &address))
        .map_err(|_| Error::from("收件资料无法规范化"))?;
    let id = Delivery::address_snapshot_fingerprint(&identity, key);
    let destination = destinations.entry(id.clone()).or_insert_with(|| DeliveryDestination {
        key: id,
        recipient: row.recipient.clone(),
        phone: row.phone.clone(),
        address,
        pending_address: false,
        items: vec![],
    });
    if let Some(item) = destination.items.iter_mut().find(|item| item.delivery_line_id == line.base.id) {
        item.quantity = PackingItem::total([item.quantity, quantity])?;
    } else {
        destination.items.push(PackingItem { delivery_line_id: line.base.id.clone(), quantity });
    }
    Ok(())
}

impl AddressPool {
    /// 先保留仍有效的未发地址，再分配待补或已调整部分，避免重排其他供应商任务。
    /// # 参数
    /// `tasks` 为所有未发任务的旧快照及真实行；`key` 为地址密钥。
    /// # 返回
    /// 返回与任务顺序一致的新快照。
    /// # 错误
    /// 旧数量超过真实任务或新分配非法时拒绝。
    pub fn reconcile(
        &mut self,
        tasks: &[(DeliveryAddressSnapshot, Vec<DeliveryLine>)],
        key: &[u8],
    ) -> Result<Vec<DeliveryAddressSnapshot>> {
        let mut result = Vec::new();
        for (snapshot, lines) in tasks {
            let mut retained = DeliveryAddressSnapshot {
                source_version: snapshot.source_version.clone(),
                destinations: vec![],
            };
            for destination in snapshot.destinations.iter().filter(|row| !row.pending_address) {
                for item in &destination.items {
                    let candidate = DeliveryAddressSnapshot {
                        source_version: snapshot.source_version.clone(),
                        destinations: vec![DeliveryDestination {
                            items: vec![item.clone()],
                            ..destination.clone()
                        }],
                    };
                    let mut available = self.clone();
                    if available.consume(&candidate, lines).is_ok() {
                        *self = available;
                        merge_destinations(&mut retained, candidate)?;
                    }
                }
            }
            result.push(retained);
        }
        for (snapshot, (_, lines)) in result.iter_mut().zip(tasks) {
            let remaining = unassigned_lines(snapshot, lines)?;
            let added = self.assign(&remaining, snapshot.source_version.clone(), key)?;
            merge_destinations(snapshot, added)?;
            snapshot.destinations.sort_by(|left, right| left.key.cmp(&right.key));
            for destination in &mut snapshot.destinations {
                destination.items.sort_by(|left, right| left.delivery_line_id.cmp(&right.delivery_line_id));
            }
        }
        Ok(result)
    }
}
fn unassigned_lines(snapshot: &DeliveryAddressSnapshot, lines: &[DeliveryLine]) -> Result<Vec<DeliveryLine>> {
    let mut remaining = vec![];
    for line in lines {
        let reserved = PackingItem::total(
            snapshot
                .destinations
                .iter()
                .flat_map(|row| &row.items)
                .filter(|item| item.delivery_line_id == line.base.id)
                .map(|item| item.quantity),
        )?;
        let left = line.quantity.to_decimal() - reserved.to_decimal();
        if left < Decimal::ZERO {
            return Err(Error::from("原收货安排超过当前商品数量，请核对来源"));
        }
        if left > Decimal::ZERO {
            remaining.push(DeliveryLine { quantity: Quantity::try_from(left)?, ..line.clone() });
        }
    }
    Ok(remaining)
}

fn merge_destinations(target: &mut DeliveryAddressSnapshot, added: DeliveryAddressSnapshot) -> Result<()> {
    for destination in added.destinations {
        if let Some(existing) = target.destinations.iter_mut().find(|row| row.key == destination.key) {
            for item in destination.items {
                if let Some(current) =
                    existing.items.iter_mut().find(|row| row.delivery_line_id == item.delivery_line_id)
                {
                    current.quantity = PackingItem::total([current.quantity, item.quantity])?;
                } else {
                    existing.items.push(item);
                }
            }
        } else {
            target.destinations.push(destination);
        }
    }
    Ok(())
}

/// 判断是否减少或替换既有地址；仅为未分配数量新增地址不要求修改原因。
/// # 参数
/// `previous` 和 `next` 使用同一商品身份范围。
/// # 返回
/// 已有地址数量被减少或移除时返回 true。
/// # 错误
/// 地址数量汇总溢出时拒绝。
pub fn requires_change_reason(previous: &[ArrangementLine], next: &[ArrangementLine]) -> Result<bool> {
    fn amounts(rows: &[ArrangementLine]) -> Result<BTreeMap<(String, String, String, String), Quantity>> {
        let mut result = BTreeMap::new();
        for line in rows {
            for row in &line.allocations {
                let key =
                    (line.line_id.clone(), row.recipient.clone(), row.phone.clone(), row.full_address());
                let quantity = result
                    .get(&key)
                    .copied()
                    .map(|old| PackingItem::total([old, row.quantity]))
                    .transpose()?
                    .unwrap_or(row.quantity);
                result.insert(key, quantity);
            }
        }
        Ok(result)
    }
    let previous = amounts(previous)?;
    let next = amounts(next)?;
    Ok(previous.iter().any(|(key, quantity)| next.get(key).is_none_or(|value| value < quantity)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::fulfillment::{
        DeliveryDispatch, DeliveryLineData, DeliveryPackage, DeliveryPacking, DeliveryType,
    };
    fn line(id: &str, quantity: &str) -> DeliveryLine {
        DeliveryLine::new(
            id.to_string().into(),
            DeliveryLineData {
                delivery_id: "delivery".to_string().into(),
                line_no: 1,
                sales_order_line_id: "sales-line".to_string().into(),
                quantity: quantity.parse().unwrap(),
                stock_reservation_id: None,
                purchase_line_sales_allocation_id: Some("allocation".to_string().into()),
            },
            DeliveryType::SupplierDirect,
        )
        .unwrap()
    }
    fn address(quantity: &str) -> AddressAllocation {
        let mut row = AddressAllocation {
            recipient: "张三".into(),
            phone: "13800000000".into(),
            region_code: "310115".into(),
            region_name: String::new(),
            address: "测试路1号".into(),
            quantity: quantity.parse().unwrap(),
        };
        row.normalize().unwrap();
        row
    }
    fn plan(quantity: &str) -> Vec<ArrangementLine> {
        vec![ArrangementLine { line_id: "sales-line".into(), allocations: vec![address(quantity)] }]
    }
    fn capacity(quantity: &str) -> Vec<ArrangementCapacity> {
        vec![ArrangementCapacity {
            line_id: "sales-line".into(),
            item_name: "商品".into(),
            quantity: quantity.parse().unwrap(),
            unit: "件".into(),
        }]
    }
    #[test]
    fn missing_and_partial_addresses_are_valid_but_unknown_and_excess_are_not() {
        validate_arrangement(
            &mut [ArrangementLine { line_id: "sales-line".into(), allocations: vec![] }],
            &capacity("5"),
        )
        .unwrap();
        validate_arrangement(&mut plan("3"), &capacity("5")).unwrap();
        assert!(validate_arrangement(&mut plan("6"), &capacity("5")).is_err());
        let mut rows = plan("1");
        rows[0].line_id = "other".into();
        assert!(validate_arrangement(&mut rows, &capacity("5")).is_err());
        assert!(validate_arrangement(&mut [], &capacity("5")).is_err());
    }
    #[test]
    fn recipient_region_and_quantity_are_required() {
        for field in ["recipient", "phone", "address", "region"] {
            let mut row = address("1");
            match field {
                "recipient" => row.recipient.clear(),
                "phone" => row.phone.clear(),
                "address" => row.address.clear(),
                _ => row.region_code = "310000".into(),
            }
            assert!(row.normalize().is_err());
        }
        let mut row = address("1");
        row.quantity = "0".parse().unwrap();
        assert!(row.normalize().is_err());
    }
    #[test]
    fn sequential_tasks_never_reuse_assigned_address_quantity() {
        let mut pool = AddressPool(plan("5"));
        let first = pool.assign(&[line("first", "3")], "v1".into(), b"test").unwrap();
        let next = pool.assign(&[line("next", "3")], "v1".into(), b"test").unwrap();
        assert_eq!(first.destinations[0].items[0].quantity, "3".parse().unwrap());
        assert_eq!(
            next.destinations.iter().find(|row| !row.pending_address).unwrap().items[0].quantity,
            "2".parse().unwrap()
        );
        assert_eq!(
            next.destinations.iter().find(|row| row.pending_address).unwrap().items[0].quantity,
            "1".parse().unwrap()
        );
    }
    #[test]
    fn shipped_addresses_cannot_be_removed_changed_or_reduced() {
        let original = [line("shipped", "2")];
        let snapshot = AddressPool(plan("2")).assign(&original, "v1".into(), b"test").unwrap();
        let mut valid = AddressPool(plan("5"));
        valid.consume(&snapshot, &original).unwrap();
        assert_eq!(valid.0[0].allocations[0].quantity, "3".parse().unwrap());
        assert!(AddressPool(plan("1")).consume(&snapshot, &original).is_err());
        let mut changed = plan("5");
        changed[0].allocations[0].phone = "13900000000".into();
        assert!(AddressPool(changed).consume(&snapshot, &original).is_err());
        assert!(AddressPool(vec![]).consume(&snapshot, &original).is_err());
    }
    #[test]
    fn pending_quantity_must_be_zero_while_known_address_can_ship() {
        let lines = vec![line("line", "5")];
        let snapshot = AddressPool(plan("3")).assign(&lines, "v1".into(), b"test").unwrap();
        let known = snapshot.destinations.iter().find(|row| !row.pending_address).unwrap();
        let mut packing = DeliveryPacking {
            source_version: "v1".into(),
            dispatch: snapshot
                .destinations
                .iter()
                .map(|row| DeliveryDispatch {
                    destination_key: row.key.clone(),
                    items: row
                        .items
                        .iter()
                        .map(|item| PackingItem {
                            quantity: if row.pending_address { "0".parse().unwrap() } else { item.quantity },
                            ..item.clone()
                        })
                        .collect(),
                })
                .collect(),
            packages: vec![DeliveryPackage {
                id: "package".into(),
                destination_key: known.key.clone(),
                carrier: "顺丰".into(),
                tracking_no: "SF1".into(),
                items: known.items.clone(),
            }],
        };
        packing.validate(&snapshot, &lines, true).unwrap();
        packing.dispatch.iter_mut().find(|row| row.destination_key == "pending-address").unwrap().items[0]
            .quantity = "1".parse().unwrap();
        assert!(packing.validate(&snapshot, &lines, false).is_err());
    }
    #[test]
    fn filling_pending_addresses_preserves_other_tasks_existing_allocation() {
        let known_lines = vec![line("known", "2")];
        let pending_lines = vec![line("pending", "3")];
        let known = AddressPool(plan("2")).assign(&known_lines, "v1".into(), b"test").unwrap();
        let pending = AddressPool(vec![]).assign(&pending_lines, "v1".into(), b"test").unwrap();
        // 待补任务先出现，也不得抢走另一任务已经录好的地址。
        let mut next = plan("3");
        next[0].allocations[0].address = "新增路2号".into();
        next[0].allocations.push(address("2"));
        let result = AddressPool(next)
            .reconcile(&[(pending, pending_lines), (known.clone(), known_lines)], b"test")
            .unwrap();
        assert!(result[1].same_arrangement(&known));
        assert!(
            result[0]
                .destinations
                .iter()
                .all(|row| !row.pending_address && row.address.ends_with("新增路2号"))
        );
    }
    #[test]
    fn only_replacing_or_reducing_existing_addresses_requires_reason() {
        assert!(!requires_change_reason(&plan("2"), &plan("5")).unwrap());
        assert!(requires_change_reason(&plan("5"), &plan("2")).unwrap());
        let mut next = plan("5");
        next[0].allocations[0].phone = "13900000000".into();
        assert!(requires_change_reason(&plan("5"), &next).unwrap());
        assert!(!requires_change_reason(&[], &plan("5")).unwrap());
    }
    #[test]
    fn duplicate_addresses_are_merged_before_quantity_validation() {
        let mut rows = plan("2");
        rows[0].allocations.push(address("3"));
        validate_arrangement(&mut rows, &capacity("5")).unwrap();
        assert_eq!(rows[0].allocations.len(), 1);
        assert_eq!(rows[0].allocations[0].quantity, "5".parse().unwrap());
    }
    #[test]
    fn sensitive_addresses_are_not_in_debug_output() {
        let row = address("1");
        let debug = format!("{row:?}");
        assert!(!debug.contains("13800000000"));
        assert!(!debug.contains("测试路"));
    }
}
