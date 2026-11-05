//! 地址补充后只重核实际受影响的包裹，保留其余录入。
use erp_core::money::Quantity;
use rust_decimal::Decimal;

use super::{DeliveryAddressSnapshot, DeliveryDispatch, DeliveryPacking, PackingItem};

impl DeliveryPacking {
    /// 将包裹草稿切换到新收货安排，只移除地址或分配数量变化的包裹。
    /// # 参数
    /// `previous` 为原可信快照，`next` 为已通过数量保护的新快照。
    /// # 返回
    /// 新包裹草稿及需要重新登记的包裹数量。
    /// # 错误
    /// 不返回错误；调用方在保存前继续执行完整草稿校验。
    pub fn rebase(
        &self,
        previous: &DeliveryAddressSnapshot,
        next: &DeliveryAddressSnapshot,
    ) -> (Self, usize) {
        let unchanged = |destination: &str, line: &str| {
            previous.source_version == next.source_version
                && capacity(previous, destination, line)
                    .is_some_and(|value| Some(value) == capacity(next, destination, line))
        };
        let packages = self
            .packages
            .iter()
            .filter(|package| {
                package.items.iter().all(|item| unchanged(&package.destination_key, &item.delivery_line_id))
            })
            .cloned()
            .collect::<Vec<_>>();
        let removed = self.packages.len() - packages.len();
        let dispatch = next
            .destinations
            .iter()
            .map(|destination| DeliveryDispatch {
                destination_key: destination.key.clone(),
                items: destination
                    .items
                    .iter()
                    .map(|item| PackingItem {
                        delivery_line_id: item.delivery_line_id.clone(),
                        quantity: if destination.pending_address {
                            Quantity::try_from(Decimal::ZERO).expect("零数量符合六位数量精度")
                        } else if unchanged(&destination.key, &item.delivery_line_id) {
                            self.dispatch
                                .iter()
                                .find(|row| row.destination_key == destination.key)
                                .and_then(|row| {
                                    row.items
                                        .iter()
                                        .find(|value| value.delivery_line_id == item.delivery_line_id)
                                })
                                .map(|value| value.quantity)
                                .unwrap_or(item.quantity)
                        } else {
                            item.quantity
                        },
                    })
                    .collect(),
            })
            .collect();
        (Self { source_version: next.source_version.clone(), dispatch, packages }, removed)
    }
}
fn capacity(source: &DeliveryAddressSnapshot, destination: &str, line: &str) -> Option<Quantity> {
    source
        .destinations
        .iter()
        .find(|row| row.key == destination && !row.pending_address)
        .and_then(|row| row.items.iter().find(|row| row.delivery_line_id == line))
        .map(|row| row.quantity)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::fulfillment::delivery_packing::tests::fixture;
    #[test]
    fn adding_address_preserves_packages_at_existing_addresses() {
        let (mut previous, _, mut packing) = fixture();
        let next = previous.clone();
        previous.destinations[1].pending_address = true;
        previous.destinations[1].key = "pending-address".into();
        packing.packages.pop();
        packing.dispatch[1].destination_key = "pending-address".into();
        for item in &mut packing.dispatch[1].items {
            item.quantity = Quantity::try_from(Decimal::ZERO).expect("零数量符合六位数量精度");
        }
        let original = packing.packages[0].clone();
        let (rebased, removed) = packing.rebase(&previous, &next);
        assert_eq!(removed, 0);
        assert_eq!(rebased.packages, vec![original]);
        assert_eq!(rebased.dispatch[1].items, next.destinations[1].items);
    }
    #[test]
    fn changing_one_address_only_removes_that_addresses_package() {
        let (previous, lines, packing) = fixture();
        let mut next = previous.clone();
        next.destinations[1].key = "new-address".into();
        next.destinations[1].address = "新地址".into();
        let (rebased, removed) = packing.rebase(&previous, &next);
        assert_eq!(removed, 1);
        assert_eq!(rebased.packages, vec![packing.packages[0].clone()]);
        rebased.validate(&next, &lines, false).unwrap();
        assert!(rebased.validate(&next, &lines, true).is_err());
    }
    #[test]
    fn changed_source_version_requires_rechecking_all_packages() {
        let (previous, _, packing) = fixture();
        let mut next = previous.clone();
        next.source_version = "new-version".into();
        let (rebased, removed) = packing.rebase(&previous, &next);
        assert_eq!(removed, 2);
        assert!(rebased.packages.is_empty());
    }
}
