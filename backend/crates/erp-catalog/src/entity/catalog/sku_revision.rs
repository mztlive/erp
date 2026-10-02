//! `sku_revision` SKU 修订（数据模型 §6.3、§4.4，不可变修订）。
//!
//! 正式版本按 §4.4 内联结构化快照字段（SKU 名称、规格、条码、物流属性与价格）；
//! `(sku_id, revision_no)` 唯一（唯一约束跨行，属 P3/索引校验）。
//! 出厂价、一件代发价、集采价与市场价均为独立维护的含税销售参考价。
//! `sales_visible_price_gross` 保留历史字段名，业务含义为一件代发价；已维护该价格
//! 是公司商品池销售资格条件之一。四价不得从供应商成本或彼此自动计算。
//! 修订一经形成不得修改，本实体不提供 `update()`。

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::revision::RevisionBase;
use erp_core::common::time::BusinessDate;
use erp_core::ids::{FileAssetId, SkuId, SkuRevisionId};
use erp_core::money::{Amount, Quantity};
use erp_core::validation::{normalize_optional_text, normalize_required_text};
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

use super::revision::{ensure_effective_window, ensure_revision_no};
use crate::entity::catalog::SkuSalesPrices;
use crate::entity::catalog::status::EnableStatus;

/// SKU 名称最大长度。
const NAME_MAX_LEN: usize = 128;
/// 描述最大长度。
const DESCRIPTION_MAX_LEN: usize = 512;
/// 规格/服务内容最大长度。
const SPECIFICATION_MAX_LEN: usize = 1024;
/// 条码最大长度。
const BARCODE_MAX_LEN: usize = 128;

/// SKU 修订创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkuRevisionData {
    /// 所属稳定 SKU。
    pub sku_id: SkuId,
    /// 修订序号（同一 SKU 内从 1 递增）。
    pub revision_no: u32,
    /// 公司审核后的 SKU 名称（结构化快照）。
    pub name: String,
    /// 公司审核后的描述。
    pub description: Option<String>,
    /// 公司审核后的规格或服务内容。
    pub specification: Option<String>,
    /// 条码原值（冲突时进入人工差异，不据此自动合并 SKU）。
    pub barcode: Option<String>,
    /// 来源 SKU 主图（已归档受控文件，D05；缺省为 `None`）。
    pub source_main_image_asset_id: Option<FileAssetId>,
    /// 重量（千克，定点数，非负）。
    pub weight_kg: Option<Quantity>,
    /// 体积（立方米，定点数，非负）。
    pub volume_m3: Option<Quantity>,
    /// 公司出厂含税销售参考价（独立维护，非负）。
    #[serde(default)]
    pub factory_price_gross: Option<Amount>,
    /// 公司一件代发含税销售参考价（与供应商成本独立，非负）。
    pub sales_visible_price_gross: Option<Amount>,
    /// 公司集采含税销售参考价（独立维护，非负）。
    #[serde(default)]
    pub bulk_price_gross: Option<Amount>,
    /// 公司集采价起订数量；未维护时按一件代发价取价，有值必须大于零。
    #[serde(default)]
    pub bulk_min_quantity: Option<Quantity>,
    /// 含税市场参考价（非负；非正式发布价）。
    pub market_price: Option<Amount>,
    /// 修订启停状态。
    pub status: EnableStatus,
    /// 生效开始日。
    pub effective_from: BusinessDate,
    /// 生效结束日；空表示无限期。
    pub effective_to: Option<BusinessDate>,
}

