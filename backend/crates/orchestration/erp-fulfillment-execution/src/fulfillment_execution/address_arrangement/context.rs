//! 读取可信销售需求和既有发货快照；后补安排不得修改商业修订。
use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_core::ids::{DeliveryId, SalesOrderRevisionId};
use erp_fulfillment::entity::address_arrangement::{
    AddressAllocation, AddressArrangement, AddressPool, ArrangementCapacity, ArrangementLine,
};
use erp_fulfillment::entity::fulfillment::{Delivery, DeliveryAddressSnapshot, DeliveryLine};
use erp_fulfillment::repository::{AddressArrangementExt, FulfillmentExt};
use erp_party::SensitiveDataCodec;
use erp_permissions::sales_order as sales_order_permissions;
use erp_read_models::sales_center::access::SalesAccess;
use erp_sales::entity::sales_order::{CloseStatus, CommercialStatus, SalesOrder};
use erp_sales::repository::SalesOrderExt;
use erp_sales::repository::prelude::*;
use mongodb::Database;
use persistence_core::Executor;

use super::super::{FulfillmentProcess, delivery_addresses, task};
use super::sources::Sources;
use super::{AddressArrangementView, ArrangementRow, ShippedAddress};
use crate::{Error, Result};

pub(super) struct Context {
    pub sales: SalesOrder,
    pub revision: String,
    pub stored: Option<AddressArrangement>,
    pub capacities: Vec<ArrangementCapacity>,
    pub plan: Vec<ArrangementLine>,
    pub deliveries: Vec<Delivery>,
    pub lines: Vec<DeliveryLine>,
    pub snapshots: Vec<(String, DeliveryAddressSnapshot)>,
    pub sources: Option<Sources>,
}
impl Context {
    /// 载入生效销售需求及其履约记录。
    ///
    /// 用例实例、销售身份及当前执行器。返回同一事务中的销售来源上下文。销售未生效、已关闭或加密记录不可读时拒绝。
    pub async fn load(process: &FulfillmentProcess, id: &str, executor: &mut dyn Executor) -> Result<Self> {
        let db = &process.db;
        let sales = db
            .sales_orders()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("销售单不存在".into()))?;
        if sales.commercial_status != CommercialStatus::Effective || sales.close_status == CloseStatus::Closed
        {
            return Err(Error::BusinessLogicError("仅可补充未关闭的生效销售单收货安排".into()));
        }
        let revision = sales
            .stable
            .current_revision_id
            .clone()
            .ok_or_else(|| Error::ConflictError("销售单尚未生效".into()))?;

        let (capacities, mut plan) = source_lines(db, &revision, executor).await?;
        let stored = db.address_arrangements().find_by_id(id, executor).await?;
        if let Some(stored) = &stored
            && stored.source_revision == revision
        {
            plan = serde_json::from_str(&process.sensitive_data.decrypt(&stored.content_encrypted)?)
                .map_err(|_| Error::Internal("收货安排无法读取".into()))?;
        }

        let deliveries =
            db.fulfillment().deliveries_for_arrangement(&id.to_string().into(), executor).await?;
        let lines = db
            .fulfillment()
            .delivery_lines_by_delivery_ids(
                &deliveries.iter().map(|row| DeliveryId::new(&row.base.id)).collect::<Vec<_>>(),
                executor,
            )
            .await?;

