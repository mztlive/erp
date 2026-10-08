//! 采购及履约读取使用当前供给事实提示风险，不改写正式采购事实。

use std::collections::{BTreeMap, BTreeSet, HashMap};

use erp_core::ids::PurchaseOrderId;
use erp_supply::entity::supplier_offering::{
    AvailabilityInterruptionReason, AvailabilityStatus, SupplierOffering, SupplierOfferingAvailability,
};
use erp_supply::repository::{
    SupplierOfferingAvailabilityRepositoryExt, SupplierOfferingExt, SupplierOfferingRepositoryExt,
};
use mongodb::Database;
use persistence_core::Executor;
use serde::Serialize;

use super::repository::warnings::current_purchase_sources;
use crate::{Error, Result};

/// 当前供给事实产生的提示；不能作为修改旧单金额、付款或状态的命令。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplyInterruptionWarning {
    pub offering_id: String,
    pub status: Option<AvailabilityStatus>,
    pub availability_version: Option<u64>,
    pub code: String,
    pub message: String,
}

/// 历史未记录正式选源时明确保持未知，不按供应商或 SKU 推断关系。
#[derive(Debug, Default, Clone, Serialize, PartialEq, Eq)]
pub struct PurchaseSupplyWarnings {
    pub purchase_order_owner_user_id: Option<String>,
    pub warnings: Vec<SupplyInterruptionWarning>,
    pub association_unknown: bool,
    pub association_notice: Option<String>,
}

/// 仅供已经独立授权采购或履约对象的消费方读取当前提示。
pub struct PurchaseSupplyWarningReader {
    db: Database,
}

impl PurchaseSupplyWarningReader {
    /// 绑定只读数据库，不创建通知或修改正式事实。
    /// # 参数
    /// `db` 为组合层数据库。
    /// # 返回
    /// 提示读取器。
    /// # 错误
    /// 无。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 沿已经授权采购的当前指针读取精确选源及当前可供状态。
    /// # 参数
    /// `order_id` 必须由消费方证明对象权限；`executor` 与该对象授权共用。
    /// # 返回
    /// 供给提示及未关联历史声明，不包括报价或金额。
    /// # 错误
    /// 事实缺失、读取超限或仓储失败时拒绝；终止采购返回空。
    pub async fn for_purchase(
        &self,
        order_id: &PurchaseOrderId,
        executor: &mut dyn Executor,
    ) -> Result<PurchaseSupplyWarnings> {
        self.for_purchases(std::slice::from_ref(order_id), executor)
            .await?
            .remove(&order_id.to_string())
            .ok_or_else(|| Error::NotFound("采购单不存在或无权查看".into()))
    }

