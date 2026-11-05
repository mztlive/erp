//! 按订单批量读取采购版本与地址，避免逐任务重复读取整单。
use std::collections::{BTreeMap, HashSet};

use erp_core::money::Quantity;
use erp_fulfillment::entity::address_arrangement::{AddressAllocation, AddressPool, ArrangementLine};
use erp_fulfillment::entity::fulfillment::{Delivery, DeliveryAddressSnapshot, PackingItem};
use erp_procurement::repository::PurchaseOrderExt;
use erp_supply::api_order::for_sales_lines;
use persistence_core::Executor;

use super::super::FulfillmentProcess;
use super::context::Context;
use crate::{Error, Result};

pub(super) struct Sources {
    pub versions: BTreeMap<String, String>,
    pub allocations: BTreeMap<String, Vec<AddressAllocation>>,
    pub external: HashSet<String>,
}
impl Sources {
    /// 批量读取采购地址和供应商接口执行状态。
    /// # 参数
    /// 用例实例、销售上下文及执行器。
    /// # 返回
    /// 当前订单的采购版本、地址与接口执行身份。
    /// # 错误
    /// 关联采购行缺失或仓储读取失败时拒绝。
    pub async fn load(
        process: &FulfillmentProcess,
        context: &Context,
        executor: &mut dyn Executor,
    ) -> Result<Self> {
        let db = &process.db;
        let versions = purchase_versions(process, context, executor).await?;
        let ids = context
            .lines
            .iter()
            .filter_map(|line| line.purchase_line_sales_allocation_id.as_ref().map(ToString::to_string))
            .collect::<Vec<_>>();
        let allocations = db.purchase_line_sales_allocations().list_active_by_ids(&ids, executor).await?;
        let rows = db
            .purchase_order_revision_lines()
            .list_active_by_ids(
                &allocations
                    .iter()
                    .map(|row| row.purchase_order_revision_line_id.to_string())
                    .collect::<Vec<_>>(),
                executor,
            )
            .await?;
        let mut mapped = BTreeMap::new();
        for allocation in allocations {
            let source = rows
                .iter()
                .find(|row| row.base.id == allocation.purchase_order_revision_line_id.as_ref())
                .ok_or_else(|| Error::ConflictError("采购商品明细不存在".into()))?;
            mapped.insert(
                allocation.base.id,
                source
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
            );
        }
        let external = external_deliveries(process, context, executor).await?;
        Ok(Self { versions, allocations: mapped, external })
    }
    /// 计算发货对应的生效来源版本。
    /// # 参数
    /// 销售上下文及发货记录。
    /// # 返回
    /// 销售与采购版本组合。
    /// # 错误
    /// 采购缺少生效版本时拒绝。
    pub fn version(&self, context: &Context, delivery: &Delivery) -> Result<String> {
        let purchase = if let Some(id) = &delivery.purchase_order_id {
            self.versions
                .get(id.as_ref())
                .filter(|version| !version.is_empty())
                .ok_or_else(|| Error::ConflictError("采购单缺少生效版本，请核对来源".into()))?
                .as_str()
        } else {
            "warehouse"
        };
        Ok(format!("{}:{}", context.revision, purchase))
    }
    /// 按原采购或销售分配生成可信任务地址。
    /// # 参数
    /// 销售上下文、发货记录与指纹密钥。
    /// # 返回
    /// 可核对的地址快照；多地址数量不匹配返回空。
    /// # 错误
    /// 采购分配缺失或数量计算失败时拒绝。
    pub fn original(
        &self,
        context: &Context,
        delivery: &Delivery,
        key: &[u8],
    ) -> Result<Option<DeliveryAddressSnapshot>> {
        let mut plan = vec![];
        let mut lines = context.task_lines(&delivery.base.id);
        for line in &mut lines {
            let mut allocations = if let Some(id) = &line.purchase_line_sales_allocation_id {
                self.allocations
                    .get(id.as_ref())
                    .ok_or_else(|| Error::ConflictError("采购销售分配不存在".into()))?
                    .clone()
            } else {
                context
                    .plan
                    .iter()
                    .find(|row| row.line_id == line.sales_order_line_id.as_ref())
                    .map(|row| row.allocations.clone())
                    .unwrap_or_default()
            };
            if allocations.len() == 1 && line.quantity <= allocations[0].quantity {
                allocations[0].quantity = line.quantity;
            }
            let total: Quantity = PackingItem::total(allocations.iter().map(|row| row.quantity))?;
            if !allocations.is_empty() && total != line.quantity {
                return Ok(None);
            }
            plan.push(ArrangementLine { line_id: line.base.id.clone(), allocations });
            line.sales_order_line_id = line.base.id.clone().into();
        }
        Ok(Some(AddressPool(plan).assign(&lines, self.version(context, delivery)?, key)?))
    }
}

async fn external_deliveries(
    process: &FulfillmentProcess,
    context: &Context,
    executor: &mut dyn Executor,
) -> Result<HashSet<String>> {
    Ok(for_sales_lines(
        &process.db,
        &context.capacities.iter().map(|row| row.line_id.clone()).collect::<Vec<_>>(),
        executor,
    )
    .await?
    .into_iter()
    .map(|row| format!("{}-delivery", row.base.id))
    .collect())
}

async fn purchase_versions(
    process: &FulfillmentProcess,
    context: &Context,
    executor: &mut dyn Executor,
) -> Result<BTreeMap<String, String>> {
    let db = &process.db;
    let ids = context
        .deliveries
        .iter()
        .filter_map(|row| row.purchase_order_id.as_ref().map(ToString::to_string))
        .collect::<Vec<_>>();
    let purchases = db.purchase_orders().list_active_by_ids(&ids, executor).await?;
    let versions = purchases
        .into_iter()
        .map(|row| (row.base.id, row.stable.current_revision_id.unwrap_or_default()))
        .collect();
    Ok(versions)
}
