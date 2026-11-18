//! 采购行的正式选源事实；历史缺失保持未知，不能由供应商及SKU推断。

use std::collections::HashMap;

use erp_core::common::time::BusinessDate;
use erp_core::ids::{
    SkuId, SkuRevisionId, SupplierAccountId, SupplierOfferingId, SupplierOfferingRevisionId,
};
use erp_core::money::{Quantity, Rate, UnitPrice};
use erp_core::{Error, Result};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use super::{
    LineSupply, PurchaseInquiryQuoteSource, PurchaseLineInput, PurchaseLineType, PurchaseOrderRevisionLine,
    PurchaseOrderSubmissionLine,
};
use crate::entity::facts::AvailabilityStatus;

/// 本次创建依据实际消费的供给身份及版本，整组原子保存。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PurchaseOfferingSource {
    /// 选中的正式供给主键。
    pub supplier_offering_id: SupplierOfferingId,
    /// 创建时供给乐观锁版本。
    pub offering_version: u64,
    /// 选中的正式条款修订主键。
    pub supplier_offering_revision_id: SupplierOfferingRevisionId,
    /// 创建时条款修订乐观锁版本。
    pub revision_version: u64,
    /// 创建时可供投影乐观锁版本。
    pub availability_version: u64,
    /// 询价专项条款；空值沿用既有通用供给版本。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inquiry_quote: Option<PurchaseInquiryQuoteSource>,
}

impl PurchaseOfferingSource {
    /// 创建普通采购行时，从原事务核实的选中供给冻结供给、当前条款与可供投影的身份和版本。
    /// 条款或可供投影归属不符、当前指针不一致或身份版本缺失时拒绝；不从客户端补造选源。
    pub fn from_supply(supply: &LineSupply) -> Result<Self> {
        if supply.revision.supplier_offering_id.as_ref() != supply.offering.base.id.as_str()
            || supply.availability.supplier_offering_id.as_ref() != supply.offering.base.id.as_str()
            || supply.offering.stable.current_revision_id.as_deref() != Some(supply.revision.base.id.as_str())
        {
            return Err(Error::from("采购供给选源与当前条款或可供归属不一致"));
        }

        let source = Self {
            supplier_offering_id: supply.offering.base.id.clone().into(),
            offering_version: supply.offering.version,
            supplier_offering_revision_id: supply.revision.base.id.clone().into(),
            revision_version: supply.revision.version,
            availability_version: supply.availability.base.version,
            inquiry_quote: None,
        };
        source.ensure_valid()?;
        Ok(source)
    }

    /// 提交普通供给的采购行时，用同一事务读取已启用供给，重验冻结供应商、SKU、当前条款及供给、条款和可供版本。
    /// 同时检查业务日期上的条款有效性与本次数量可供性；任一事实变化即拒绝，不替换原冻结价格或版本。专项报价必须走专项重验。
    pub fn ensure_current_supply(
        &self,
        supply: &LineSupply,
        supplier_id: &SupplierAccountId,
        sku_id: &SkuId,
        quantity: Quantity,
        on_date: BusinessDate,
    ) -> Result<()> {
        self.ensure_valid()?;
        if self.inquiry_quote.is_some() {
            return Err(Error::from("专项报价必须按冻结专项条款重验，不得采用通用供给价格"));
        }
        if Self::from_supply(supply)? != *self
            || supply.offering.supplier_id != *supplier_id
            || supply.offering.sku_id != *sku_id
        {
            return Err(Error::from("采购供给选源或版本已变化，请重新核对"));
        }

        if !on_date.is_within(supply.revision.valid_from, supply.revision.valid_to)
            || supply.availability.availability_status != AvailabilityStatus::Available
            || quantity.to_decimal() <= Decimal::ZERO
            || supply.availability.available_quantity.is_some_and(|available| available < quantity)
        {
            return Err(Error::from("采购供给条款已失效或当前数量不可供，请重新核对"));
        }
        Ok(())
    }

