//! `supplier_offering` 与不可变商业条款修订。
//!
//! 供给直接连接公司 SKU 与供应商。供应商侧订货编码属于供给身份，不再通过
//! 供应商商品/SKU 主档或映射间接取得。价格、税率、起订量、区域和有效期属于
//! 不可变修订；实时库存与可供状态由 `supplier_offering_availability` 承担。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::{revision::RevisionBase, stable::StableBase, time::BusinessDate};
use crate::errors::{Error, Result};
use crate::ids::{
    SkuId, SupplierAccountId, SupplierApiConnectionId, SupplierCapabilityRevisionId,
    SupplierCommercialProfileRevisionId, SupplierOfferingId, SupplierOfferingRevisionId,
};
use crate::money::{round_to_cent, Amount, Quantity, Rate, UnitPrice};
use crate::supplier_offering::{OfferingRevisionImpact, OfferingSourceType, OfferingStatus};
use crate::validation::{normalize_optional_text, normalize_required_text};

const SUPPLIER_CODE_MAX_LEN: usize = 128;
const DROPSHIP_EXPRESS_MAX_LEN: usize = 512;
const TIMEZONE_MAX_LEN: usize = 64;
const CALENDAR_VERSION_MAX_LEN: usize = 64;
const MAX_REGIONS: usize = 50;
const MAX_CAPABILITIES: usize = 50;
const REGION_MAX_LEN: usize = 128;
const CAPABILITY_MAX_LEN: usize = 64;

/// 供给稳定身份创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierOfferingData {
    /// 公司 SKU。
    pub sku_id: SkuId,
    /// 供应商。
    pub supplier_id: SupplierAccountId,
    /// 供应商侧商品编码；没有 SPU 编码时可为空。
    pub supplier_product_code: Option<String>,
    /// 供应商侧订货 SKU 编码。
    pub supplier_sku_code: String,
    /// 登记来源。
    pub source_type: OfferingSourceType,
    /// API 来源连接；只有 API 来源允许填写。
    pub source_connection_id: Option<SupplierApiConnectionId>,
}

/// 公司 SKU 的供应商供给稳定身份。
#[derive(Debug, Serialize, Deserialize, Clone, Entity)]
pub struct SupplierOffering {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub stable: StableBase<OfferingStatus>,
    /// 公司 SKU。
    pub sku_id: SkuId,
    /// 供应商。
    pub supplier_id: SupplierAccountId,
    /// 供应商侧商品编码。
    pub supplier_product_code: Option<String>,
    /// 供应商侧订货 SKU 编码。
    pub supplier_sku_code: String,
    /// 登记来源。
    pub source_type: OfferingSourceType,
    /// API 来源连接。
    pub source_connection_id: Option<SupplierApiConnectionId>,
}

impl PartialEq for SupplierOffering {
    fn eq(&self, other: &Self) -> bool {
        self.base == other.base
            && self.stable.status == other.stable.status
            && self.stable.current_revision_id == other.stable.current_revision_id
            && self.stable.created_by == other.stable.created_by
            && self.stable.updated_by == other.stable.updated_by
            && self.sku_id == other.sku_id
            && self.supplier_id == other.supplier_id
            && self.supplier_product_code == other.supplier_product_code
            && self.supplier_sku_code == other.supplier_sku_code
            && self.source_type == other.source_type
            && self.source_connection_id == other.source_connection_id
    }
}

impl Eq for SupplierOffering {}

