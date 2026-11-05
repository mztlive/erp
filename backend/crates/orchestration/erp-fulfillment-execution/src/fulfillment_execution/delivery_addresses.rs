//! 授权发货详情的地址读取与来源快照；敏感资料只加密落库。
use std::collections::BTreeMap;

use erp_core::ids::{DeliveryId, SalesOrderRevisionId};
use erp_core::money::Quantity;
use erp_fulfillment::dto::DeliveryDetailView;
use erp_fulfillment::entity::fulfillment::{
    Delivery, DeliveryAddressSnapshot, DeliveryDestination, DeliveryLine, PackingItem,
};
use erp_fulfillment::repository::FulfillmentExt;
use erp_party::SensitiveDataCodec;
use erp_procurement::repository::PurchaseOrderExt;
use erp_sales::repository::SalesOrderExt;
use erp_sales::repository::prelude::*;
use erp_supply::api_order::for_purchase;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction};

use super::FulfillmentProcess;
use crate::{Error, Result};

impl FulfillmentProcess {
    /// 在已有发货详情权限下读取地址和包裹，正式记录只使用冻结快照。
    /// # 参数
    /// `id` 为已授权路由中的发货单身份。
    /// # 返回
    /// 表头、真实行、地址分配及包裹；无可信地址时返回空分配。
    /// # 错误
    /// 来源缺失、快照损坏或数据库读取失败时拒绝。
    pub async fn delivery_packing_detail(&self, id: &str) -> Result<DeliveryDetailView> {
        let mut view = self.domain().delivery_detail(id).await?;
        let delivery = self
            .db
            .deliveries()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("发货单不存在".into()))?;
        let lines = self
            .db
            .fulfillment()
            .delivery_lines_by_delivery_ids(&[DeliveryId::new(id)], &mut NoTransaction)
            .await?;
        view.address_snapshot = if delivery.is_editable() {
            address_source(
                &self.db,
                &self.sensitive_data,
                &self.fingerprint_key,
                &delivery,
                &lines,
                &mut NoTransaction,
            )
            .await?
        } else {
            frozen_addresses(&self.sensitive_data, &delivery)?
        };
        if delivery.is_editable()
            && view.address_snapshot.is_none()
            && !external_delivery(&self.db, &delivery, &mut NoTransaction).await?
        {
            view.packing_unavailable_reason = Some("此草稿的数量无法与来源收货地址完整对应，请先核对来源交付安排并重新分配发货任务。原物流记录已保留。".into());
        }
        Ok(view)
    }
}

pub(super) fn frozen_addresses(
    codec: &SensitiveDataCodec,
    delivery: &Delivery,
) -> Result<Option<DeliveryAddressSnapshot>> {
    delivery
        .packing_addresses_encrypted
        .as_ref()
        .map(|cipher| {
            let plain = codec.decrypt(cipher)?;
            serde_json::from_str(&plain).map_err(|_| Error::Internal("发货地址快照无法读取".into()))
        })
        .transpose()
}

pub(super) fn freeze_addresses(
    codec: &SensitiveDataCodec,
    delivery: &mut Delivery,
    snapshot: &DeliveryAddressSnapshot,
) -> Result<()> {
    let plain =
        serde_json::to_string(snapshot).map_err(|_| Error::Internal("发货地址快照无法保存".into()))?;
    delivery.packing_addresses_encrypted = Some(codec.encrypt(&plain)?);
    Ok(())
}

pub(super) async fn address_source(
    db: &Database,
    codec: &SensitiveDataCodec,
    key: &[u8],
    delivery: &Delivery,
    lines: &[DeliveryLine],
    executor: &mut dyn Executor,
) -> Result<Option<DeliveryAddressSnapshot>> {
    let (revision, version) = source_context(db, delivery, executor).await?;
    if let Some(frozen) = frozen_addresses(codec, delivery)? {
        if frozen.source_version != version {
            return Err(Error::ConflictError("来源交付安排已变化，请核对原发货草稿后重新分配".into()));
        }
        return Ok(Some(frozen));
    }
    if external_delivery(db, delivery, executor).await? {
        return Ok(None);
    }
    if let Some(snapshot) =
        super::address_arrangement::planned_source(db, codec, key, delivery, &version, executor).await?
    {
        return Ok(Some(snapshot));
    }
    collect_destinations(db, key, revision, version, lines, executor).await
}

