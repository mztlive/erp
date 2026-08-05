//! `supplier_offering` / `supplier_offering_revision`（数据模型 §6.14）。
//!
//! 供给关系是「公司 SKU ↔ 供应商 SKU」的稳定供给关系：稳定表只保存身份、状态和
//! 当前修订指针；其余价格和条件都属于不可变修订。每条有效供给修订必须同时具备
//! 一件代发供给价和集采供给价（双价不得互相覆盖或择一折叠）；`input_tax_rate`、
//! `supply_region`、`valid_from` 必须显式进入修订，缺失即 fail-closed。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::revision::RevisionBase;
use crate::common::stable::StableBase;
use crate::common::time::BusinessDate;
use crate::errors::{Error, Result};
use crate::ids::{
    SkuId, SupplierAccountId, SupplierCapabilityRevisionId, SupplierCatalogSkuId,
    SupplierCommercialProfileRevisionId, SupplierOfferingId, SupplierOfferingRevisionId,
};
use crate::money::{round_to_cent, Amount, Quantity, Rate, UnitPrice};
use crate::supplier_catalog::sku::AvailabilityStatus;
use crate::validation::normalize_optional_text;

/// 快递说明最大长度。
const DROPSHIP_EXPRESS_MAX_LEN: usize = 512;
/// 时区最大长度。
const TIMEZONE_MAX_LEN: usize = 64;
/// 日历版本最大长度。
const CALENDAR_VERSION_MAX_LEN: usize = 64;
/// 供给区域最大条数。
const MAX_REGIONS: usize = 50;
/// 商品能力最大条数。
const MAX_CAPABILITIES: usize = 50;
/// 区域/能力条目最大长度。
const REGION_MAX_LEN: usize = 128;
const CAPABILITY_MAX_LEN: usize = 64;

/// 供给状态（§6.14：启用、暂停、停止）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum OfferingStatus {
    /// 启用。
    Active,
    /// 暂停。
    Paused,
    /// 停止。
    Stopped,
}

impl OfferingStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Active => "启用",
            Self::Paused => "暂停",
            Self::Stopped => "停止",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "ACTIVE",
            Self::Paused => "PAUSED",
            Self::Stopped => "STOPPED",
        }
    }
}

/// 供给修订的预填依据（§6.14 `prefill_source_refs`）。
///
/// 按字段保存可选的结构化预填引用，只记录预填依据，不替代采购最终确认值；
/// 某字段发生自动预填时对应引用必填，未发生预填时不得伪造引用（P3 填充）。
/// 税率来源只能是供应商商业资料修订或税务策略修订，区域来源只能是供应商能力
/// 修订或供给区域策略修订；税务/供给区域策略修订类型未在 P0 定义，留 P3。
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PrefillSourceRefs {
    /// 税率预填来源（供应商商业资料修订）。
    pub input_tax_rate: Option<SupplierCommercialProfileRevisionId>,
    /// 区域预填来源（供应商能力修订）。
    pub supply_region: Option<SupplierCapabilityRevisionId>,
    /// 生效日期预填来源（服务端业务日期快照）。
    pub valid_from_date: Option<BusinessDate>,
    /// 生效日期预填来源时区。
    pub valid_from_timezone: Option<String>,
    /// 生效日期预填来源可选日历版本。
    pub valid_from_calendar_version: Option<String>,
}

impl PrefillSourceRefs {
    /// 校验预填依据一致性。
    ///
    /// 时区/日历版本只有在携带服务端业务日期时才有意义。
    ///
    /// # 参数
    /// * `timezone` - 预填时区（可空）
    /// * `calendar_version` - 预填日历版本（可空）
    /// * `has_date` - 是否携带预填业务日期
    ///
    /// # 错误
    /// 有时区/日历版本但没有业务日期时返回错误。
    fn ensure_valid(
        timezone: Option<String>,
        calendar_version: Option<String>,
        has_date: bool,
    ) -> Result<()> {
        if (timezone.is_some() || calendar_version.is_some()) && !has_date {
            return Err(Error::from("预填时区/日历版本必须与预填业务日期同时提供"));
        }
        Ok(())
    }
}

/// 供给稳定身份创建数据（不含系统字段）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierOfferingData {
    /// ERP SKU。
    pub sku_id: SkuId,
    /// 供应商。
    pub supplier_id: SupplierAccountId,
    /// 供应商 SKU。
    pub supplier_catalog_sku_id: SupplierCatalogSkuId,
}