impl SupplierOffering {
    /// 创建供给稳定身份并规范化供应商侧编码。
    ///
    /// # 参数
    /// * `id` - 供给主键
    /// * `data` - 供给身份数据
    /// * `created_by` - 创建人
    ///
    /// # 返回
    /// 返回初始为启用状态的供给。
    ///
    /// # 错误
    /// SKU 编码为空/超长，或非 API 来源携带连接时返回错误。
    pub fn new(
        id: SupplierOfferingId,
        data: SupplierOfferingData,
        created_by: impl Into<String>,
    ) -> Result<Self> {
        let supplier_sku_code = normalize_required_text(
            data.supplier_sku_code,
            "供应商 SKU 编码不能为空",
            SUPPLIER_CODE_MAX_LEN,
            "供应商 SKU 编码过长",
        )?;
        let supplier_product_code = normalize_optional_text(
            data.supplier_product_code,
            "供应商商品编码",
            SUPPLIER_CODE_MAX_LEN,
        )?;
        ensure_source_connection(data.source_type, data.source_connection_id.is_some())?;
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            stable: StableBase::new(OfferingStatus::Active, created_by),
            sku_id: data.sku_id,
            supplier_id: data.supplier_id,
            supplier_product_code,
            supplier_sku_code,
            source_type: data.source_type,
            source_connection_id: data.source_connection_id,
        })
    }

    /// 更新供给关系状态。
    ///
    /// # 参数
    /// * `status` - 新状态
    /// * `updated_by` - 操作人
    ///
    /// # 返回
    /// 状态更新成功返回 `Ok(())`。
    pub fn update_status(&mut self, status: OfferingStatus, updated_by: impl Into<String>) -> Result<()> {
        self.stable.status = status;
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 校验供给可用于指定供应商与供应商连接下单。
    ///
    /// API 来源供给必须使用登记时的连接；手工和 Excel 来源不绑定连接，但供应商
    /// 必须始终一致。
    ///
    /// # 参数
    /// * `supplier_id` - 下单供应商
    /// * `connection_id` - 本次下单连接
    ///
    /// # 返回
    /// 关系一致时返回 `true`。
    pub fn belongs_to_ordering_source(
        &self,
        supplier_id: &SupplierAccountId,
        connection_id: &SupplierApiConnectionId,
    ) -> bool {
        self.supplier_id == *supplier_id
            && self
                .source_connection_id
                .as_ref()
                .is_none_or(|source_connection_id| source_connection_id == connection_id)
    }

    /// 校验当前修订号并计算下一修订号。
    ///
    /// # 参数
    /// * `current_revision_no` - 仓储读取到的当前最大修订号
    /// * `expected_revision_no` - 调用方持有的期望修订号
    ///
    /// # 返回
    /// 版本一致时返回下一修订号。
    ///
    /// # 错误
    /// 修订号不一致或已达到 `u32` 上限时返回领域错误。
    pub fn next_revision_no(&self, current_revision_no: u32, expected_revision_no: u32) -> Result<u32> {
        if current_revision_no != expected_revision_no {
            return Err(Error::from("供给修订号不一致"));
        }
        current_revision_no
            .checked_add(1)
            .ok_or_else(|| Error::from("供给修订号已达到上限"))
    }

    /// 返回下一次成功持久化后的实体版本。
    ///
    /// # 返回
    /// 返回当前乐观锁版本加一。
    ///
    /// # 错误
    /// 当前版本已达到 `u64` 上限时返回领域错误。
    pub fn next_persisted_version(&self) -> Result<u64> {
        self.base
            .version
            .checked_add(1)
            .ok_or_else(|| Error::from("供给版本已达到上限"))
    }
}

/// 供给修订的预填依据。
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PrefillSourceRefs {
    /// 税率预填来源。
    pub input_tax_rate: Option<SupplierCommercialProfileRevisionId>,
    /// 区域预填来源。
    pub supply_region: Option<SupplierCapabilityRevisionId>,
    /// 生效日期预填值。
    pub valid_from_date: Option<BusinessDate>,
    /// 生效日期预填时区。
    pub valid_from_timezone: Option<String>,
    /// 生效日期预填日历版本。
    pub valid_from_calendar_version: Option<String>,
}