pub(super) async fn source_context(
    db: &Database,
    delivery: &Delivery,
    executor: &mut dyn Executor,
) -> Result<(String, String)> {
    let sales = db
        .sales_orders()
        .find_by_id(delivery.sales_order_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::NotFound("来源销售单不存在".into()))?;
    let revision =
        sales.stable.current_revision_id.ok_or_else(|| Error::ConflictError("销售单缺少生效版本".into()))?;
    let purchase = if let Some(id) = &delivery.purchase_order_id {
        Some(
            db.purchase_orders()
                .find_by_id(id.as_ref(), executor)
                .await?
                .ok_or_else(|| Error::NotFound("来源采购单不存在".into()))?,
        )
    } else {
        None
    };
    let version = format!(
        "{}:{}",
        revision,
        purchase
            .as_ref()
            .and_then(|order| order.stable.current_revision_id.as_deref())
            .unwrap_or("warehouse")
    );
    Ok((revision, version))
}

async fn collect_destinations(
    db: &Database,
    key: &[u8],
    revision: String,
    version: String,
    lines: &[DeliveryLine],
    executor: &mut dyn Executor,
) -> Result<Option<DeliveryAddressSnapshot>> {
    let sales_lines = db
        .sales_order_revision_lines()
        .list_lines_by_revision(&SalesOrderRevisionId::new(revision), executor)
        .await?;
    let goods = db
        .sales_order_goods_service_line_revisions()
        .list_by_revision_line_ids(
            &sales_lines.iter().map(|line| line.base.id.clone().into()).collect::<Vec<_>>(),
            executor,
        )
        .await?;
    let ids = lines
        .iter()
        .filter_map(|line| line.purchase_line_sales_allocation_id.as_ref().map(ToString::to_string))
        .collect::<Vec<_>>();
    let allocations = db.purchase_line_sales_allocations().list_active_by_ids(&ids, executor).await?;
    let purchase_ids =
        allocations.iter().map(|row| row.purchase_order_revision_line_id.to_string()).collect::<Vec<_>>();
    let purchase_lines =
        db.purchase_order_revision_lines().list_active_by_ids(&purchase_ids, executor).await?;
    let mut destinations = BTreeMap::new();
    for line in lines {
        if let Some(allocation_id) = &line.purchase_line_sales_allocation_id {
            let rows = purchase_rows(allocation_id.as_ref(), &allocations, &purchase_lines)?;
            if !append_destinations(&mut destinations, key, line, rows)? {
                return Ok(None);
            }
        } else {
            let source = sales_lines
                .iter()
                .find(|source| source.sales_order_line_id == line.sales_order_line_id)
                .ok_or_else(|| Error::ConflictError("销售商品已变更，请核对发货明细".into()))?;
            let goods = goods
                .iter()
                .find(|goods| goods.revision_line_id.as_ref() == source.base.id)
                .ok_or_else(|| Error::ConflictError("销售交付安排缺失".into()))?;
            let rows = goods
                .delivery_allocations
                .iter()
                .map(|row| (row.recipient.as_str(), row.phone.as_str(), row.full_address(), row.quantity))
                .collect::<Vec<_>>();
            if !append_destinations(&mut destinations, key, line, rows)? {
                return Ok(None);
            }
        }
    }
    Ok(Some(DeliveryAddressSnapshot {
        source_version: version,
        destinations: destinations.into_values().collect(),
    }))
}

/// 从真实采购销售分配提取已经审核的地址数量。
fn purchase_rows<'a>(
    allocation_id: &str,
    allocations: &[erp_procurement::entity::purchase_order::PurchaseLineSalesAllocation],
    purchase_lines: &'a [erp_procurement::entity::purchase_order::PurchaseOrderRevisionLine],
) -> Result<Vec<(&'a str, &'a str, String, Quantity)>> {
    let allocation = allocations
        .iter()
        .find(|row| row.base.id == allocation_id)
        .ok_or_else(|| Error::ConflictError("采购销售分配不存在".into()))?;
    let source = purchase_lines
        .iter()
        .find(|row| row.base.id == allocation.purchase_order_revision_line_id.as_ref())
        .ok_or_else(|| Error::ConflictError("采购商品明细不存在".into()))?;
    Ok(source
        .delivery_allocations
        .iter()
        .map(|row| (row.recipient.as_str(), row.phone.as_str(), row.full_address(), row.quantity))
        .collect::<Vec<_>>())
}