        Ok(Self {
            sales,
            revision,
            stored,
            capacities,
            plan,
            deliveries,
            lines,
            snapshots: vec![],
            sources: None,
        })
    }
    /// 生成编辑上下文的并发版本。
    ///
    /// 可选的当前发货身份。销售、安排与任务版本组合。
    pub fn version(&self, delivery: Option<&str>) -> String {
        let task_version = delivery
            .and_then(|id| self.deliveries.iter().find(|row| row.base.id == id))
            .map(|row| row.base.version)
            .unwrap_or(0);
        format!(
            "{}:{}:{}:{}",
            self.revision,
            self.sales.base.version,
            self.stored.as_ref().map(|row| row.base.version).unwrap_or(0),
            task_version
        )
    }
    /// 选取当前任务的真实发货行。
    ///
    /// 发货身份。该任务的发货行。
    pub fn task_lines(&self, id: &str) -> Vec<DeliveryLine> {
        self.lines.iter().filter(|row| row.delivery_id.as_ref() == id).cloned().collect()
    }
    /// 载入冻结地址并分配未冻结任务。
    ///
    /// 用例实例及当前执行器。上下文保存地址快照与采购来源。冻结地址无法核对、来源缺失或解密失败时拒绝。
    pub async fn load_snapshots(
        &mut self,
        process: &FulfillmentProcess,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let sources = Sources::load(process, self, executor).await?;
        let mut pool = AddressPool(self.plan.clone());
        let planned = self.stored.as_ref().is_some_and(|row| row.source_revision == self.revision);
        for delivery in &self.deliveries {
            if let Some(frozen) = delivery_addresses::frozen_addresses(&process.sensitive_data, delivery)? {
                if planned {
                    pool.consume(&frozen, &self.task_lines(&delivery.base.id))?;
                }
                self.snapshots.push((delivery.base.id.clone(), frozen));
            }
        }

        for delivery in self
            .deliveries
            .iter()
            .filter(|row| row.is_editable() && row.packing_addresses_encrypted.is_none())
        {
            if sources.external.contains(&delivery.base.id) {
                continue;
            }
            let snapshot = if planned {
                Some(pool.assign(
                    &self.task_lines(&delivery.base.id),
                    sources.version(self, delivery)?,
                    &process.fingerprint_key,
                )?)
            } else {
                sources.original(self, delivery, &process.fingerprint_key)?
            };
            if let Some(snapshot) = snapshot {
                self.snapshots.push((delivery.base.id.clone(), snapshot));
            }
        }

        self.sources = Some(sources);
        Ok(())
    }
    /// 获取任务的销售及采购来源版本。
    ///
    /// 当前发货记录。来源版本组合。采购缺少生效版本或来源未载入时拒绝。
    pub fn source_version(&self, delivery: &Delivery) -> Result<String> {
        self.sources.as_ref().ok_or_else(|| Error::Internal("收货来源未载入".into()))?.version(self, delivery)
    }
    /// 生成限定编辑范围的收货安排。
    ///
    /// 可选的发货身份。含只读已发地址的编辑上下文。任务来源与销售安排无法核对时拒绝。
    pub fn view(&self, target: Option<&str>) -> Result<AddressArrangementView> {
        let (capacities, plan) = if let Some(id) = target {
            self.task_plan(id)?
        } else {
            (self.capacities.clone(), self.plan.clone())
        };
        Ok(AddressArrangementView {
            version: self.version(target),
            blocked_reason: self
                .sources
                .as_ref()
                .filter(|sources| !sources.external.is_empty())
                .map(|_| "该销售单已有供应商接口订单，收货地址需按供应商订单变更流程核对".to_string()),
            source_changed: self
                .stored
                .as_ref()
                .is_some_and(|stored| stored.source_revision != self.revision),
            registered_packages: self
                .deliveries
                .iter()
                .filter(|row| row.is_editable() && target.is_none_or(|id| row.base.id == id))
                .map(|row| row.packing.as_ref().map(|packing| packing.packages.len()).unwrap_or(0))
                .sum(),
            lines: capacities
                .into_iter()
                .map(|capacity| ArrangementRow {
                    shipped_addresses: if target.is_none() {
                        self.shipped_addresses(&capacity.line_id)
                    } else {
                        vec![]
                    },
                    allocations: plan
                        .iter()
                        .find(|row| row.line_id == capacity.line_id)
                        .map(|row| row.allocations.clone())
                        .unwrap_or_default(),
                    capacity,
                })
                .collect(),
        })
    }
    fn shipped_addresses(&self, id: &str) -> Vec<ShippedAddress> {
        let mut result = vec![];
        for delivery in self.deliveries.iter().filter(|row| !row.is_editable()) {
            if let Some((_, snapshot)) = self.snapshots.iter().find(|(key, _)| *key == delivery.base.id) {
                for destination in &snapshot.destinations {
                    for item in &destination.items {
                        if self.lines.iter().any(|line| {
                            line.base.id == item.delivery_line_id && line.sales_order_line_id.as_ref() == id
                        }) {
                            result.push(ShippedAddress {
                                key: format!("{}:{}", destination.key, item.delivery_line_id),
                                recipient: destination.recipient.clone(),
                                phone: destination.phone.clone(),
                                address: destination.address.clone(),
                                quantity: item.quantity,
                            });
                        }
                    }
                }
            }
        }
        result
    }
    /// 将任务地址映射到真实发货行。
    ///
    /// 发货身份。当前任务数量上限及完整地址。商品或地址无法匹配当前销售需求时拒绝。
    pub fn task_plan(&self, id: &str) -> Result<(Vec<ArrangementCapacity>, Vec<ArrangementLine>)> {
        let lines = self.task_lines(id);
        let mut capacities = vec![];
        let mut plan = vec![];
        for line in &lines {
            let source = self
                .capacities
                .iter()
                .find(|row| row.line_id == line.sales_order_line_id.as_ref())
                .ok_or_else(|| Error::ConflictError("销售商品已变化，请核对来源".into()))?;
            capacities.push(ArrangementCapacity {
                line_id: line.base.id.clone(),
                quantity: line.quantity,
                ..source.clone()
            });

            let mut allocations = vec![];
            if let Some((_, snapshot)) = self.snapshots.iter().find(|(key, _)| key == id) {
                for destination in snapshot.destinations.iter().filter(|row| !row.pending_address) {
                    for item in destination.items.iter().filter(|item| item.delivery_line_id == line.base.id)
                    {
                        let row = self
                            .plan
                            .iter()
                            .filter(|row| row.line_id == line.sales_order_line_id.as_ref())
                            .flat_map(|row| &row.allocations)
                            .find(|row| {
                                row.recipient == destination.recipient
                                    && row.phone == destination.phone
                                    && row.full_address() == destination.address
                            })
                            .ok_or_else(|| {
                                Error::ConflictError("收货安排已变化，请从销售单核对后更新安排".into())
                            })?;
                        allocations.push(AddressAllocation { quantity: item.quantity, ..row.clone() });
                    }
                }
            }
            plan.push(ArrangementLine { line_id: line.base.id.clone(), allocations });
        }
        Ok((capacities, plan))
    }
}
// 校验销售范围及操作权限。
//
// 用例实例、销售身份、操作人、写入标志及执行器。授权通过。无对象范围或操作权限时拒绝。
pub(super) async fn authorize_sales(
    process: &FulfillmentProcess,
    id: &str,
    actor: &AuditActor,
    writing: bool,
    executor: &mut dyn Executor,
) -> Result<()> {
    let permission =
        if writing { sales_order_permissions::HANDOVER } else { sales_order_permissions::DETAIL };
    SalesAccess::new(process.db.clone(), process.task_identity.clone())
        .require_object(actor, &permission, id, executor)
        .await?;
    Ok(())
}
// 校验当前履约任务处理权限。
//
// 用例实例、发货身份、操作人及执行器。可编辑的发货记录。非当前处理人、无执行权限、已发货或接口订单时拒绝。
pub(super) async fn authorize_delivery(
    process: &FulfillmentProcess,
    id: &str,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<Delivery> {
    let delivery = process
        .db
        .deliveries()
        .find_by_id(id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("发货任务不存在".into()))?;
    if !delivery.is_editable()
        || delivery_addresses::external_delivery(&process.db, &delivery, executor).await?
    {
        return Err(Error::BusinessLogicError("已发货或供应商接口订单不能在此修改地址".into()));
    }

    let mut task = task::load_single_open_task(&process.db, "delivery", id, executor).await?;
    task::ensure_task_matches_frozen_identity(&task, &task::FulfillmentTaskObject::Delivery(&delivery))?;
    super::super::task_access::require_assignee(
        &process.task_identity,
        actor,
        &task,
        super::super::task_access::TaskPurpose::Fulfillment,
        executor,
    )
    .await?;
    task.record_activity(Instant::now())?;
    Ok(delivery)
}

// 按独立收货安排扣除冻结任务，再为未冻结任务确定分配。
//
// 数据库、密文服务、指纹密钥、当前发货、来源版本及执行器。独立安排生成的当前任务地址；未配置或版本失效时返回空。历史地址不可核对、解密失败或已发数量被挪用时拒绝。
pub(in crate::fulfillment_execution) async fn planned_source(
    db: &Database,
    codec: &SensitiveDataCodec,
    key: &[u8],
    delivery: &Delivery,
    version: &str,
    executor: &mut dyn Executor,
) -> Result<Option<DeliveryAddressSnapshot>> {
    let Some(record) =
        db.address_arrangements().find_by_id(delivery.sales_order_id.as_ref(), executor).await?
    else {
        return Ok(None);
    };
    if !version.starts_with(&format!("{}:", record.source_revision)) {
        return Ok(None);
    }
    let plan: Vec<ArrangementLine> = serde_json::from_str(&codec.decrypt(&record.content_encrypted)?)
        .map_err(|_| Error::Internal("收货安排无法读取".into()))?;
    let mut pool = AddressPool(plan);

    let deliveries = db.fulfillment().deliveries_for_arrangement(&delivery.sales_order_id, executor).await?;
    let lines = db
        .fulfillment()
        .delivery_lines_by_delivery_ids(
            &deliveries.iter().map(|row| DeliveryId::new(&row.base.id)).collect::<Vec<_>>(),
            executor,
        )
        .await?;

    for row in &deliveries {
        if let Some(snapshot) = delivery_addresses::frozen_addresses(codec, row)? {
            pool.consume(
                &snapshot,
                &lines
                    .iter()
                    .filter(|line| line.delivery_id.as_ref() == row.base.id)
                    .cloned()
                    .collect::<Vec<_>>(),
            )?;
        } else if !row.is_editable() {
            return Err(Error::ConflictError("历史已发记录缺少地址分配，请先核对收货安排".into()));
        }
    }

    for row in deliveries.iter().filter(|row| row.is_editable() && row.packing_addresses_encrypted.is_none())
    {
        let snapshot = pool.assign(
            &lines
                .iter()
                .filter(|line| line.delivery_id.as_ref() == row.base.id)
                .cloned()
                .collect::<Vec<_>>(),
            version.to_string(),
            key,
        )?;
        if row.base.id == delivery.base.id {
            return Ok(Some(snapshot));
        }
    }
    Ok(None)
}

async fn source_lines(
    db: &Database,
    revision: &str,
    executor: &mut dyn Executor,
) -> Result<(Vec<ArrangementCapacity>, Vec<ArrangementLine>)> {
    let source = db
        .sales_order_revision_lines()
        .list_lines_by_revision(&SalesOrderRevisionId::new(revision), executor)
        .await?;
    let goods = db
        .sales_order_goods_service_line_revisions()
        .list_by_revision_line_ids(
            &source.iter().map(|row| row.base.id.clone().into()).collect::<Vec<_>>(),
            executor,
        )
        .await?;

    let mut capacities = vec![];
    let mut plan = vec![];
    for row in &source {
        let Some(goods) = goods.iter().find(|goods| goods.revision_line_id.as_ref() == row.base.id) else {
            continue;
        };
        capacities.push(ArrangementCapacity {
            line_id: row.sales_order_line_id.to_string(),
            item_name: row.item_name_snapshot.clone(),
            quantity: goods.quantity,
            unit: goods.base_unit_code.clone(),
        });
        plan.push(ArrangementLine {
            line_id: row.sales_order_line_id.to_string(),
            allocations: goods
                .delivery_allocations
                .iter()
                .map(|row| AddressAllocation {
                    recipient: row.recipient.clone(),
                    phone: row.phone.clone(),
                    region_code: row.region_code.to_string(),
                    region_name: row.region_name.clone(),
                    address: row.address.clone(),
                    quantity: row.quantity,
                })
                .collect(),
        });
    }
    Ok((capacities, plan))
}