/// 供给商业条款修订创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierOfferingRevisionData {
    /// 所属供给。
    pub supplier_offering_id: SupplierOfferingId,
    /// 修订号。
    pub revision_no: u32,
    /// 一件代发含税价。
    pub dropship_supply_price_gross: UnitPrice,
    /// 一件代发不含税价。
    pub dropship_supply_price_net: UnitPrice,
    /// 集采含税价。
    pub bulk_supply_price_gross: UnitPrice,
    /// 集采不含税价。
    pub bulk_supply_price_net: UnitPrice,
    /// 进项税率。
    pub input_tax_rate: Rate,
    /// 一件代发快递说明。
    pub dropship_express: Option<String>,
    /// 运费。
    pub freight_amount: Option<Amount>,
    /// 服务费。
    pub service_fee_amount: Option<Amount>,
    /// 集采起订量。
    pub bulk_minimum_order_quantity: Quantity,
    /// 可供区域。
    pub supply_region: Vec<String>,
    /// 商品级能力。
    pub product_capabilities: Vec<String>,
    /// 生效日期。
    pub valid_from: BusinessDate,
    /// 失效日期。
    pub valid_to: Option<BusinessDate>,
    /// 预填依据。
    pub prefill_source_refs: PrefillSourceRefs,
}

impl SupplierOfferingRevisionData {
    /// 由含税价格和税率构造完整商业条款数据。
    ///
    /// 不含税价格统一按合同规则从含税价格扣除按分舍入的税额，调用方不得重复
    /// 实现价格换算。
    ///
    /// # 参数
    /// * `supplier_offering_id` - 所属供给
    /// * `revision_no` - 修订号
    /// * `dropship_supply_price_gross` - 一件代发含税价
    /// * `bulk_supply_price_gross` - 集采含税价
    /// * `input_tax_rate` - 进项税率
    /// * `dropship_express` - 一件代发快递说明
    /// * `freight_amount` - 运费
    /// * `service_fee_amount` - 服务费
    /// * `bulk_minimum_order_quantity` - 集采起订量
    /// * `supply_region` - 可供区域
    /// * `product_capabilities` - 商品级能力
    /// * `valid_from` - 生效日期
    /// * `valid_to` - 失效日期
    /// * `prefill_source_refs` - 预填依据
    ///
    /// # 返回
    /// 返回已派生两组不含税价格的商业条款数据。
    #[allow(clippy::too_many_arguments)]
    pub fn from_gross_prices(
        supplier_offering_id: SupplierOfferingId,
        revision_no: u32,
        dropship_supply_price_gross: UnitPrice,
        bulk_supply_price_gross: UnitPrice,
        input_tax_rate: Rate,
        dropship_express: Option<String>,
        freight_amount: Option<Amount>,
        service_fee_amount: Option<Amount>,
        bulk_minimum_order_quantity: Quantity,
        supply_region: Vec<String>,
        product_capabilities: Vec<String>,
        valid_from: BusinessDate,
        valid_to: Option<BusinessDate>,
        prefill_source_refs: PrefillSourceRefs,
    ) -> Self {
        Self {
            supplier_offering_id,
            revision_no,
            dropship_supply_price_gross,
            dropship_supply_price_net: net_price(dropship_supply_price_gross, input_tax_rate),
            bulk_supply_price_gross,
            bulk_supply_price_net: net_price(bulk_supply_price_gross, input_tax_rate),
            input_tax_rate,
            dropship_express,
            freight_amount,
            service_fee_amount,
            bulk_minimum_order_quantity,
            supply_region,
            product_capabilities,
            valid_from,
            valid_to,
            prefill_source_refs,
        }
    }
}

