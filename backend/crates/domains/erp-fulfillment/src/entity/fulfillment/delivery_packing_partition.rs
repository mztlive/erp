//! 包裹逐地址拆分的数量守恒与身份重映射，不执行 I/O。
use std::collections::BTreeMap;

use erp_core::ids::{DeliveryId, DeliveryLineId};
use erp_core::money::Quantity;
use erp_core::{Error, Result};
use rust_decimal::Decimal;

use super::{
    Delivery, DeliveryAddressSnapshot, DeliveryLine, DeliveryLineData, DeliveryPacking, PackingItem,
};

/// 本次和剩余行、各自地址快照、正式包裹。
pub type PackingPartition =
    (Vec<DeliveryLine>, Vec<DeliveryLine>, DeliveryAddressSnapshot, DeliveryAddressSnapshot, DeliveryPacking);
impl DeliveryPacking {
    /// 按地址拆出本次正式明细和未发明细，保持每个地址商品数量守恒。
    /// # 参数
    /// 本次和剩余发货身份、真实来源行、已核验地址快照及独立身份生成器。
    /// # 返回
    /// 本次行、剩余行、各自快照和更新真实行身份后的包裹。
    /// # 错误
    /// 来源缺失、数量越界、身份或行号非法时拒绝，不执行任何持久化。
    pub fn partition(
        self,
        delivery: &Delivery,
        rest: &Delivery,
        original: &[DeliveryLine],
        source: &DeliveryAddressSnapshot,
        mut next_id: impl FnMut() -> String,
    ) -> Result<PackingPartition> {
        self.validate(source, original, true)?;
        let mut packing = self;
        let mut shipped = Vec::new();
        let mut remaining = Vec::new();
        let mut current_source = source.clone();
        let mut rest_source = source.clone();
        let mut remap = BTreeMap::new();
        for (current, remainder) in current_source.destinations.iter_mut().zip(&mut rest_source.destinations)
        {
            let mut current_items = Vec::new();
            let mut rest_items = Vec::new();
            for item in &current.items {
                let line = original
                    .iter()
                    .find(|line| line.base.id == item.delivery_line_id)
                    .ok_or_else(|| Error::from("发货行已变更"))?;
                let quantity = dispatch_quantity(&packing, &current.key, &item.delivery_line_id);
                let left = item
                    .quantity
                    .to_decimal()
                    .checked_sub(quantity)
                    .ok_or_else(|| Error::from("发货数量超出范围"))?;
                if quantity > Decimal::ZERO {
                    let new = new_line(
                        line,
                        delivery,
                        Quantity::try_from(quantity)?,
                        original.iter().map(|row| row.line_no).max().unwrap_or(0),
                        shipped.len(),
                        next_id(),
                    )?;
                    remap.insert((current.key.clone(), item.delivery_line_id.clone()), new.base.id.clone());
                    current_items
                        .push(PackingItem { delivery_line_id: new.base.id.clone(), quantity: new.quantity });
                    shipped.push(new);
                }
                if left > Decimal::ZERO {
                    let new = new_line(line, rest, Quantity::try_from(left)?, 0, remaining.len(), next_id())?;
                    rest_items
                        .push(PackingItem { delivery_line_id: new.base.id.clone(), quantity: new.quantity });
                    remaining.push(new);
                }
            }
            current.items = current_items;
            remainder.items = rest_items;
        }
        remap_packing(&mut packing, &remap);
        current_source.destinations.retain(|row| !row.items.is_empty());
        rest_source.destinations.retain(|row| !row.items.is_empty());
        Ok((shipped, remaining, current_source, rest_source, packing))
    }
}

/// 未选择的地址商品本次发货量为零。
fn dispatch_quantity(packing: &DeliveryPacking, address: &str, line: &str) -> Decimal {
    packing
        .dispatch
        .iter()
        .find(|row| row.destination_key == address)
        .and_then(|row| row.items.iter().find(|item| item.delivery_line_id == line))
        .map(|item| item.quantity.to_decimal())
        .unwrap_or(Decimal::ZERO)
}

fn remap_packing(packing: &mut DeliveryPacking, remap: &BTreeMap<(String, String), String>) {
    for row in &mut packing.dispatch {
        row.items.retain_mut(|item| {
            if let Some(id) = remap.get(&(row.destination_key.clone(), item.delivery_line_id.clone())) {
                item.delivery_line_id = id.clone();
                true
            } else {
                false
            }
        });
    }
    packing.dispatch.retain(|row| !row.items.is_empty());
    for package in &mut packing.packages {
        for item in &mut package.items {
            if let Some(id) = remap.get(&(package.destination_key.clone(), item.delivery_line_id.clone())) {
                item.delivery_line_id = id.clone();
            }
        }
    }
}

