//! `sku_revision` SKU 修订（数据模型 §6.3、§4.4，不可变修订）。
//!
//! 正式版本按 §4.4 内联结构化快照字段（SKU 名称、规格、条码、物流属性与价格）；
//! `(sku_id, revision_no)` 唯一（唯一约束跨行，属 P3/索引校验）。
//! 销售可见价与市场价是独立事实：`sales_visible_price_gross >= 0` 是公司商品池
//! 销售资格条件之一，两者不得从采购成本、来源底价或彼此自动计算（数据模型 §6.3）。
//! 修订一经形成不得修改，本实体不提供 `update()`。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::catalog::status::EnableStatus;
use crate::common::revision::RevisionBase;
use crate::common::time::BusinessDate;
use crate::errors::{Error, Result};
use crate::ids::{FileAssetId, SkuId, SkuRevisionId};
use crate::money::{Amount, Quantity};
use crate::validation::{normalize_optional_text, normalize_required_text};

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
    /// 公司对销售可见的含税价格（与供应商成本独立，非负）。
    pub sales_visible_price_gross: Option<Amount>,
    /// 市场展示参考价（非负；非正式发布价）。
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
    /// 公司对销售可见的含税价格（非负）。
    pub sales_visible_price_gross: Option<Amount>,
    /// 市场展示参考价（非负）。
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
    /// 并要求重量/体积/销售可见价/市场价均为非负定点数。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SkuRevisionId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的 SKU 修订实体。
    ///
    /// # 错误
    /// 当 name 为空/超长、revision_no 为 0、生效区间倒挂，或物流属性/价格为负数时返回错误。
    pub fn new(id: SkuRevisionId, data: SkuRevisionData) -> Result<Self> {
        let name = normalize_required_text(data.name, "SKU名称不能为空", NAME_MAX_LEN, "SKU名称过长")?;
        let description = normalize_optional_text(data.description, "SKU描述", DESCRIPTION_MAX_LEN)?;
        let specification = normalize_optional_text(data.specification, "SKU规格", SPECIFICATION_MAX_LEN)?;
        let barcode = normalize_optional_text(data.barcode, "条码", BARCODE_MAX_LEN)?;
        ensure_revision_no(data.revision_no)?;
        ensure_effective_window(data.effective_from, data.effective_to)?;
        ensure_non_negative_quantity(data.weight_kg, "重量")?;
        ensure_non_negative_quantity(data.volume_m3, "体积")?;
        ensure_non_negative_amount(data.sales_visible_price_gross, "销售可见价")?;
        ensure_non_negative_amount(data.market_price, "市场价")?;

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
            sales_visible_price_gross: data.sales_visible_price_gross,
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
                sales_visible_price_gross: self.sales_visible_price_gross,
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
}

/// 校验修订序号从 1 开始。
///
/// # 参数
/// * `revision_no` - 修订序号
///
/// # 返回
/// 大于等于 1 时返回 `Ok(())`。
///
/// # 错误
/// 为 0 时返回错误。
fn ensure_revision_no(revision_no: u32) -> Result<()> {
    if revision_no == 0 {
        return Err(Error::from("修订序号必须从 1 开始"));
    }
    Ok(())
}

