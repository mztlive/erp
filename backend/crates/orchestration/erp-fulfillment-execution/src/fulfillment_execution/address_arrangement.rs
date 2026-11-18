//! 销售与当前履约处理人共用收货安排；审批版本不变，已发快照不可改写。
mod context;
mod sources;
mod write;

use std::fmt;

use application_core::AuditActor;
use erp_core::money::Quantity;
use erp_fulfillment::entity::address_arrangement::{AddressAllocation, ArrangementCapacity, ArrangementLine};
use persistence_core::{Executor, Transactional};
use serde::{Deserialize, Serialize};

use super::FulfillmentProcess;
use crate::Result;

/// 受权限保护的收货安排编辑上下文。
#[derive(Debug, Clone, Serialize)]
pub struct AddressArrangementView {
    pub version: String,
    pub lines: Vec<ArrangementRow>,
    pub registered_packages: usize,
    pub source_changed: bool,
    pub blocked_reason: Option<String>,
}
/// 商品及可编辑地址分配，数量由服务端决定。
#[derive(Debug, Clone, Serialize)]
pub struct ArrangementRow {
    #[serde(flatten)]
    pub capacity: ArrangementCapacity,
    pub allocations: Vec<AddressAllocation>,
    pub shipped_addresses: Vec<ShippedAddress>,
}
/// 已发收件资料只读显示，调试输出不含个人资料。
#[derive(Clone, Serialize)]
pub struct ShippedAddress {
    pub key: String,
    pub recipient: String,
    pub phone: String,
    pub address: String,
    pub quantity: Quantity,
}
impl fmt::Debug for ShippedAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ShippedAddress").field("quantity", &self.quantity).finish_non_exhaustive()
    }
}
/// 全量地址输入；修改已有安排必须填写原因并确认包裹重核。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveAddressArrangement {
    pub expected_version: String,
    pub lines: Vec<ArrangementLine>,
    pub reason: String,
    pub reset_packages: bool,
}
impl FulfillmentProcess {
    /// 销售单编辑收货安排之前，在同一快照检查当前账号的销售范围，再返回可编辑商品、地址和并发版本。
    /// 范围不足、缺少生效销售版本或来源异常时不交付安排。
    pub async fn sales_address_arrangement(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<AddressArrangementView> {
        let process = self.clone();
        let id = id.to_owned();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    context::authorize_sales(&process, &id, &actor, false, executor).await?;
                    process.address_arrangement_snapshot(&id, None, executor).await
                })
            })
            .await
    }
    /// 当前发货任务补充地址时，只读取该任务的商品、数量和可编辑安排，不授予其他任务的访问权。
    /// 非当前处理人、已发货或供应商接口子单时拒绝。
    pub async fn delivery_address_arrangement(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<AddressArrangementView> {
        let process = self.clone();
        let id = id.to_owned();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let delivery = context::authorize_delivery(&process, &id, &actor, executor).await?;
                    process
                        .address_arrangement_snapshot(delivery.sales_order_id.as_ref(), Some(&id), executor)
                        .await
                })
            })
            .await
    }

    // 在原授权快照中读取销售需求、地址及发货资料，事务成功后才交付视图。
    async fn address_arrangement_snapshot(
        &self,
        sales_id: &str,
        delivery_id: Option<&str>,
        executor: &mut dyn Executor,
    ) -> Result<AddressArrangementView> {
        let mut context = context::Context::load(self, sales_id, executor).await?;
        context.load_snapshots(self, executor).await?;
        context.view(delivery_id)
    }
}
pub(super) use context::planned_source;