/// 不可变供给商业条款修订。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct SupplierOfferingRevision {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub revision: RevisionBase,
    /// 所属供给。
    pub supplier_offering_id: SupplierOfferingId,
    /// 一件代发含税价。
    pub dropship_supply_price_gross: UnitPrice,
    /// 一件代发不含税价。
    pub dropship_supply_price_net: UnitPrice,
    /// 集采含税价。
    pub bulk_supply_price_gross: UnitPrice,
    /// 集采不含税价。
    pub bulk_supply_price_net: UnitPrice,
    /// 进项税率。
    pub input_tax_rate: Rate,
    /// 一件代发快递说明。
    pub dropship_express: Option<String>,
    /// 运费。
    pub freight_amount: Option<Amount>,
    /// 服务费。
    pub service_fee_amount: Option<Amount>,
    /// 集采起订量。
    pub bulk_minimum_order_quantity: Quantity,
    /// 可供区域。
    pub supply_region: Vec<String>,
    /// 商品级能力。
    pub product_capabilities: Vec<String>,
    /// 生效日期。
    pub valid_from: BusinessDate,
    /// 失效日期。
    pub valid_to: Option<BusinessDate>,
    /// 预填依据。
    pub prefill_source_refs: PrefillSourceRefs,
}

impl SupplierOfferingRevision {
    /// 创建并校验不可变商业条款修订。
    ///
    /// # 参数
    /// * `id` - 修订主键
    /// * `data` - 商业条款
    ///
    /// # 返回
    /// 返回规范化后的修订。
    ///
    /// # 错误
    /// 价格换算、起订量、区域、费用或有效期不合法时返回错误。
    pub fn new(id: SupplierOfferingRevisionId, data: SupplierOfferingRevisionData) -> Result<Self> {
        ensure_revision_no(data.revision_no)?;
        ensure_price_pairs(&data)?;
        ensure_supply_conditions(&data)?;
        let dropship_express =
            normalize_optional_text(data.dropship_express, "快递说明", DROPSHIP_EXPRESS_MAX_LEN)?;
        let supply_region =
            normalize_required_list(data.supply_region, REGION_MAX_LEN, MAX_REGIONS, "供给区域")?;
        let product_capabilities = normalize_list(
            data.product_capabilities,
            CAPABILITY_MAX_LEN,
            MAX_CAPABILITIES,
            "商品能力",
        )?;
        let prefill_source_refs = normalize_prefill_refs(data.prefill_source_refs)?;
        ensure_validity_window(data.valid_from, data.valid_to)?;
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            revision: RevisionBase::new(data.revision_no),
            supplier_offering_id: data.supplier_offering_id,
            dropship_supply_price_gross: data.dropship_supply_price_gross,
            dropship_supply_price_net: data.dropship_supply_price_net,
            bulk_supply_price_gross: data.bulk_supply_price_gross,
            bulk_supply_price_net: data.bulk_supply_price_net,
            input_tax_rate: data.input_tax_rate,
            dropship_express,
            freight_amount: data.freight_amount,
            service_fee_amount: data.service_fee_amount,
            bulk_minimum_order_quantity: data.bulk_minimum_order_quantity,
            supply_region,
            product_capabilities,
            valid_from: data.valid_from,
            valid_to: data.valid_to,
            prefill_source_refs,
        })
    }

    /// 比较上一版商业条款并分类销售安全影响。
    ///
    /// 关键供给条件优先于成本变化；同一修订同时改变关键条件和成本时返回
    /// [`OfferingRevisionImpact::CriticalSupplyChanged`]。
    ///
    /// # 参数
    /// * `prior` - 上一版商业条款
    ///
    /// # 返回
    /// 返回无影响、成本变化或关键供给变化。
    pub fn impact_from(&self, prior: &Self) -> OfferingRevisionImpact {
        let critical_changed = prior.bulk_minimum_order_quantity != self.bulk_minimum_order_quantity
            || prior.supply_region != self.supply_region
            || prior.product_capabilities != self.product_capabilities
            || prior.dropship_express != self.dropship_express
            || prior.valid_from != self.valid_from
            || prior.valid_to != self.valid_to;
        if critical_changed {
            return OfferingRevisionImpact::CriticalSupplyChanged;
        }
        let cost_changed = prior.dropship_supply_price_gross != self.dropship_supply_price_gross
            || prior.dropship_supply_price_net != self.dropship_supply_price_net
            || prior.bulk_supply_price_gross != self.bulk_supply_price_gross
            || prior.bulk_supply_price_net != self.bulk_supply_price_net
            || prior.input_tax_rate != self.input_tax_rate
            || prior.freight_amount != self.freight_amount
            || prior.service_fee_amount != self.service_fee_amount;
        if cost_changed {
            OfferingRevisionImpact::CostChanged
        } else {
            OfferingRevisionImpact::None
        }
    }
}