/// 校验生效区间不倒挂。
///
/// # 参数
/// * `effective_from` - 生效开始日
/// * `effective_to` - 生效结束日
///
/// # 返回
/// 结束日晚于开始日（或无限期）时返回 `Ok(())`。
///
/// # 错误
/// 结束日早于或等于开始日时返回错误。
fn ensure_effective_window(effective_from: BusinessDate, effective_to: Option<BusinessDate>) -> Result<()> {
    if let Some(effective_to) = effective_to {
        if effective_to <= effective_from {
            return Err(Error::from("生效结束日必须晚于生效开始日"));
        }
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
/// * `value` - 销售可见价或市场价
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
    use super::*;
    use crate::common::state::{assert_adjacency_closed, ensure_transition};
    use crate::ids::SkuRevisionId;
    use crate::money::{line_amounts, Rate, UnitPrice};
    use std::str::FromStr;

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
            sales_visible_price_gross: Some(Amount::from_str("99.90").unwrap()),
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
        assert_eq!(
            revision.source_main_image_asset_id,
            Some(FileAssetId::new("asset-main-1"))
        );
        assert_eq!(revision.weight_kg, Some(Quantity::from_str("0.500000").unwrap()));
        assert_eq!(
            revision.sales_visible_price_gross,
            Some(Amount::from_str("99.90").unwrap())
        );
        assert_eq!(revision.revision.revision_no, 1);
        assert!(revision.is_active());
    }

    /// 失败路径：必填空与超长各一条。
    #[test]
    fn new_rejects_empty_and_overlong_name() {
        let empty = SkuRevisionData {
            name: "  ".to_string(),
            ..data()
        };
        assert!(SkuRevision::new(SkuRevisionId::new("rev-1"), empty).is_err());

        let overlong = SkuRevisionData {
            name: "n".repeat(129),
            ..data()
        };
        assert!(SkuRevision::new(SkuRevisionId::new("rev-1"), overlong).is_err());
    }

    /// 失败路径：越界（修订序号为 0）与关联不一致（生效区间倒挂）各一条。
    #[test]
    fn new_rejects_zero_revision_no_and_reversed_window() {
        let zero_revision = SkuRevisionData {
            revision_no: 0,
            ..data()
        };
        assert!(SkuRevision::new(SkuRevisionId::new("rev-1"), zero_revision).is_err());

        let reversed = SkuRevisionData {
            effective_from: BusinessDate::from_ymd(2026, 3, 1).unwrap(),
            effective_to: Some(BusinessDate::from_ymd(2026, 2, 1).unwrap()),
            ..data()
        };
        assert!(SkuRevision::new(SkuRevisionId::new("rev-1"), reversed).is_err());
    }

    /// 金额：价格与物流属性为负数时被拒绝（定点类型仍可带负号，需实体校验）。
    #[test]
    fn new_rejects_negative_prices_and_logistics() {
        let negative_price = SkuRevisionData {
            sales_visible_price_gross: Some(Amount::from_str("-1.00").unwrap()),
            ..data()
        };
        assert!(SkuRevision::new(SkuRevisionId::new("rev-1"), negative_price).is_err());

        let negative_market = SkuRevisionData {
            market_price: Some(Amount::from_str("-0.01").unwrap()),
            ..data()
        };
        assert!(SkuRevision::new(SkuRevisionId::new("rev-1"), negative_market).is_err());

        let negative_weight = SkuRevisionData {
            weight_kg: Some(Quantity::from_str("-0.100000").unwrap()),
            ..data()
        };
        assert!(SkuRevision::new(SkuRevisionId::new("rev-1"), negative_weight).is_err());
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
        assert_eq!(
            successor.sales_visible_price_gross,
            current.sales_visible_price_gross
        );
        assert_eq!(successor.status, current.status);
    }

    /// 金额三元组：销售可见价参与逐行舍入计算时满足 gross = net + tax 恒等。
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
        assert_eq!(value["weight_kg"], serde_json::json!("0.500000"));

        let back: SkuRevision = serde_json::from_str(&json).unwrap();
        assert_eq!(back, revision);
    }

    /// 状态机：合法迁移通过，邻接矩阵对称闭合。
    #[test]
    fn status_transitions_follow_document_state() {
        assert!(ensure_transition(EnableStatus::Active, EnableStatus::Disabled).is_ok());
        assert!(ensure_transition(EnableStatus::Disabled, EnableStatus::Active).is_ok());
        assert_adjacency_closed(&[EnableStatus::Active, EnableStatus::Disabled]);
    }

    /// BSON wire 往返：金额/数量持久化为 Decimal128。
    #[test]
    fn sku_revision_roundtrips_through_bson() {
        let revision = SkuRevision::new(SkuRevisionId::new("rev-1"), data()).unwrap();
        let bytes = bson::serialize_to_vec(&revision).unwrap();
        let wire_doc: bson::Document = bson::deserialize_from_slice(&bytes).unwrap();
        assert!(matches!(
            wire_doc.get("sales_visible_price_gross"),
            Some(bson::Bson::Decimal128(_))
        ));
        assert!(matches!(
            wire_doc.get("weight_kg"),
            Some(bson::Bson::Decimal128(_))
        ));

        let back: SkuRevision = bson::deserialize_from_slice(&bytes).unwrap();
        assert_eq!(back, revision);
    }
}