/// 供给稳定身份实体（数据模型 §6.14）。
///
/// `StableBase` 未派生 `PartialEq`，因此本实体手工实现全字段语义相等。
#[derive(Debug, Serialize, Deserialize, Clone, Entity)]
pub struct SupplierOffering {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub stable: StableBase<OfferingStatus>,
    /// ERP SKU。
    pub sku_id: SkuId,
    /// 供应商。
    pub supplier_id: SupplierAccountId,
    /// 供应商 SKU。
    pub supplier_catalog_sku_id: SupplierCatalogSkuId,
}

impl PartialEq for SupplierOffering {
    /// 全字段语义相等。
    fn eq(&self, other: &Self) -> bool {
        self.base == other.base
            && self.stable.status == other.stable.status
            && self.stable.current_revision_id == other.stable.current_revision_id
            && self.stable.created_by == other.stable.created_by
            && self.stable.updated_by == other.stable.updated_by
            && self.sku_id == other.sku_id
            && self.supplier_id == other.supplier_id
            && self.supplier_catalog_sku_id == other.supplier_catalog_sku_id
    }
}

impl Eq for SupplierOffering {}

impl SupplierOffering {
    /// 创建供给稳定身份。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SupplierOfferingId`）
    /// * `data` - 创建数据
    /// * `created_by` - 创建人（账号或系统身份）
    ///
    /// # 返回
    /// 返回新建的供给实体（初始状态 `Active`）。
    ///
    /// # 错误
    /// 无（ID 与身份字段均为类型化入参）；身份唯一性由唯一索引保证（P3）。
    pub fn new(
        id: SupplierOfferingId,
        data: SupplierOfferingData,
        created_by: impl Into<String>,
    ) -> Result<Self> {
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            stable: StableBase::new(OfferingStatus::Active, created_by),
            sku_id: data.sku_id,
            supplier_id: data.supplier_id,
            supplier_catalog_sku_id: data.supplier_catalog_sku_id,
        })
    }

    /// 更新供给启停状态（启用/暂停/停止）。
    ///
    /// 供货价变化形成新修订（§6.14），不在稳定表修改；
    /// 稳定身份字段（`sku_id`/`supplier_id`/`supplier_catalog_sku_id`）不可修改。
    ///
    /// # 参数
    /// * `status` - 新状态
    /// * `updated_by` - 本次更新执行人
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    pub fn update(&mut self, status: OfferingStatus, updated_by: impl Into<String>) -> Result<()> {
        self.stable.status = status;
        self.stable.touch(updated_by);
        Ok(())
    }
}

/// 供给修订创建数据（不含系统字段）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierOfferingRevisionData {
    /// 所属供给。
    pub supplier_offering_id: SupplierOfferingId,
    /// 修订号（同一供给内从 1 递增）。
    pub revision_no: u32,
    /// 采购确认的一件代发供给价（含税；已含包装、发货费用）。
    pub dropship_supply_price_gross: UnitPrice,
    /// 采购确认的一件代发供给价（不含税）。
    pub dropship_supply_price_net: UnitPrice,
    /// 采购确认的集采供给价（含税）。
    pub bulk_supply_price_gross: UnitPrice,
    /// 采购确认的集采供给价（不含税）。
    pub bulk_supply_price_net: UnitPrice,
    /// 两项供给价共同使用的进项税率。
    pub input_tax_rate: Rate,
    /// 一件代发快递说明（自由文本）。
    pub dropship_express: Option<String>,
    /// 费用。
    pub freight_amount: Option<Amount>,
    /// 服务费。
    pub service_fee_amount: Option<Amount>,
    /// 集采起订量（按 ERP 基本单位；一件代发固定从 1 件起，不另存起订量）。
    pub bulk_minimum_order_quantity: Quantity,
    /// 可供区域（fail-closed 必填）。
    pub supply_region: Vec<String>,
    /// 可供状态。
    pub availability_status: AvailabilityStatus,
    /// 可供数量。
    pub available_quantity: Option<Quantity>,
    /// 商品级能力（取消、退款、物流等）。
    pub product_capabilities: Vec<String>,
    /// 有效期开始（fail-closed 必填）。
    pub valid_from: BusinessDate,
    /// 有效期结束（必须晚于 `valid_from`）。
    pub valid_to: Option<BusinessDate>,
    /// 按字段保存的可选结构化预填依据。
    pub prefill_source_refs: PrefillSourceRefs,
}

