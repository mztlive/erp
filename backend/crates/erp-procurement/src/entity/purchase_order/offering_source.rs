//! 采购行的正式选源事实；历史缺失保持未知，不能由供应商及SKU推断。

use std::collections::HashMap;

use erp_core::common::time::BusinessDate;
use erp_core::ids::{
    SkuId, SkuRevisionId, SupplierAccountId, SupplierOfferingId, SupplierOfferingRevisionId,
};
use erp_core::money::Quantity;
use erp_core::{Error, Result};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use super::{
    LineSupply, PurchaseLineInput, PurchaseLineType, PurchaseOrderRevisionLine, PurchaseOrderSubmissionLine,
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
}

impl PurchaseOfferingSource {
    /// 从当前事务验证的精确采购依据冻结正式选源。
    ///
    /// # 参数
    /// * `supply` - 采购创建实际选用的供给、条款和可供事实。
    /// # 返回
    /// 返回不可由客户端改写的完整选源值。
    /// # 错误
    /// 条款或可供投影不属于当前供给，或身份、版本缺失时拒绝创建。
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
        };
        source.ensure_valid()?;
        Ok(source)
    }

    /// 核对提交事务读取的精确供给仍与采购冻结来源一致且可用。
    ///
    /// # 参数
    /// * `supply` - 提供方筛选为启用状态的当前供给、条款和可供事实。
    /// * `supplier_id` - 本采购冻结的供应商身份。
    /// * `sku_id` - 本采购行冻结的 SKU 身份。
    /// * `quantity` - 本采购行本次提交的基础单位数量。
    /// * `on_date` - 提交时的业务日期。
    /// # 返回
    /// 原身份、当前指针及三种版本相同，条款有效且数量可供时返回成功。
    /// # 错误
    /// 来源、归属、版本、有效期或可供资格变化时拒绝；不替换冻结价格和版本。
    pub fn ensure_current_supply(
        &self,
        supply: &LineSupply,
        supplier_id: &SupplierAccountId,
        sku_id: &SkuId,
        quantity: Quantity,
        on_date: BusinessDate,
    ) -> Result<()> {
        self.ensure_valid()?;
        if Self::from_supply(supply)? != *self
            || supply.offering.supplier_id != *supplier_id
            || supply.offering.sku_id != *sku_id
        {
            return Err(Error::from("采购供给选源或版本已变化，请重新核对"));
        }
        if supply.revision.valid_from > on_date
            || supply.revision.valid_to.is_some_and(|until| until < on_date)
            || supply.availability.availability_status != AvailabilityStatus::Available
            || quantity.to_decimal() <= Decimal::ZERO
            || supply.availability.available_quantity.is_some_and(|available| available < quantity)
        {
            return Err(Error::from("采购供给条款已失效或当前数量不可供，请重新核对"));
        }
        Ok(())
    }

    /// 校验来源身份及三种版本均可追溯。
    fn ensure_valid(&self) -> Result<()> {
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

/// 行归属只允许商品行持有完整选源，未知历史行保留空值。
///
/// # 参数
/// * `kind` - 采购行类型。
/// * `source` - 已冻结的供给选源；`None` 表示历史行尚未记录选源。
///
/// # 返回
/// 未记录选源，或商品行选源的身份与三种版本都可追溯时返回 `Ok(())`。
///
/// # 错误
/// 物流费用行携带选源，或选源身份为空白、任一版本为 0 时返回领域错误。
pub(super) fn ensure_offering_source(
    kind: PurchaseLineType,
    source: Option<&PurchaseOfferingSource>,
) -> Result<()> {
    if let Some(source) = source {
        if kind != PurchaseLineType::ItemService {
            return Err(Error::from("物流费用行不得携带供给选源事实"));
        }
        source.ensure_valid()?;
    }
    Ok(())
}

/// 从真实旧草稿行继承选源，客户端编辑只影响已允许的采购内容。
///
/// # 参数
/// * `inputs` - 已验证原销售、SKU来源不变的请求行。
/// * `existing` - 当前服务端草稿或冻结提交的完整行。
/// # 返回
/// 原地补齐对应原行的选源；旧行未知仍为None。
/// # 错误
/// 来源行缺失、SKU版本变化或原行身份歧义时拒绝。
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
            kind: line.line_type,
        }),
    )
}

/// 采购变更从明确基准行复制原选源，不重新猜测或切换供给。
///
/// # 参数
/// * `inputs` - 已规范化销售关联的采购变更目标行。
/// * `existing` - 本次变更基准版本的完整正式行。
/// # 返回
/// 原地继承精确供给选源，冻结价格等原有规则由调用方执行。
/// # 错误
/// 来源不匹配或歧义时拒绝。
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
    kind: PurchaseLineType,
}

/// 对应原行必须唯一；没有原选源的历史数据不补造来源。
fn inherit_sources<'a>(
    inputs: &mut [PurchaseLineInput],
    existing: impl Iterator<Item = SourceBinding<'a>>,
) -> Result<()> {
    let mut sources = HashMap::new();
    for line in existing.filter(|line| line.kind == PurchaseLineType::ItemService) {
        let key = source_key(line.stable_line, line.sku, line.revision)?;
        if sources.insert(key, line.source.cloned()).is_some() {
            return Err(Error::from("采购原行身份歧义，不能继承正式选源"));
        }
    }
    for input in inputs {
        input.supplier_offering_source = if input.line_type == PurchaseLineType::LogisticsFee {
            None
        } else {
            let key = source_key(
                input.sales_order_line_id.as_ref().map(|id| id.as_ref()),
                input.sku_id.as_ref(),
                input.sku_revision_id.as_ref(),
            )?;
            sources
                .get(&key)
                .ok_or_else(|| Error::from("采购来源行或SKU版本已变化，不能继承正式选源"))?
                .clone()
        };
    }
    Ok(())
}

/// 构造原行精确身份，不使用供应商和SKU作为供给关联。
fn source_key(
    stable: Option<&str>,
    sku: Option<&SkuId>,
    revision: Option<&SkuRevisionId>,
) -> Result<(String, String, String)> {
    Ok((
        stable.ok_or("采购来源缺少销售稳定行")?.into(),
        sku.ok_or("采购来源缺少SKU")?.to_string(),
        revision.ok_or("采购来源缺少SKU版本")?.to_string(),
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
                bulk_supply_price_gross: "10".parse().unwrap(),
                dropship_supply_price_gross: "11".parse().unwrap(),
                input_tax_rate: "0.13".parse().unwrap(),
            },
            availability: AvailabilityFact {
                base: VersionedFactIdentity { id: "availability".into(), version: 11 },
                supplier_offering_id: SupplierOfferingId::new("selected"),
                availability_status: AvailabilityStatus::Available,
                available_quantity: None,
                source_revision_token: None,
            },
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
        assert!(ensure_offering_source(PurchaseLineType::LogisticsFee, Some(&source)).is_err());
        assert!(ensure_offering_source(PurchaseLineType::ItemService, None).is_ok());
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
        let mut value = original.clone();
        value.revision.valid_to = Some("2026-10-03".parse().unwrap());
        changed.push(value);
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