fn append_destinations(
    destinations: &mut BTreeMap<String, DeliveryDestination>,
    key: &[u8],
    line: &DeliveryLine,
    mut rows: Vec<(&str, &str, String, Quantity)>,
) -> Result<bool> {
    if rows.len() == 1 && line.quantity <= rows[0].3 {
        rows[0].3 = line.quantity;
    }
    let total = PackingItem::total(rows.iter().map(|row| row.3))?;
    // 历史部分草稿没有地址消费凭据时不猜测剩余属于哪个地址。
    if rows.is_empty() {
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
            .push(PackingItem { delivery_line_id: line.base.id.clone(), quantity: line.quantity });
        return Ok(true);
    }
    if total != line.quantity {
        return Ok(false);
    }
    for (recipient, phone, address, quantity) in rows {
        let identity = serde_json::to_string(&(recipient, phone, &address))
            .map_err(|_| Error::Internal("收件资料无法规范化".into()))?;
        let id = Delivery::address_snapshot_fingerprint(&identity, key);
        let destination = destinations.entry(id.clone()).or_insert_with(|| DeliveryDestination {
            pending_address: false,
            key: id,
            recipient: recipient.into(),
            phone: phone.into(),
            address,
            items: Vec::new(),
        });
        if let Some(item) = destination.items.iter_mut().find(|item| item.delivery_line_id == line.base.id) {
            item.quantity = PackingItem::total([item.quantity, quantity])?;
        } else {
            destination.items.push(PackingItem { delivery_line_id: line.base.id.clone(), quantity });
        }
    }
    Ok(true)
}

/// API 子单使用供应商冻结地址和独立观察流程，不接受整张采购的地址推断。
pub(super) async fn external_delivery(
    db: &Database,
    delivery: &Delivery,
    executor: &mut dyn Executor,
) -> Result<bool> {
    let Some(id) = &delivery.purchase_order_id else { return Ok(false) };
    let executions = for_purchase(db, id.as_ref(), None, executor).await?;
    Ok(executions.iter().any(|execution| format!("{}-delivery", execution.base.id) == delivery.base.id))
}

/// 在确认事务内写入来源版本锁，使并发改地址与发货确认不能各自提交。
pub(super) async fn guard_sources(
    db: &Database,
    delivery: &Delivery,
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut sales = db
        .sales_orders()
        .find_by_id(delivery.sales_order_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::NotFound("来源销售单不存在".into()))?;
    db.sales_orders().update(&mut sales, executor).await?;
    if let Some(id) = &delivery.purchase_order_id {
        let mut purchase = db
            .purchase_orders()
            .find_by_id(id.as_ref(), executor)
            .await?
            .ok_or_else(|| Error::NotFound("来源采购单不存在".into()))?;
        db.purchase_orders().update(&mut purchase, executor).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use erp_fulfillment::entity::fulfillment::{DeliveryLineData, DeliveryType};

    use super::*;
    fn line() -> DeliveryLine {
        DeliveryLine::new(
            "line".to_string().into(),
            DeliveryLineData {
                delivery_id: "delivery".to_string().into(),
                line_no: 1,
                sales_order_line_id: "sales-line".to_string().into(),
                quantity: "3".parse().unwrap(),
                stock_reservation_id: None,
                purchase_line_sales_allocation_id: Some("allocation".to_string().into()),
            },
            DeliveryType::SupplierDirect,
        )
        .unwrap()
    }
    #[test]
    fn identical_recipient_and_address_are_merged_before_partition() {
        let mut result = BTreeMap::new();
        assert!(
            append_destinations(
                &mut result,
                b"test-key",
                &line(),
                vec![
                    ("甲", "13800000000", "地址".into(), "1".parse().unwrap()),
                    ("甲", "13800000000", "地址".into(), "2".parse().unwrap()),
                ]
            )
            .unwrap()
        );
        assert_eq!(result.len(), 1);
        let address = result.values().next().unwrap();
        assert_eq!(address.items.len(), 1);
        assert_eq!(address.items[0].quantity, line().quantity);
    }
    #[test]
    fn partial_task_with_multiple_source_addresses_is_never_guessed() {
        let mut result = BTreeMap::new();
        assert!(
            !append_destinations(
                &mut result,
                b"test-key",
                &line(),
                vec![
                    ("甲", "13800000000", "地址甲".into(), "2".parse().unwrap()),
                    ("乙", "13900000000", "地址乙".into(), "2".parse().unwrap()),
                ]
            )
            .unwrap()
        );
        assert!(result.is_empty());
    }
    #[test]
    fn one_destination_can_safely_represent_a_partial_task() {
        let mut result = BTreeMap::new();
        assert!(
            append_destinations(
                &mut result,
                b"test-key",
                &line(),
                vec![("甲", "13800000000", "地址".into(), "5".parse().unwrap()),]
            )
            .unwrap()
        );
        assert_eq!(result.values().next().unwrap().items[0].quantity, line().quantity);
    }
}