/// 供给修订实体（不可变修订，数据模型 §6.14/§4.2）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct SupplierOfferingRevision {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub revision: RevisionBase,
    /// 所属供给。
    pub supplier_offering_id: SupplierOfferingId,
    /// 一件代发供给价（含税）。
    pub dropship_supply_price_gross: UnitPrice,
    /// 一件代发供给价（不含税）。
    pub dropship_supply_price_net: UnitPrice,
    /// 集采供给价（含税）。
    pub bulk_supply_price_gross: UnitPrice,
    /// 集采供给价（不含税）。
    pub bulk_supply_price_net: UnitPrice,
    /// 进项税率。
    pub input_tax_rate: Rate,
    /// 一件代发快递说明。
    pub dropship_express: Option<String>,
    /// 费用。
    pub freight_amount: Option<Amount>,
    /// 服务费。
    pub service_fee_amount: Option<Amount>,
    /// 集采起订量。
    pub bulk_minimum_order_quantity: Quantity,
    /// 可供区域。
    pub supply_region: Vec<String>,
    /// 可供状态。
    pub availability_status: AvailabilityStatus,
    /// 可供数量。
    pub available_quantity: Option<Quantity>,
    /// 商品级能力。
    pub product_capabilities: Vec<String>,
    /// 有效期开始。
    pub valid_from: BusinessDate,
    /// 有效期结束。
    pub valid_to: Option<BusinessDate>,
    /// 结构化预填依据。
    pub prefill_source_refs: PrefillSourceRefs,
}

impl SupplierOfferingRevision {
    /// 创建供给修订。
    ///
    /// 完成快递说明/区域/能力的校验与规范化，并强制以下不变式（§6.14/§4.2）：
    /// - 双价完整：一项代发与集采供给价必须同时具备含税与不含税两项；
    /// - 含税/不含税按统一定点规则换算：`net = gross − round_to_cent(gross × rate)`
    ///   （§4.2 铁律 4 的单价换算形态），禁止择一折叠或价格倒挂；
    /// - `bulk_minimum_order_quantity > 0`；费用/可供数量非负；
    /// - `supply_region` 非空（fail-closed）且条目规范化；
    /// - 有效期窗口合法；预填依据逐字段一致。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SupplierOfferingRevisionId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的供给修订实体。
    ///
    /// # 错误
    /// 修订号为零、价格换算不一致、起订量非正、区域为空、有效期倒挂或
    /// 预填依据不一致时返回错误。
    ///
    /// # 说明
    /// 同一供给的有效期不得重叠（§6.14）依赖按供给聚合查询，留 P3 校验。
    pub fn new(id: SupplierOfferingRevisionId, data: SupplierOfferingRevisionData) -> Result<Self> {
        ensure_revision_no(data.revision_no)?;
        ensure_price_pairs(&data)?;
        ensure_supply_conditions(&data)?;
        let texts = normalize_offering_texts(&data)?;
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
            dropship_express: texts.dropship_express,
            freight_amount: data.freight_amount,
            service_fee_amount: data.service_fee_amount,
            bulk_minimum_order_quantity: data.bulk_minimum_order_quantity,
            supply_region: texts.supply_region,
            availability_status: data.availability_status,
            available_quantity: data.available_quantity,
            product_capabilities: texts.product_capabilities,
            valid_from: data.valid_from,
            valid_to: data.valid_to,
            prefill_source_refs: texts.prefill_source_refs,
        })
    }
}

/// 校验修订号从 1 开始。
///
/// # 参数
/// * `revision_no` - 修订号
///
/// # 错误
/// 修订号为零时返回错误。
fn ensure_revision_no(revision_no: u32) -> Result<()> {
    if revision_no == 0 {
        return Err(Error::from("修订号必须从 1 开始"));
    }
    Ok(())
}

/// 校验一对供给价的含税/不含税换算一致性。
///
/// 换算规则（§4.2 铁律 4 的单价形态）：`net = gross − round_to_cent(gross × rate)`；
/// 含税价不得低于不含税价。
///
/// # 参数
/// * `gross` - 含税供给价
/// * `net` - 不含税供给价
/// * `rate` - 进项税率
/// * `label` - 价格组说明（一件代发/集采）
///
/// # 错误
/// 换算不一致或含税价低于不含税价时返回错误。
fn ensure_price_pair(gross: UnitPrice, net: UnitPrice, rate: Rate, label: &str) -> Result<()> {
    if gross.to_decimal() < rust_decimal::Decimal::ZERO || net.to_decimal() < rust_decimal::Decimal::ZERO {
        return Err(Error::from(format!("{label}供给价不能为负")));
    }
    let expected_tax = round_to_cent(gross.to_decimal() * rate.to_decimal());
    let expected_net = gross.to_decimal() - expected_tax;
    if expected_net != net.to_decimal() {
        return Err(Error::from(format!("{label}供给价含税/不含税换算不一致")));
    }
    Ok(())
}