/// 按合同规则由含税价派生不含税价。
fn net_price(gross: UnitPrice, rate: Rate) -> UnitPrice {
    UnitPrice::try_from(gross.to_decimal() - round_to_cent(gross.to_decimal() * rate.to_decimal()))
        .expect("合法含税价与税率必须生成合法不含税价")
}

fn ensure_source_connection(source_type: OfferingSourceType, has_connection: bool) -> Result<()> {
    if source_type != OfferingSourceType::Api && has_connection {
        return Err(Error::from("只有 API 来源可以关联供应商 API 连接"));
    }
    if source_type == OfferingSourceType::Api && !has_connection {
        return Err(Error::from("API 来源必须关联供应商 API 连接"));
    }
    Ok(())
}

fn ensure_revision_no(revision_no: u32) -> Result<()> {
    if revision_no == 0 {
        return Err(Error::from("修订号必须从 1 开始"));
    }
    Ok(())
}

fn ensure_price_pairs(data: &SupplierOfferingRevisionData) -> Result<()> {
    ensure_price_pair(
        data.dropship_supply_price_gross,
        data.dropship_supply_price_net,
        data.input_tax_rate,
        "一件代发",
    )?;
    ensure_price_pair(
        data.bulk_supply_price_gross,
        data.bulk_supply_price_net,
        data.input_tax_rate,
        "集采",
    )
}

fn ensure_price_pair(gross: UnitPrice, net: UnitPrice, rate: Rate, label: &str) -> Result<()> {
    if gross.to_decimal().is_sign_negative() || net.to_decimal().is_sign_negative() {
        return Err(Error::from(format!("{label}供给价不能为负")));
    }
    let expected_net = gross.to_decimal() - round_to_cent(gross.to_decimal() * rate.to_decimal());
    if expected_net != net.to_decimal() {
        return Err(Error::from(format!("{label}供给价含税/不含税换算不一致")));
    }
    Ok(())
}

fn ensure_supply_conditions(data: &SupplierOfferingRevisionData) -> Result<()> {
    if data.bulk_minimum_order_quantity.to_decimal() <= rust_decimal::Decimal::ZERO {
        return Err(Error::from("集采起订量必须为正"));
    }
    if data
        .freight_amount
        .is_some_and(|value| value.to_decimal().is_sign_negative())
    {
        return Err(Error::from("运费不能为负"));
    }
    if data
        .service_fee_amount
        .is_some_and(|value| value.to_decimal().is_sign_negative())
    {
        return Err(Error::from("服务费不能为负"));
    }
    Ok(())
}

fn ensure_validity_window(valid_from: BusinessDate, valid_to: Option<BusinessDate>) -> Result<()> {
    if valid_to.is_some_and(|value| value <= valid_from) {
        return Err(Error::from("有效期结束必须晚于开始"));
    }
    Ok(())
}

fn normalize_required_list(
    values: Vec<String>,
    item_max_len: usize,
    max_count: usize,
    label: &str,
) -> Result<Vec<String>> {
    let values = normalize_list(values, item_max_len, max_count, label)?;
    if values.is_empty() {
        return Err(Error::from(format!("{label}不能为空")));
    }
    Ok(values)
}