    // 冻结采购行选源前核验供给、条款和可供投影的版本可追溯；若携带询价专项条款，还须验证其冻结商业条件。
    fn ensure_valid(&self) -> Result<()> {
        if let Some(quote) = &self.inquiry_quote {
            quote.validate()?;
        }
        if self.supplier_offering_id.as_ref().trim().is_empty()
            || self.supplier_offering_revision_id.as_ref().trim().is_empty()
            || self.offering_version == 0
            || self.revision_version == 0
            || self.availability_version == 0
        {
            return Err(Error::from("采购正式选源身份或版本缺失"));
        }
        Ok(())
    }
}

// 构造采购行时只允许商品或服务行携带选源，未记录来源的历史行保持空值。
// 已提供的选源须有完整身份及版本；专项报价行的采购含税单价与进项税率必须等于原报价，物流费用行不得选源。
pub(super) fn ensure_offering_source(
    kind: PurchaseLineType,
    source: Option<&PurchaseOfferingSource>,
    price: Option<UnitPrice>,
    tax: Option<Rate>,
) -> Result<()> {
    if let Some(source) = source {
        if kind != PurchaseLineType::ItemService {
            return Err(Error::from("物流费用行不得携带供给选源事实"));
        }
        source.ensure_valid()?;
        if source
            .inquiry_quote
            .as_ref()
            .is_some_and(|quote| price != Some(quote.unit_cost_gross) || tax != Some(quote.input_tax_rate))
        {
            return Err(Error::from("专项采购单价及税率必须等于原始报价，禁止任意覆盖"));
        }
    }
    Ok(())
}

/// 从真实旧草稿行继承选源，客户端编辑只影响已允许的采购内容。
///
/// 原地补齐对应原行的选源；旧行未知仍为None。请求显式调整交付安排时保留请求值，由调用方按销售当前收件资料复验；未调整时沿用原行。来源行缺失、
/// SKU版本变化或原行身份歧义时拒绝。
///
/// - `inputs`：已验证原销售、SKU来源不变的请求行。
/// - `existing`：当前服务端草稿或冻结提交的完整行。
pub fn inherit_submission_sources(
    inputs: &mut [PurchaseLineInput],
    existing: &[PurchaseOrderSubmissionLine],
) -> Result<()> {
    inherit_sources(
        inputs,
        existing.iter().map(|line| SourceBinding {
            stable_line: line.sales_order_line_id.as_ref().map(|id| id.as_ref()),
            sku: line.sku_id.as_ref(),
            revision: line.sku_revision_id.as_ref(),
            source: line.supplier_offering_source.as_ref(),
            delivery: &line.delivery_allocations,
            kind: line.line_type,
        }),
    )
}

/// 采购变更从明确基准行复制原选源，不重新猜测或切换供给。
///
/// 原地继承精确供给选源，冻结价格等原有规则由调用方执行。来源不匹配或歧义时拒绝。
pub fn inherit_revision_sources(
    inputs: &mut [PurchaseLineInput],
    existing: &[PurchaseOrderRevisionLine],
) -> Result<()> {
    inherit_sources(
        inputs,
        existing.iter().map(|line| SourceBinding {
            stable_line: line.sales_order_line_id.as_ref().map(|id| id.as_ref()),
            sku: line.sku_id.as_ref(),
            revision: line.sku_revision_id.as_ref(),
            source: line.supplier_offering_source.as_ref(),
            delivery: &line.delivery_allocations,
            kind: line.line_type,
        }),
    )
}

/// 来源按已冻结的销售稳定行、SKU和SKU版本匹配。
struct SourceBinding<'a> {
    stable_line: Option<&'a str>,
    sku: Option<&'a SkuId>,
    revision: Option<&'a SkuRevisionId>,
    source: Option<&'a PurchaseOfferingSource>,
    delivery: &'a [super::PurchaseDeliveryAllocation],
    kind: PurchaseLineType,
}