/// 校验两项供给价双价完整且换算一致（§6.14 双价不得折叠/覆盖）。
///
/// # 参数
/// * `data` - 供给修订创建数据
///
/// # 错误
/// 任一价格组换算不一致或为负时返回错误。
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
    )?;
    Ok(())
}

/// 校验集采起订量与费用/可供数量数值（§6.14/§4.2）。
///
/// # 参数
/// * `data` - 供给修订创建数据
///
/// # 错误
/// 起订量非正，或任一费用/数量为负时返回错误。
fn ensure_supply_conditions(data: &SupplierOfferingRevisionData) -> Result<()> {
    if data.bulk_minimum_order_quantity.to_decimal() <= rust_decimal::Decimal::ZERO {
        return Err(Error::from("集采起订量必须为正"));
    }
    if let Some(freight) = data.freight_amount {
        if freight.to_decimal() < rust_decimal::Decimal::ZERO {
            return Err(Error::from("运费不能为负"));
        }
    }
    if let Some(service_fee) = data.service_fee_amount {
        if service_fee.to_decimal() < rust_decimal::Decimal::ZERO {
            return Err(Error::from("服务费不能为负"));
        }
    }
    if let Some(quantity) = data.available_quantity {
        if quantity.to_decimal() < rust_decimal::Decimal::ZERO {
            return Err(Error::from("可供数量不能为负"));
        }
    }
    Ok(())
}

/// 供给修订文本字段的规范化结果（快递说明/区域/能力/预填依据）。
struct OfferingRevisionTexts {
    dropship_express: Option<String>,
    supply_region: Vec<String>,
    product_capabilities: Vec<String>,
    prefill_source_refs: PrefillSourceRefs,
}

/// 规范化供给修订文本字段（快递说明/区域/能力/预填依据）。
///
/// # 参数
/// * `data` - 供给修订创建数据
///
/// # 返回
/// 返回规范化后的文本字段。
///
/// # 错误
/// 区域为空、条目超长/超限或预填依据不一致时返回错误。
fn normalize_offering_texts(data: &SupplierOfferingRevisionData) -> Result<OfferingRevisionTexts> {
    let dropship_express = normalize_optional_text(
        data.dropship_express.clone(),
        "快递说明",
        DROPSHIP_EXPRESS_MAX_LEN,
    )?;
    let supply_region = normalize_region_list(data.supply_region.clone())?;
    let product_capabilities = normalize_capability_list(data.product_capabilities.clone())?;
    let prefill_source_refs = normalize_prefill_refs(data.prefill_source_refs.clone())?;
    Ok(OfferingRevisionTexts {
        dropship_express,
        supply_region,
        product_capabilities,
        prefill_source_refs,
    })
}

/// 校验有效期窗口。
///
/// # 参数
/// * `valid_from` - 有效期开始
/// * `valid_to` - 有效期结束
///
/// # 错误
/// 有效期结束早于或等于开始时返回错误。
fn ensure_validity_window(valid_from: BusinessDate, valid_to: Option<BusinessDate>) -> Result<()> {
    if let Some(valid_to) = valid_to {
        if valid_to <= valid_from {
            return Err(Error::from("有效期结束必须晚于开始"));
        }
    }
    Ok(())
}

/// 规范化可供区域列表（去空白、丢弃空条目、去重、数量上限）。
///
/// # 参数
/// * `regions` - 原始区域列表
///
/// # 返回
/// 返回规范化后的区域列表。
///
/// # 错误
/// 列表为空或条目超长/超限时返回错误。
fn normalize_region_list(regions: Vec<String>) -> Result<Vec<String>> {
    let normalized = normalize_string_list(regions, REGION_MAX_LEN, MAX_REGIONS, "供给区域")?;
    if normalized.is_empty() {
        return Err(Error::from("供给区域不能为空"));
    }
    Ok(normalized)
}