fn normalize_list(
    values: Vec<String>,
    item_max_len: usize,
    max_count: usize,
    label: &str,
) -> Result<Vec<String>> {
    if values.len() > max_count {
        return Err(Error::from(format!("{label}最多 {max_count} 项")));
    }
    let mut normalized = Vec::with_capacity(values.len());
    for value in values {
        if let Some(value) = normalize_optional_text(Some(value), label, item_max_len)? {
            if !normalized.contains(&value) {
                normalized.push(value);
            }
        }
    }
    Ok(normalized)
}

fn normalize_prefill_refs(refs: PrefillSourceRefs) -> Result<PrefillSourceRefs> {
    let timezone = normalize_optional_text(refs.valid_from_timezone, "预填时区", TIMEZONE_MAX_LEN)?;
    let calendar_version = normalize_optional_text(
        refs.valid_from_calendar_version,
        "预填日历版本",
        CALENDAR_VERSION_MAX_LEN,
    )?;
    if (timezone.is_some() || calendar_version.is_some()) && refs.valid_from_date.is_none() {
        return Err(Error::from("预填时区/日历版本必须与预填业务日期同时提供"));
    }
    Ok(PrefillSourceRefs {
        input_tax_rate: refs.input_tax_rate,
        supply_region: refs.supply_region,
        valid_from_date: refs.valid_from_date,
        valid_from_timezone: timezone,
        valid_from_calendar_version: calendar_version,
    })
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::{
        PrefillSourceRefs, SupplierOffering, SupplierOfferingData, SupplierOfferingRevision,
        SupplierOfferingRevisionData,
    };
    use crate::common::time::BusinessDate;
    use crate::ids::{
        SkuId, SupplierAccountId, SupplierApiConnectionId, SupplierOfferingId, SupplierOfferingRevisionId,
    };
    use crate::money::{Amount, Quantity, Rate, UnitPrice};
    use crate::supplier_offering::{OfferingRevisionImpact, OfferingSourceType};

    fn offering_data() -> SupplierOfferingData {
        SupplierOfferingData {
            sku_id: SkuId::new("sku-1"),
            supplier_id: SupplierAccountId::new("supplier-1"),
            supplier_product_code: Some(" SPU-1 ".to_string()),
            supplier_sku_code: " SKU-1 ".to_string(),
            source_type: OfferingSourceType::Manual,
            source_connection_id: None,
        }
    }

    fn revision_data() -> SupplierOfferingRevisionData {
        SupplierOfferingRevisionData {
            supplier_offering_id: SupplierOfferingId::new("offering-1"),
            revision_no: 1,
            dropship_supply_price_gross: UnitPrice::from_str("11.30").unwrap(),
            dropship_supply_price_net: UnitPrice::from_str("9.83").unwrap(),
            bulk_supply_price_gross: UnitPrice::from_str("9.04").unwrap(),
            bulk_supply_price_net: UnitPrice::from_str("7.86").unwrap(),
            input_tax_rate: Rate::from_str("0.13").unwrap(),
            dropship_express: Some(" 顺丰 ".to_string()),
            freight_amount: Some(Amount::from_str("1").unwrap()),
            service_fee_amount: None,
            bulk_minimum_order_quantity: Quantity::from_str("10").unwrap(),
            supply_region: vec![" 全国 ".to_string()],
            product_capabilities: vec!["REFUND".to_string()],
            valid_from: BusinessDate::from_str("2026-08-08").unwrap(),
            valid_to: None,
            prefill_source_refs: PrefillSourceRefs::default(),
        }
    }

    #[test]
    fn offering_owns_supplier_ordering_identity() {
        let offering =
            SupplierOffering::new(SupplierOfferingId::new("offering-1"), offering_data(), "admin-1").unwrap();
        assert_eq!(offering.supplier_sku_code, "SKU-1");
        assert_eq!(offering.supplier_product_code.as_deref(), Some("SPU-1"));

        let invalid = SupplierOfferingData {
            source_type: OfferingSourceType::Api,
            source_connection_id: None,
            ..offering_data()
        };
        assert!(SupplierOffering::new(SupplierOfferingId::new("offering-2"), invalid, "admin").is_err());

        let valid_api = SupplierOfferingData {
            source_type: OfferingSourceType::Api,
            source_connection_id: Some(SupplierApiConnectionId::new("connection-1")),
            ..offering_data()
        };
        let offering =
            SupplierOffering::new(SupplierOfferingId::new("offering-3"), valid_api, "admin").unwrap();
        assert!(offering.belongs_to_ordering_source(
            &SupplierAccountId::new("supplier-1"),
            &SupplierApiConnectionId::new("connection-1")
        ));
        assert!(!offering.belongs_to_ordering_source(
            &SupplierAccountId::new("supplier-2"),
            &SupplierApiConnectionId::new("connection-1")
        ));
        assert_eq!(offering.next_revision_no(1, 1).unwrap(), 2);
        assert!(offering.next_revision_no(2, 1).is_err());
        assert_eq!(
            offering.next_persisted_version().unwrap(),
            offering.base.version + 1
        );
    }

    #[test]
    fn revision_contains_terms_but_not_availability() {
        let revision =
            SupplierOfferingRevision::new(SupplierOfferingRevisionId::new("revision-1"), revision_data())
                .unwrap();
        assert_eq!(revision.revision.revision_no, 1);
        assert_eq!(revision.supply_region, vec!["全国"]);

        let zero_moq = SupplierOfferingRevisionData {
            bulk_minimum_order_quantity: Quantity::from_str("0").unwrap(),
            ..revision_data()
        };
        assert!(
            SupplierOfferingRevision::new(SupplierOfferingRevisionId::new("revision-2"), zero_moq).is_err()
        );
    }

    #[test]
    fn gross_price_factory_and_revision_impact_are_domain_rules() {
        let data = SupplierOfferingRevisionData::from_gross_prices(
            SupplierOfferingId::new("offering-1"),
            1,
            UnitPrice::from_str("11.30").unwrap(),
            UnitPrice::from_str("9.04").unwrap(),
            Rate::from_str("0.13").unwrap(),
            None,
            None,
            None,
            Quantity::from_str("10").unwrap(),
            vec!["CN".to_string()],
            vec!["REFUND".to_string()],
            BusinessDate::from_str("2026-08-08").unwrap(),
            None,
            PrefillSourceRefs::default(),
        );
        assert_eq!(data.dropship_supply_price_net.to_string(), "9.83");
        let prior =
            SupplierOfferingRevision::new(SupplierOfferingRevisionId::new("revision-1"), data.clone())
                .unwrap();

        let cost = SupplierOfferingRevision::new(
            SupplierOfferingRevisionId::new("revision-2"),
            SupplierOfferingRevisionData::from_gross_prices(
                SupplierOfferingId::new("offering-1"),
                2,
                UnitPrice::from_str("12.00").unwrap(),
                data.bulk_supply_price_gross,
                data.input_tax_rate,
                None,
                None,
                None,
                data.bulk_minimum_order_quantity,
                data.supply_region.clone(),
                data.product_capabilities.clone(),
                data.valid_from,
                data.valid_to,
                PrefillSourceRefs::default(),
            ),
        )
        .unwrap();
        assert_eq!(cost.impact_from(&prior), OfferingRevisionImpact::CostChanged);

        let mut critical_data = data;
        critical_data.revision_no = 2;
        critical_data.supply_region.push("HK".to_string());
        let critical =
            SupplierOfferingRevision::new(SupplierOfferingRevisionId::new("revision-3"), critical_data)
                .unwrap();
        assert_eq!(
            critical.impact_from(&prior),
            OfferingRevisionImpact::CriticalSupplyChanged
        );
    }
}