    /// 批量读取当前页面已经授权的采购选源、正式关系状态及独立可供提示。
    /// # 参数
    /// `order_ids` 至多一千个独立授权的采购身份；`executor` 与对象授权共用。
    /// # 返回
    /// 按采购身份装配的提示；不存在的订单不返回。
    /// # 错误
    /// 范围超限、选源不完整或读取失败时拒绝。
    pub async fn for_purchases(
        &self,
        order_ids: &[PurchaseOrderId],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, PurchaseSupplyWarnings>> {
        let sources = current_purchase_sources(&self.db, order_ids, executor).await?;
        let ids = sources
            .values()
            .flat_map(|facts| &facts.sources)
            .flatten()
            .map(|source| (source.supplier_offering_id.to_string(), source.supplier_offering_id.clone()))
            .collect::<BTreeMap<_, _>>()
            .into_values()
            .collect::<Vec<_>>();
        if ids.len() > 10_000 {
            return Err(Error::ValidationError("采购选源超过提示读取上限".into()));
        }
        let availability = self
            .db
            .supplier_offering_availabilities()
            .find_by_offering_ids(&ids, executor)
            .await?
            .into_iter()
            .map(|fact| (fact.supplier_offering_id.to_string(), fact))
            .collect::<BTreeMap<_, _>>();
        let offerings = self
            .db
            .supplier_offerings()
            .list_by_ids(&ids, executor)
            .await?
            .into_iter()
            .map(|fact| (fact.base.id.clone(), fact))
            .collect::<BTreeMap<_, _>>();
        Ok(sources.into_iter().map(|(id,facts)| {
            let association_unknown=facts.sources.iter().any(Option::is_none);
            let ids=facts.sources.into_iter().flatten().map(|source|source.supplier_offering_id.to_string()).collect::<BTreeSet<_>>();
            let warnings=ids.iter().filter_map(|id|current_warning(id,offerings.get(id),availability.get(id))).collect();
            (id,PurchaseSupplyWarnings{purchase_order_owner_user_id:facts.owner_user_id,warnings,association_unknown,association_notice:association_unknown.then(||"历史采购行未记录正式供给选源，关联未知；须由当前采购负责人核验，不能按供应商或SKU推断。".into())})
        }).collect())
    }
}

/// 只根据当前提供方解释的中断事实产生提示，正常可供不产生警告。
///
/// # 参数
/// * `id` - 供给身份。
/// * `offering` - 当前供给；缺失表示正式关系未知。
/// * `fact` - 当前可供事实；缺失且供给未自述中断时记为可供未知。
///
/// # 返回
/// 需要提示时返回警告。供给存在且状态与可供事实都未中断时返回 `None`。
///
/// # 错误
/// 不返回错误。
pub(super) fn current_warning(
    id: &str,
    offering: Option<&SupplierOffering>,
    fact: Option<&SupplierOfferingAvailability>,
) -> Option<SupplyInterruptionWarning> {
    let Some(offering) = offering else {
        return Some(SupplyInterruptionWarning {
            offering_id: id.into(),
            status: fact.map(|fact| fact.availability_status),
            availability_version: fact.map(|fact| fact.base.version),
            code: "SUPPLY_RELATION_UNKNOWN".into(),
            message:
                "当前正式供给关系缺失，请当前采购负责人核验履约；旧单金额、付款条件和状态保持原正式事实。"
                    .into(),
        });
    };
    let reason = offering
        .stable
        .status
        .interruption_reason()
        .or_else(|| fact.and_then(|fact| fact.interruption_reason()));
    let Some(reason) = reason else {
        return fact.is_none().then(|| SupplyInterruptionWarning {
            offering_id: id.into(),
            status: None,
            availability_version: None,
            code: "SUPPLY_AVAILABILITY_UNKNOWN".into(),
            message: "当前可供事实缺失，请当前采购负责人核验履约；旧单金额、付款条件和状态保持原正式事实。"
                .into(),
        });
    };
    let (code, label) = match reason {
        AvailabilityInterruptionReason::SupplierStopped => ("SUPPLIER_STOPPED", "供应商已停止供应"),
        AvailabilityInterruptionReason::SupplyUnavailable => ("SUPPLY_UNAVAILABLE", "供应商当前缺货或不可供"),
        AvailabilityInterruptionReason::AvailabilityStale => ("SUPPLY_AVAILABILITY_STALE", "可供事实已过期"),
        AvailabilityInterruptionReason::ZeroInventory => ("SUPPLY_ZERO_INVENTORY", "供应商当前库存为零"),
    };
    Some(SupplyInterruptionWarning {
        offering_id: id.into(),
        status: fact.map(|fact| fact.availability_status),
        availability_version: fact.map(|fact| fact.base.version),
        code: code.into(),
        message: format!("{label}，请当前采购负责人核验未完成履约；旧单金额、付款条件和状态保持原正式事实。"),
    })
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::Instant;
    use erp_core::ids::{SkuId, SupplierAccountId, SupplierOfferingAvailabilityId, SupplierOfferingId};
    use erp_supply::entity::supplier_offering::{
        OfferingSourceType, OfferingStatus, SupplierOfferingAvailabilityData, SupplierOfferingData,
    };
    use serde_json::to_value;

    use super::*;

    fn offering() -> SupplierOffering {
        SupplierOffering::new(
            SupplierOfferingId::new("offering1"),
            SupplierOfferingData {
                sku_id: SkuId::new("sku"),
                supplier_id: SupplierAccountId::new("supplier"),
                supplier_product_code: None,
                supplier_sku_code: "ordering-code".into(),
                source_type: OfferingSourceType::Manual,
                source_connection_id: None,
                maintainer_user_id: "buyer".into(),
                business_org_unit_id: "company".into(),
            },
            "buyer",
        )
        .unwrap()
    }

    #[test]
    fn current_supply_warning_does_not_rewrite_or_expose_frozen_commercial_terms() {
        let mut fact = SupplierOfferingAvailability::new(
            SupplierOfferingAvailabilityId::new("availability1"),
            SupplierOfferingAvailabilityData {
                supplier_offering_id: SupplierOfferingId::new("offering1"),
                availability_status: AvailabilityStatus::Available,
                available_quantity: None,
                source_updated_at: Instant::from_unix_secs(1),
                received_at: Instant::from_unix_secs(1),
                source_revision_token: None,
                updated_by: "supplier1".into(),
            },
        )
        .unwrap();
        let offering = offering();
        assert!(current_warning("offering1", Some(&offering), Some(&fact)).is_none());
        fact.availability_status = AvailabilityStatus::Stopped;
        fact.base.version = 9;
        let warning = current_warning("offering1", Some(&offering), Some(&fact)).unwrap();
        assert_eq!(warning.code, "SUPPLIER_STOPPED");
        assert_eq!(warning.availability_version, Some(9));
        let value = to_value(warning).unwrap();
        assert!(value.get("unit_cost_gross").is_none());
        assert!(value.get("payment_term").is_none());
        assert_eq!(current_warning("offering1", Some(&offering), None).unwrap().status, None);
    }

    #[test]
    fn stopped_or_paused_relationship_warns_even_when_availability_is_available() {
        let mut offering = offering();
        let fact = SupplierOfferingAvailability::new(
            SupplierOfferingAvailabilityId::new("availability1"),
            SupplierOfferingAvailabilityData {
                supplier_offering_id: SupplierOfferingId::new("offering1"),
                availability_status: AvailabilityStatus::Available,
                available_quantity: None,
                source_updated_at: Instant::from_unix_secs(1),
                received_at: Instant::from_unix_secs(1),
                source_revision_token: None,
                updated_by: "supplier".into(),
            },
        )
        .unwrap();
        let before = fact.clone();
        for (status, code) in
            [(OfferingStatus::Stopped, "SUPPLIER_STOPPED"), (OfferingStatus::Paused, "SUPPLY_UNAVAILABLE")]
        {
            offering.stable.status = status;
            let warning = current_warning("offering1", Some(&offering), Some(&fact)).unwrap();
            assert_eq!(warning.code, code);
            assert_eq!(warning.status, Some(AvailabilityStatus::Available));
            assert!(warning.message.contains("当前采购负责人"));
            assert_eq!(current_warning("offering1", Some(&offering), None).unwrap().code, code);
        }
        assert_eq!(fact, before);
        assert_eq!(current_warning("offering1", None, Some(&fact)).unwrap().code, "SUPPLY_RELATION_UNKNOWN");
    }
}