// 对应原行必须唯一；没有原选源的历史数据不补造来源。
fn inherit_sources<'a>(
    inputs: &mut [PurchaseLineInput],
    existing: impl Iterator<Item = SourceBinding<'a>>,
) -> Result<()> {
    let requested = inputs.iter().map(|line| line.delivery_allocations.clone()).collect::<Vec<_>>();
    let mut sources = HashMap::new();
    for line in existing.filter(|line| line.kind == PurchaseLineType::ItemService) {
        let key = source_key(line.stable_line, line.sku, line.revision)?;
        if sources.insert(key, line).is_some() {
            return Err(Error::from("采购原行身份歧义，不能继承正式选源"));
        }
    }

    for input in &mut *inputs {
        (input.supplier_offering_source, input.delivery_allocations) =
            if input.line_type == PurchaseLineType::LogisticsFee {
                (None, Vec::new())
            } else {
                let key = source_key(
                    input.sales_order_line_id.as_ref().map(|id| id.as_ref()),
                    input.sku_id.as_ref(),
                    input.sku_revision_id.as_ref(),
                )?;
                let source = sources
                    .get(&key)
                    .ok_or_else(|| Error::from("采购来源行或SKU版本已变化，不能继承正式选源"))?;
                (source.source.cloned(), source.delivery.to_vec())
            };
    }

    // 请求交付安排仅在所有行继承成功后恢复。
    for (input, delivery) in inputs.iter_mut().zip(requested) {
        if !delivery.is_empty() {
            input.delivery_allocations = delivery;
        }
    }
    Ok(())
}