/// 规范化商品能力列表（去空白、丢弃空条目、去重、数量上限；允许为空）。
///
/// # 参数
/// * `capabilities` - 原始能力列表
///
/// # 返回
/// 返回规范化后的能力列表。
///
/// # 错误
/// 条目超长或超限时返回错误。
fn normalize_capability_list(capabilities: Vec<String>) -> Result<Vec<String>> {
    normalize_string_list(capabilities, CAPABILITY_MAX_LEN, MAX_CAPABILITIES, "商品能力")
}

/// 规范化字符串列表并去重保序。
///
/// # 参数
/// * `values` - 原始列表
/// * `item_max_len` - 单条最大长度
/// * `max_count` - 最大条数
/// * `label` - 字段说明
///
/// # 返回
/// 返回规范化后的列表。
///
/// # 错误
/// 条目超长或超限时返回错误。
fn normalize_string_list(
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
        let value = normalize_optional_text(Some(value), label, item_max_len)?;
        if let Some(value) = value {
            if !normalized.contains(&value) {
                normalized.push(value);
            }
        }
    }
    Ok(normalized)
}

/// 规范化预填依据。
///
/// 时区/日历版本必须与预填业务日期同时提供（§6.14 逐字段对应）。
///
/// # 参数
/// * `refs` - 原始预填依据
///
/// # 返回
/// 返回规范化后的预填依据。
///
/// # 错误
/// 预填时区/日历版本缺少业务日期时返回错误。
fn normalize_prefill_refs(refs: PrefillSourceRefs) -> Result<PrefillSourceRefs> {
    let timezone = normalize_optional_text(refs.valid_from_timezone, "预填时区", TIMEZONE_MAX_LEN)?;
    let calendar_version = normalize_optional_text(
        refs.valid_from_calendar_version,
        "预填日历版本",
        CALENDAR_VERSION_MAX_LEN,
    )?;
    PrefillSourceRefs::ensure_valid(
        timezone.clone(),
        calendar_version.clone(),
        refs.valid_from_date.is_some(),
    )?;
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
    use super::{
        OfferingStatus, PrefillSourceRefs, SupplierOffering, SupplierOfferingData, SupplierOfferingRevision,
        SupplierOfferingRevisionData,
    };
    use crate::common::time::BusinessDate;
    use crate::ids::{
        SkuId, SupplierAccountId, SupplierCatalogSkuId, SupplierOfferingId, SupplierOfferingRevisionId,
    };
    use crate::money::{Amount, Quantity, Rate, UnitPrice};
    use crate::supplier_catalog::sku::AvailabilityStatus;
    use std::str::FromStr;

    fn offering_data() -> SupplierOfferingData {
        SupplierOfferingData {
            sku_id: SkuId::new("sku-1"),
            supplier_id: SupplierAccountId::new("sup-1"),
            supplier_catalog_sku_id: SupplierCatalogSkuId::new("scs-1"),
        }
    }

    /// 一对换算一致的供给价：gross 9.99 @13% → tax 1.30 → net 8.69。
    fn pair() -> (UnitPrice, UnitPrice) {
        (
            UnitPrice::from_str("9.9900").unwrap(),
            UnitPrice::from_str("8.6900").unwrap(),
        )
    }

    fn offering_revision_data() -> SupplierOfferingRevisionData {
        let (dropship_gross, dropship_net) = pair();
        let (bulk_gross, bulk_net) = pair();
        SupplierOfferingRevisionData {
            supplier_offering_id: SupplierOfferingId::new("so-1"),
            revision_no: 1,
            dropship_supply_price_gross: dropship_gross,
            dropship_supply_price_net: dropship_net,
            bulk_supply_price_gross: bulk_gross,
            bulk_supply_price_net: bulk_net,
            input_tax_rate: Rate::from_str("0.130000").unwrap(),
            dropship_express: Some(" 次日达 ".to_string()),
            freight_amount: Some(Amount::from_str("5.00").unwrap()),
            service_fee_amount: None,
            bulk_minimum_order_quantity: Quantity::from_str("10.000000").unwrap(),
            supply_region: vec![" 全国 ".to_string(), "全国".to_string()],
            availability_status: AvailabilityStatus::Available,
            available_quantity: Some(Quantity::from_str("100.000000").unwrap()),
            product_capabilities: vec!["CANCEL".to_string(), "REFUND".to_string()],
            valid_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
            valid_to: Some(BusinessDate::from_ymd(2026, 12, 31).unwrap()),
            prefill_source_refs: PrefillSourceRefs::default(),
        }
    }

    #[test]
    fn offering_new_and_status_update() {
        let mut offering =
            SupplierOffering::new(SupplierOfferingId::new("so-1"), offering_data(), "admin-1").unwrap();
        assert_eq!(offering.stable.status(), OfferingStatus::Active);

        offering.update(OfferingStatus::Paused, "admin-2").unwrap();
        assert_eq!(offering.stable.status(), OfferingStatus::Paused);
        assert_eq!(offering.stable.updated_by, "admin-2");
    }

    #[test]
    fn offering_revision_happy_path_dedups_regions() {
        let revision =
            SupplierOfferingRevision::new(SupplierOfferingRevisionId::new("sor-1"), offering_revision_data())
                .unwrap();
        assert_eq!(revision.supply_region, vec!["全国"]);
        assert_eq!(revision.dropship_express.as_deref(), Some("次日达"));
        assert_eq!(
            revision.bulk_minimum_order_quantity,
            Quantity::from_str("10.000000").unwrap()
        );
        assert_eq!(revision.revision.revision_no, 1);
    }

    #[test]
    fn offering_revision_rejects_inconsistent_price_pair() {
        let bad_net = UnitPrice::from_str("8.7000").unwrap();
        let data = SupplierOfferingRevisionData {
            dropship_supply_price_net: bad_net,
            ..offering_revision_data()
        };
        assert!(SupplierOfferingRevision::new(SupplierOfferingRevisionId::new("sor-2"), data).is_err());

        let missing_bulk_net = SupplierOfferingRevisionData {
            bulk_supply_price_net: UnitPrice::from_str("0.0000").unwrap(),
            ..offering_revision_data()
        };
        assert!(
            SupplierOfferingRevision::new(SupplierOfferingRevisionId::new("sor-3"), missing_bulk_net)
                .is_err()
        );
    }

    #[test]
    fn offering_revision_rejects_zero_moq_and_empty_region() {
        let zero_moq = SupplierOfferingRevisionData {
            bulk_minimum_order_quantity: Quantity::from_str("0.000000").unwrap(),
            ..offering_revision_data()
        };
        assert!(SupplierOfferingRevision::new(SupplierOfferingRevisionId::new("sor-4"), zero_moq).is_err());

        let empty_region = SupplierOfferingRevisionData {
            supply_region: vec![],
            ..offering_revision_data()
        };
        assert!(
            SupplierOfferingRevision::new(SupplierOfferingRevisionId::new("sor-5"), empty_region).is_err()
        );

        let inverted = SupplierOfferingRevisionData {
            valid_from: BusinessDate::from_ymd(2026, 12, 31).unwrap(),
            valid_to: Some(BusinessDate::from_ymd(2026, 1, 1).unwrap()),
            ..offering_revision_data()
        };
        assert!(SupplierOfferingRevision::new(SupplierOfferingRevisionId::new("sor-6"), inverted).is_err());

        let zero_no = SupplierOfferingRevisionData {
            revision_no: 0,
            ..offering_revision_data()
        };
        assert!(SupplierOfferingRevision::new(SupplierOfferingRevisionId::new("sor-7"), zero_no).is_err());
    }

    #[test]
    fn prefill_refs_require_date_for_timezone() {
        let refs = PrefillSourceRefs {
            valid_from_timezone: Some("Asia/Shanghai".to_string()),
            ..Default::default()
        };
        let data = SupplierOfferingRevisionData {
            prefill_source_refs: refs,
            ..offering_revision_data()
        };
        assert!(SupplierOfferingRevision::new(SupplierOfferingRevisionId::new("sor-8"), data).is_err());

        let good = PrefillSourceRefs {
            valid_from_date: Some(BusinessDate::from_ymd(2026, 1, 1).unwrap()),
            valid_from_timezone: Some(" Asia/Shanghai ".to_string()),
            valid_from_calendar_version: Some("v1".to_string()),
            ..Default::default()
        };
        let data = SupplierOfferingRevisionData {
            prefill_source_refs: good,
            ..offering_revision_data()
        };
        let revision = SupplierOfferingRevision::new(SupplierOfferingRevisionId::new("sor-9"), data).unwrap();
        assert_eq!(
            revision.prefill_source_refs.valid_from_timezone.as_deref(),
            Some("Asia/Shanghai")
        );
    }

    #[test]
    fn offering_status_labels_and_codes() {
        assert_eq!(OfferingStatus::Paused.label(), "暂停");
        assert_eq!(OfferingStatus::Stopped.as_str(), "STOPPED");
        assert_eq!(
            serde_json::to_string(&OfferingStatus::Paused).unwrap(),
            "\"PAUSED\""
        );
    }
}