fn new_line(
    source: &DeliveryLine,
    target: &Delivery,
    quantity: Quantity,
    offset: u32,
    index: usize,
    id: String,
) -> Result<DeliveryLine> {
    DeliveryLine::new(
        DeliveryLineId::new(id),
        DeliveryLineData {
            delivery_id: DeliveryId::new(&target.base.id),
            line_no: offset
                .checked_add(u32::try_from(index + 1).map_err(|_| Error::from("发货行数超限"))?)
                .ok_or("发货行数超限")?,
            sales_order_line_id: source.sales_order_line_id.clone(),
            quantity,
            stock_reservation_id: source.stock_reservation_id.clone(),
            purchase_line_sales_allocation_id: source.purchase_line_sales_allocation_id.clone(),
        },
        target.delivery_type,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::fulfillment::delivery_packing::tests::fixture;
    use crate::entity::fulfillment::{DeliveryData, DeliveryType};
    fn header(id: &str) -> Delivery {
        Delivery::new(
            DeliveryId::new(id),
            DeliveryData {
                delivery_no: id.into(),
                delivery_type: DeliveryType::SupplierDirect,
                sales_order_id: "sales".to_string().into(),
                purchase_order_id: Some("purchase".to_string().into()),
                warehouse_id: None,
                tracking_entries: vec![],
                address_snapshot_encrypted: None,
                address_snapshot_fingerprint: None,
            },
        )
        .unwrap()
    }
    #[test]
    fn partial_dispatch_preserves_each_address_and_real_source_links() {
        let (source, original, mut packing) = fixture();
        packing.dispatch[1].items.iter_mut().for_each(|item| item.quantity = "0".parse().unwrap());
        packing.packages.pop();
        let mut next = 0;
        let (shipped, remaining, current, rest, packing) = packing
            .partition(&header("delivery"), &header("rest"), &original, &source, || {
                next += 1;
                format!("line-{next}")
            })
            .unwrap();
        assert_eq!(shipped.len(), 2);
        assert_eq!(remaining.len(), 2);
        assert_eq!(current.destinations.len(), 1);
        assert_eq!(current.destinations[0].key, "a");
        assert_eq!(rest.destinations.len(), 1);
        assert_eq!(rest.destinations[0].key, "b");
        assert_eq!(
            shipped[0].quantity.to_decimal() + remaining[0].quantity.to_decimal(),
            original[0].quantity.to_decimal()
        );
        assert_eq!(
            shipped[0].purchase_line_sales_allocation_id,
            original[0].purchase_line_sales_allocation_id
        );
        assert!(shipped.iter().all(|line| line.line_no > 2));
        packing.validate(&current, &shipped, true).unwrap();
        assert!(
            packing.packages[0]
                .items
                .iter()
                .all(|item| shipped.iter().any(|line| line.base.id == item.delivery_line_id))
        );
    }
    #[test]
    fn full_dispatch_splits_same_product_into_independent_address_facts() {
        let (source, original, packing) = fixture();
        let mut next = 0;
        let (shipped, remaining, current, rest, packing) = packing
            .partition(&header("delivery"), &header("rest"), &original, &source, || {
                next += 1;
                format!("line-{next}")
            })
            .unwrap();
        assert_eq!(shipped.len(), 4);
        assert!(remaining.is_empty());
        assert!(rest.destinations.is_empty());
        assert_eq!(
            shipped.iter().filter(|line| line.sales_order_line_id == original[0].sales_order_line_id).count(),
            2
        );
        packing.validate(&current, &shipped, true).unwrap();
        assert_ne!(
            current.destinations[0].items[0].delivery_line_id,
            current.destinations[1].items[0].delivery_line_id
        );
    }
    #[test]
    fn missing_address_remains_in_follow_up_without_blocking_known_address() {
        let (mut source, original, mut packing) = fixture();
        source.destinations[1].pending_address = true;
        source.destinations[1].recipient.clear();
        source.destinations[1].phone.clear();
        source.destinations[1].address.clear();
        for item in &mut packing.dispatch[1].items {
            item.quantity = "0".parse().unwrap();
        }
        packing.packages.pop();
        let mut next = 0;
        let (shipped, remaining, current, rest, _) = packing
            .partition(&header("delivery"), &header("rest"), &original, &source, || {
                next += 1;
                format!("line-{next}")
            })
            .unwrap();
        assert!(!shipped.is_empty());
        assert!(!remaining.is_empty());
        assert!(current.destinations.iter().all(|row| !row.pending_address));
        assert!(rest.destinations[0].pending_address);
        assert_eq!(rest.destinations[0].items.len(), 2);
    }
    #[test]
    fn remainder_can_be_fully_dispatched_without_reusing_first_address() {
        use crate::entity::fulfillment::{DeliveryDispatch, DeliveryPackage};
        let (source, original, mut packing) = fixture();
        packing.dispatch[1].items.iter_mut().for_each(|item| item.quantity = "0".parse().unwrap());
        packing.packages.pop();
        let mut next = 0;
        let mut identity = || {
            next += 1;
            format!("generated-{next}")
        };
        let (_, remaining, _, rest, _) = packing
            .partition(&header("first"), &header("second"), &original, &source, &mut identity)
            .unwrap();
        let destination = &rest.destinations[0];
        let followup = DeliveryPacking {
            source_version: rest.source_version.clone(),
            dispatch: vec![DeliveryDispatch {
                destination_key: destination.key.clone(),
                items: destination.items.clone(),
            }],
            packages: vec![DeliveryPackage {
                id: "last-package".into(),
                destination_key: destination.key.clone(),
                carrier: "顺丰".into(),
                tracking_no: "last-number".into(),
                items: destination.items.clone(),
            }],
        };
        let (shipped, remaining, current, rest, packing) = followup
            .partition(&header("second"), &header("unused"), &remaining, &rest, &mut identity)
            .unwrap();
        assert!(remaining.is_empty());
        assert!(rest.destinations.is_empty());
        assert_eq!(current.destinations.len(), 1);
        assert_eq!(current.destinations[0].key, "b");
        packing.validate(&current, &shipped, true).unwrap();
    }

    #[test]
    fn incomplete_source_cannot_drop_unassigned_task_quantity() {
        let (mut source, original, packing) = fixture();
        source.destinations.pop();
        assert!(
            packing
                .partition(&header("delivery"), &header("rest"), &original, &source, || "unused".into())
                .is_err()
        );
    }
}