// 构造原行精确身份，不使用供应商和SKU作为供给关联。
fn source_key<'a>(
    stable: Option<&'a str>,
    sku: Option<&'a SkuId>,
    revision: Option<&'a SkuRevisionId>,
) -> Result<(&'a str, &'a str, &'a str)> {
    Ok((
        stable.ok_or("采购来源缺少销售稳定行")?,
        sku.ok_or("采购来源缺少SKU")?.as_ref(),
        revision.ok_or("采购来源缺少SKU版本")?.as_ref(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::facts::{
        AvailabilityFact, AvailabilityStatus, CurrentRevisionFact, FactIdentity, OfferingFact,
        OfferingRevisionFact, VersionedFactIdentity,
    };

    /// 构造真正归属于同一供给的版本事实。
    fn supply() -> LineSupply {
        LineSupply {
            offering: OfferingFact {
                base: FactIdentity { id: "selected".into() },
                version: 7,
                stable: CurrentRevisionFact { current_revision_id: Some("selected-terms".into()) },
                sku_id: SkuId::new("sku"),
                supplier_id: "supplier".to_string().into(),
            },
            revision: OfferingRevisionFact {
                base: FactIdentity { id: "selected-terms".into() },
                version: 3,
                supplier_offering_id: SupplierOfferingId::new("selected"),
                valid_from: "2026-01-01".parse().unwrap(),
                valid_to: None,
                bulk_supply_price_gross: Some("10".parse().unwrap()),
                dropship_supply_price_gross: "11".parse().unwrap(),
                input_tax_rate: "0.13".parse().unwrap(),
                bulk_minimum_order_quantity: Some("1".parse().unwrap()),
            },
            availability: AvailabilityFact {
                base: VersionedFactIdentity { id: "availability".into(), version: 11 },
                supplier_offering_id: SupplierOfferingId::new("selected"),
                availability_status: AvailabilityStatus::Available,
                available_quantity: None,
                source_revision_token: None,
            },
            delivery: Default::default(),
        }
    }

    #[test]
    fn source_must_belong_to_selected_offering_current_terms_and_availability() {
        let source = PurchaseOfferingSource::from_supply(&supply()).unwrap();
        assert_eq!(source.offering_version, 7);
        assert_eq!(source.revision_version, 3);
        assert_eq!(source.availability_version, 11);

        let mut wrong_terms = supply();
        wrong_terms.revision.supplier_offering_id = SupplierOfferingId::new("foreign");
        assert!(PurchaseOfferingSource::from_supply(&wrong_terms).is_err());
        let mut wrong_pointer = supply();
        wrong_pointer.offering.stable.current_revision_id = Some("old-terms".into());
        assert!(PurchaseOfferingSource::from_supply(&wrong_pointer).is_err());
        let mut wrong_availability = supply();
        wrong_availability.availability.supplier_offering_id = SupplierOfferingId::new("foreign");
        assert!(PurchaseOfferingSource::from_supply(&wrong_availability).is_err());

        assert!(ensure_offering_source(PurchaseLineType::LogisticsFee, Some(&source), None, None).is_err());
        assert!(ensure_offering_source(PurchaseLineType::ItemService, None, None, None).is_ok());
    }

    #[test]
    fn current_supply_recheck_keeps_the_frozen_source_and_accepts_unknown_quantity() {
        let supply = supply();
        let source = PurchaseOfferingSource::from_supply(&supply).unwrap();
        let before = source.clone();
        source
            .ensure_current_supply(
                &supply,
                &SupplierAccountId::new("supplier"),
                &SkuId::new("sku"),
                "2".parse().unwrap(),
                "2026-10-04".parse().unwrap(),
            )
            .unwrap();
        assert_eq!(source, before);
    }

    #[test]
    fn current_supply_recheck_rejects_each_version_pointer_and_foreign_identity() {
        let original = supply();
        let source = PurchaseOfferingSource::from_supply(&original).unwrap();
        let mut changed = Vec::new();
        let mut value = original.clone();
        value.offering.version += 1;
        changed.push(value);
        let mut value = original.clone();
        value.revision.version += 1;
        changed.push(value);
        let mut value = original.clone();
        value.availability.base.version += 1;
        changed.push(value);
        let mut value = original.clone();
        value.offering.stable.current_revision_id = Some("new-terms".into());
        changed.push(value);
        let mut value = original.clone();
        value.offering.supplier_id = SupplierAccountId::new("foreign");
        changed.push(value);
        let mut value = original.clone();
        value.offering.sku_id = SkuId::new("foreign");
        changed.push(value);
        let mut value = original;
        value.availability.supplier_offering_id = SupplierOfferingId::new("foreign");
        changed.push(value);

        for current in changed {
            assert!(
                source
                    .ensure_current_supply(
                        &current,
                        &SupplierAccountId::new("supplier"),
                        &SkuId::new("sku"),
                        "2".parse().unwrap(),
                        "2026-10-04".parse().unwrap(),
                    )
                    .is_err()
            );
        }

        assert_eq!(source.offering_version, 7);
        assert_eq!(source.revision_version, 3);
        assert_eq!(source.availability_version, 11);
    }

    #[test]
    fn current_supply_recheck_rejects_unavailable_zero_insufficient_and_expired_terms() {
        let original = supply();
        let source = PurchaseOfferingSource::from_supply(&original).unwrap();
        let mut changed = Vec::new();
        let mut value = original.clone();
        value.availability.availability_status = AvailabilityStatus::Unavailable;
        changed.push(value);
        for quantity in ["0", "1"] {
            let mut value = original.clone();
            value.availability.available_quantity = Some(quantity.parse().unwrap());
            changed.push(value);
        }

        for expired_on in ["2026-10-03", "2026-10-04"] {
            let mut value = original.clone();
            value.revision.valid_to = Some(expired_on.parse().unwrap());
            changed.push(value);
        }
        let mut value = original;
        value.revision.valid_from = "2026-10-05".parse().unwrap();
        changed.push(value);

        for current in changed {
            assert!(
                source
                    .ensure_current_supply(
                        &current,
                        &SupplierAccountId::new("supplier"),
                        &SkuId::new("sku"),
                        "2".parse().unwrap(),
                        "2026-10-04".parse().unwrap(),
                    )
                    .is_err()
            );
        }
    }
}