/// SKU 修订实体（不可变修订，数据模型 §6.3、§4.4）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct SkuRevision {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub revision: RevisionBase,
    /// 所属稳定 SKU。
    pub sku_id: SkuId,
    /// 公司审核后的 SKU 名称（结构化快照）。
    pub name: String,
    /// 公司审核后的描述。
    pub description: Option<String>,
    /// 公司审核后的规格或服务内容。
    pub specification: Option<String>,
    /// 条码原值。
    pub barcode: Option<String>,
    /// 来源 SKU 主图（已归档受控文件，D05）。
    pub source_main_image_asset_id: Option<FileAssetId>,
    /// 重量（千克，定点数，非负）。
    pub weight_kg: Option<Quantity>,
    /// 体积（立方米，定点数，非负）。
    pub volume_m3: Option<Quantity>,
    /// 公司出厂含税销售参考价（独立维护，非负）。
    #[serde(default)]
    pub factory_price_gross: Option<Amount>,
    /// 公司一件代发含税销售参考价（非负）。
    pub sales_visible_price_gross: Option<Amount>,
    /// 公司集采含税销售参考价（独立维护，非负）。
    #[serde(default)]
    pub bulk_price_gross: Option<Amount>,
    /// 公司集采价起订数量；未维护时按一件代发价取价，有值必须大于零。
    #[serde(default)]
    pub bulk_min_quantity: Option<Quantity>,
    /// 含税市场参考价（非负）。
    pub market_price: Option<Amount>,
    /// 修订启停状态。
    pub status: EnableStatus,
    /// 生效开始日。
    pub effective_from: BusinessDate,
    /// 生效结束日；空表示无限期。
    pub effective_to: Option<BusinessDate>,
}

impl SkuRevision {
    /// 创建 SKU 修订。
    ///
    /// 完成 name/description/specification/barcode 的校验与规范化（去首尾空白、
    /// 非空、长度上限），校验修订序号从 1 开始、生效区间不倒挂，
    /// 并要求重量、体积及四种含税销售参考价均为非负定点数。
    ///
    /// # 参数
    /// * `id` - 实体主键（`erp_core::ids::SkuRevisionId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的 SKU 修订实体。
    ///
    /// # 错误
    /// 当 name 为空/超长、revision_no 为 0、生效区间倒挂、物流属性/价格为负数，
    /// 或已维护的集采起订数量不大于零时返回错误。
    pub fn new(id: SkuRevisionId, data: SkuRevisionData) -> Result<Self> {
        let name = normalize_required_text(data.name, "SKU名称不能为空", NAME_MAX_LEN, "SKU名称过长")?;
        let description = normalize_optional_text(data.description, "SKU描述", DESCRIPTION_MAX_LEN)?;
        let specification = normalize_optional_text(data.specification, "SKU规格", SPECIFICATION_MAX_LEN)?;
        let barcode = normalize_optional_text(data.barcode, "条码", BARCODE_MAX_LEN)?;
        ensure_revision_no(data.revision_no)?;
        ensure_effective_window(data.effective_from, data.effective_to)?;
        ensure_non_negative_quantity(data.weight_kg, "重量")?;
        ensure_non_negative_quantity(data.volume_m3, "体积")?;
        ensure_non_negative_amount(data.factory_price_gross, "出厂价")?;
        ensure_non_negative_amount(data.sales_visible_price_gross, "一件代发价")?;
        ensure_non_negative_amount(data.bulk_price_gross, "集采价")?;
        ensure_non_negative_amount(data.market_price, "市场价")?;
        ensure_positive_quantity(data.bulk_min_quantity, "集采起订数量")?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            revision: RevisionBase::new(data.revision_no),
            sku_id: data.sku_id,
            name,
            description,
            specification,
            barcode,
            source_main_image_asset_id: data.source_main_image_asset_id,
            weight_kg: data.weight_kg,
            volume_m3: data.volume_m3,
            factory_price_gross: data.factory_price_gross,
            sales_visible_price_gross: data.sales_visible_price_gross,
            bulk_price_gross: data.bulk_price_gross,
            bulk_min_quantity: data.bulk_min_quantity,
            market_price: data.market_price,
            status: data.status,
            effective_from: data.effective_from,
            effective_to: data.effective_to,
        })
    }

    /// 从当前快照派生一份改名或改描述的后继修订。
    ///
    /// 规格、条码、主图、物流属性、价格与状态沿用当前不可变快照，只替换名称、
    /// 描述和生效区间。
    ///
    /// # 参数
    /// * `id` - 新修订主键
    /// * `revision_no` - 同一 SKU 内的下一修订序号
    /// * `name` - 新 SKU 名称
    /// * `description` - 新 SKU 描述
    /// * `effective_from` / `effective_to` - 新修订生效区间
    ///
    /// # 返回
    /// 返回经完整实体校验的新 SKU 修订。
    ///
    /// # 错误
    /// 名称、描述、修订序号或生效区间违反实体不变式时返回错误。
    pub fn content_successor(
        &self,
        id: SkuRevisionId,
        revision_no: u32,
        name: String,
        description: Option<String>,
        effective_from: BusinessDate,
        effective_to: Option<BusinessDate>,
    ) -> Result<Self> {
        Self::new(
            id,
            SkuRevisionData {
                sku_id: self.sku_id.clone(),
                revision_no,
                name,
                description,
                specification: self.specification.clone(),
                barcode: self.barcode.clone(),
                source_main_image_asset_id: self.source_main_image_asset_id.clone(),
                weight_kg: self.weight_kg,
                volume_m3: self.volume_m3,
                factory_price_gross: self.factory_price_gross,
                sales_visible_price_gross: self.sales_visible_price_gross,
                bulk_price_gross: self.bulk_price_gross,
                bulk_min_quantity: self.bulk_min_quantity,
                market_price: self.market_price,
                status: self.status,
                effective_from,
                effective_to,
            },
        )
    }

    /// 判断修订是否处于启用状态。
    ///
    /// # 返回
    /// 状态为 `Active` 时返回 `true`。
    pub fn is_active(&self) -> bool {
        self.status.is_active()
    }

    /// 读取当前修订按数量选择销售参考价所需的独立价格事实。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回一件代发价、集采价与集采起订数量；不读取供应商成本。
    ///
    /// # 错误
    /// 无。
    pub fn sales_prices(&self) -> SkuSalesPrices {
        SkuSalesPrices {
            sales_visible_price_gross: self.sales_visible_price_gross,
            bulk_price_gross: self.bulk_price_gross,
            bulk_min_quantity: self.bulk_min_quantity,
        }
    }
}

/// 校验已维护的集采起订数量严格大于零。
///
/// # 参数
/// * `value` - 可选集采起订数量
/// * `label` - 字段说明
///
/// # 返回
/// 缺省或正数时返回 `Ok(())`。
///
/// # 错误
/// 已维护的数量小于或等于零时返回错误。
fn ensure_positive_quantity(value: Option<Quantity>, label: &str) -> Result<()> {
    if value.is_some_and(|quantity| quantity.to_decimal() <= 0.into()) {
        return Err(Error::from(format!("{label}必须大于零")));
    }
    Ok(())
}

/// 校验物流属性为非负定点数量。
///
/// # 参数
/// * `value` - 重量或体积
/// * `label` - 字段说明
///
/// # 返回
/// 非负时返回 `Ok(())`。
///
/// # 错误
/// 为负数时返回错误。
fn ensure_non_negative_quantity(value: Option<Quantity>, label: &str) -> Result<()> {
    if value.is_some_and(|v| v.to_decimal().is_sign_negative()) {
        return Err(Error::from(format!("{label}不能为负数")));
    }
    Ok(())
}

/// 校验价格为非负定点金额。
///
/// # 参数
/// * `value` - 四种含税销售参考价中的任意一项
/// * `label` - 字段说明
///
/// # 返回
/// 非负时返回 `Ok(())`。
///
/// # 错误
/// 为负数时返回错误。
fn ensure_non_negative_amount(value: Option<Amount>, label: &str) -> Result<()> {
    if value.is_some_and(|v| v.to_decimal().is_sign_negative()) {
        return Err(Error::from(format!("{label}不能为负数")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::common::state::{assert_adjacency_closed, ensure_transition};
    use erp_core::ids::SkuRevisionId;
    use erp_core::money::{Rate, UnitPrice, line_amounts};

    use super::*;

    /// 构造四种独立含税参考价与集采起订量的有效修订输入。
    fn data() -> SkuRevisionData {
        SkuRevisionData {
            sku_id: SkuId::new("sku-1"),
            revision_no: 1,
            name: " 坚果礼盒 500g ".to_string(),
            description: Some(" 春节款 ".to_string()),
            specification: None,
            barcode: Some(" 6901234567890 ".to_string()),
            source_main_image_asset_id: Some(FileAssetId::new("asset-main-1")),
            weight_kg: Some(Quantity::from_str("0.500000").unwrap()),
            volume_m3: None,
            factory_price_gross: Some(Amount::from_str("75.00").unwrap()),
            sales_visible_price_gross: Some(Amount::from_str("99.90").unwrap()),
            bulk_price_gross: Some(Amount::from_str("80.00").unwrap()),
            bulk_min_quantity: Some(Quantity::from_str("10.000000").unwrap()),
            market_price: Some(Amount::from_str("129.00").unwrap()),
            status: EnableStatus::Active,
            effective_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
            effective_to: None,
        }
    }

    /// happy path：快照字段 trim 规范化，价格与物流属性落位。
    #[test]
    fn new_trims_and_normalizes_fields() {
        let revision = SkuRevision::new(SkuRevisionId::new("rev-1"), data()).unwrap();

        assert_eq!(revision.name, "坚果礼盒 500g");
        assert_eq!(revision.barcode.as_deref(), Some("6901234567890"));
        assert_eq!(revision.source_main_image_asset_id, Some(FileAssetId::new("asset-main-1")));
        assert_eq!(revision.weight_kg, Some(Quantity::from_str("0.500000").unwrap()));
        assert_eq!(revision.factory_price_gross, Some(Amount::from_str("75.00").unwrap()));
        assert_eq!(revision.sales_visible_price_gross, Some(Amount::from_str("99.90").unwrap()));
        assert_eq!(revision.bulk_price_gross, Some(Amount::from_str("80.00").unwrap()));
        assert_eq!(revision.bulk_min_quantity, Some(Quantity::from_str("10.000000").unwrap()));
        assert_eq!(revision.revision.revision_no, 1);
        assert!(revision.is_active());
    }

    /// 失败路径：必填空与超长各一条。
    #[test]
    fn new_rejects_empty_and_overlong_name() {
        let empty = SkuRevisionData { name: "  ".to_string(), ..data() };
        assert!(SkuRevision::new(SkuRevisionId::new("rev-1"), empty).is_err());

        let overlong = SkuRevisionData { name: "n".repeat(129), ..data() };
        assert!(SkuRevision::new(SkuRevisionId::new("rev-1"), overlong).is_err());
    }

    /// 失败路径：越界（修订序号为 0）与关联不一致（生效区间倒挂）各一条。
    #[test]
    fn new_rejects_zero_revision_no_and_reversed_window() {
        let zero_revision = SkuRevisionData { revision_no: 0, ..data() };
        assert_eq!(
            SkuRevision::new(SkuRevisionId::new("rev-1"), zero_revision).unwrap_err().to_string(),
            "修订序号必须从 1 开始"
        );

        let reversed = SkuRevisionData {
            effective_from: BusinessDate::from_ymd(2026, 3, 1).unwrap(),
            effective_to: Some(BusinessDate::from_ymd(2026, 2, 1).unwrap()),
            ..data()
        };
        assert_eq!(
            SkuRevision::new(SkuRevisionId::new("rev-1"), reversed).unwrap_err().to_string(),
            "生效结束日必须晚于生效开始日"
        );
    }

    /// 金额：价格与物流属性为负数时被拒绝（定点类型仍可带负号，需实体校验）。
    #[test]
    fn new_rejects_negative_prices_and_logistics() {
        let negative_factory =
            SkuRevisionData { factory_price_gross: Some(Amount::from_str("-1.00").unwrap()), ..data() };
        assert_eq!(
            SkuRevision::new(SkuRevisionId::new("rev-1"), negative_factory).unwrap_err().to_string(),
            "出厂价不能为负数"
        );

        let negative_price =
            SkuRevisionData { sales_visible_price_gross: Some(Amount::from_str("-1.00").unwrap()), ..data() };
        assert!(SkuRevision::new(SkuRevisionId::new("rev-1"), negative_price).is_err());

        let negative_bulk =
            SkuRevisionData { bulk_price_gross: Some(Amount::from_str("-0.01").unwrap()), ..data() };
        assert_eq!(
            SkuRevision::new(SkuRevisionId::new("rev-1"), negative_bulk).unwrap_err().to_string(),
            "集采价不能为负数"
        );

        let negative_market =
            SkuRevisionData { market_price: Some(Amount::from_str("-0.01").unwrap()), ..data() };
        assert!(SkuRevision::new(SkuRevisionId::new("rev-1"), negative_market).is_err());

        let negative_weight =
            SkuRevisionData { weight_kg: Some(Quantity::from_str("-0.100000").unwrap()), ..data() };
        assert!(SkuRevision::new(SkuRevisionId::new("rev-1"), negative_weight).is_err());
    }

    /// 已维护的集采起订量必须为正数，缺省不触发集采价。
    #[test]
    fn new_rejects_non_positive_bulk_minimum() {
        for minimum in ["0.000000", "-1.000000"] {
            let invalid =
                SkuRevisionData { bulk_min_quantity: Some(Quantity::from_str(minimum).unwrap()), ..data() };
            assert_eq!(
                SkuRevision::new(SkuRevisionId::new("rev-1"), invalid).unwrap_err().to_string(),
                "集采起订数量必须大于零"
            );
        }
        let revision = SkuRevision::new(
            SkuRevisionId::new("rev-1"),
            SkuRevisionData { bulk_min_quantity: None, ..data() },
        )
        .unwrap();
        assert_eq!(
            revision.sales_prices().reference_price(Quantity::from_str("100.000000").unwrap()),
            revision.sales_visible_price_gross
        );
    }

    /// 四价可独立缺省，零元价格与已有市场价不改变其他参考价。
    #[test]
    fn new_keeps_independent_missing_and_zero_reference_prices() {
        let revision = SkuRevision::new(
            SkuRevisionId::new("rev-1"),
            SkuRevisionData {
                factory_price_gross: None,
                sales_visible_price_gross: None,
                bulk_price_gross: Some(Amount::from_str("0.00").unwrap()),
                ..data()
            },
        )
        .unwrap();
        assert_eq!(revision.factory_price_gross, None);
        assert_eq!(revision.sales_visible_price_gross, None);
        assert_eq!(revision.bulk_price_gross, Some(Amount::from_str("0.00").unwrap()));
        assert_eq!(revision.market_price, data().market_price);
    }

    /// 后继修订只替换文案与生效区间并保留价格、物流和条码快照。
    #[test]
    fn content_successor_preserves_commercial_snapshot() {
        let current = SkuRevision::new(SkuRevisionId::new("rev-1"), data()).unwrap();
        let successor = current
            .content_successor(
                SkuRevisionId::new("rev-2"),
                2,
                "新 SKU 名称".to_string(),
                Some("新描述".to_string()),
                BusinessDate::from_ymd(2026, 2, 1).unwrap(),
                None,
            )
            .unwrap();

        assert_eq!(successor.revision.revision_no, 2);
        assert_eq!(successor.name, "新 SKU 名称");
        assert_eq!(successor.barcode, current.barcode);
        assert_eq!(successor.weight_kg, current.weight_kg);
        assert_eq!(successor.factory_price_gross, current.factory_price_gross);
        assert_eq!(successor.sales_visible_price_gross, current.sales_visible_price_gross);
        assert_eq!(successor.bulk_price_gross, current.bulk_price_gross);
        assert_eq!(successor.bulk_min_quantity, current.bulk_min_quantity);
        assert_eq!(successor.market_price, current.market_price);
        assert_eq!(successor.status, current.status);
    }

    /// 金额三元组：一件代发价参与逐行舍入计算时满足 gross = net + tax 恒等。
    #[test]
    fn sales_price_follows_line_amounts_consistency() {
        let revision = SkuRevision::new(SkuRevisionId::new("rev-1"), data()).unwrap();
        let price = revision.sales_visible_price_gross.unwrap();
        let unit_price = UnitPrice::try_from(price.to_decimal()).unwrap();

        let (gross, net, tax) = line_amounts(
            unit_price,
            Quantity::from_str("3.000000").unwrap(),
            Rate::from_str("0.130000").unwrap(),
        );
        assert_eq!(gross.to_decimal(), net.to_decimal() + tax.to_decimal());
        assert_eq!(gross.to_decimal().scale(), 2);
    }

    /// 金额：定点类型拒绝超位小数（禁止静默舍入），JSON 形态为字符串。
    #[test]
    fn prices_are_fixed_point_with_string_wire_shape() {
        assert!(Amount::from_str("99.9").is_ok());
        assert!(Amount::from_str("99.999").is_err());

        let revision = SkuRevision::new(SkuRevisionId::new("rev-1"), data()).unwrap();
        let json = serde_json::to_string(&revision).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["sales_visible_price_gross"], serde_json::json!("99.90"));
        assert_eq!(value["factory_price_gross"], serde_json::json!("75.00"));
        assert_eq!(value["bulk_price_gross"], serde_json::json!("80.00"));
        assert_eq!(value["bulk_min_quantity"], serde_json::json!("10.000000"));
        assert_eq!(value["weight_kg"], serde_json::json!("0.500000"));

        let back: SkuRevision = serde_json::from_str(&json).unwrap();
        assert_eq!(back, revision);
    }

    /// 历史不可变修订缺少新字段时按未维护读取，保留原一件代发价。
    #[test]
    fn legacy_revision_defaults_new_prices_and_bulk_minimum() {
        let revision = SkuRevision::new(SkuRevisionId::new("rev-1"), data()).unwrap();
        let mut value = serde_json::to_value(&revision).unwrap();
        let object = value.as_object_mut().unwrap();
        object.remove("factory_price_gross");
        object.remove("bulk_price_gross");
        object.remove("bulk_min_quantity");
        let legacy: SkuRevision = serde_json::from_value(value).unwrap();
        assert_eq!(legacy.factory_price_gross, None);
        assert_eq!(legacy.bulk_price_gross, None);
        assert_eq!(legacy.bulk_min_quantity, None);
        assert_eq!(legacy.sales_visible_price_gross, revision.sales_visible_price_gross);
        assert_eq!(legacy.market_price, revision.market_price);
        assert_eq!(
            legacy.sales_prices().reference_price(Quantity::from_str("100.000000").unwrap()),
            revision.sales_visible_price_gross
        );
    }

    /// 状态机：合法迁移通过，邻接矩阵对称闭合。
    #[test]
    fn status_transitions_follow_document_state() {
        assert!(ensure_transition(EnableStatus::Active, EnableStatus::Disabled).is_ok());
        assert!(ensure_transition(EnableStatus::Disabled, EnableStatus::Active).is_ok());
        assert_adjacency_closed(&[EnableStatus::Active, EnableStatus::Disabled]);
    }
}
