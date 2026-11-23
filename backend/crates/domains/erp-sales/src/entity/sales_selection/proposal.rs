//! 销售方案：客户提交后不可改明细。

use std::slice;

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::time::Instant;
use erp_core::ids::{
    CustomerAccountId, SalesSelectionBookletId, SalesSelectionDisplayItemId, SalesSelectionProposalId,
};
use erp_core::money::Amount;
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

use super::display_item::{DisplayKind, SalesSelectionDisplayItem};
use super::pricing::try_mul_u32;
use super::session::SessionChoice;
use super::types::{ProposalSource, SelectionForm, SubmitMode};

/// 方案陈列项行。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct SalesSelectionProposalDisplayLine {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 所属方案。
    pub proposal_id: SalesSelectionProposalId,
    /// 陈列项身份。
    pub display_item_id: SalesSelectionDisplayItemId,
    /// 封面资产。
    pub cover_asset_id: Option<String>,
    /// 档位；单品为空。
    pub tier_id: Option<String>,
    /// 份数。
    pub quantity: Option<u32>,
    /// 售价。
    pub unit_price: Amount,
    /// 行金额。
    pub line_amount: Option<Amount>,
}

/// 方案 SKU 行。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct SalesSelectionProposalSkuLine {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 所属方案。
    pub proposal_id: SalesSelectionProposalId,
    /// 来源陈列项。
    pub display_item_id: SalesSelectionDisplayItemId,
    /// SKU 稳定身份。
    pub sku_id: String,
    /// SKU 修订。
    pub sku_revision_id: String,
    /// 名称快照。
    pub name: String,
    /// 规格快照；历史记录允许缺省。
    #[serde(default)]
    pub specification: Vec<super::sku_snapshot::SpecificationAttributeSnapshot>,
    /// 单位快照。
    #[serde(default)]
    pub unit: String,
    /// 单价。
    pub unit_price: Amount,
    /// 数量。P0 套餐成员一件，数量等于套餐份数。
    pub quantity: Option<u32>,
    /// 行金额。
    pub line_amount: Option<Amount>,
}

/// 销售方案创建数据。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SalesSelectionProposalData {
    /// 方案编号。
    pub proposal_no: String,
    /// 客户身份。
    pub customer_id: CustomerAccountId,
    /// 客户名称快照。
    pub customer_name: String,
    /// 选品册。
    pub booklet_id: SalesSelectionBookletId,
    /// 提交时所属册的销售负责人快照；当前授权须读取所属册当前责任。
    pub sales_owner_user_id: String,
    /// 提交时所属册的业务组织快照；不作为当前授权依据。
    pub business_org_unit_id: String,
    /// 准备批次。
    pub batch_id: String,
    /// 选品形态。
    pub form: SelectionForm,
    /// 提交方式，必须取自选品册。
    #[serde(default)]
    pub submit_mode: SubmitMode,
    /// 已提交会话版本。
    pub session_version: u64,
    /// 提交时间。
    pub submitted_at: Instant,
    /// 按份采购合计。
    pub total_amount: Option<Amount>,
}

/// 销售方案表头.
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct SalesSelectionProposal {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 方案编号。
    pub proposal_no: String,
    /// 客户身份。
    pub customer_id: CustomerAccountId,
    /// 客户名称快照。
    pub customer_name: String,
    /// 选品册。
    pub booklet_id: SalesSelectionBookletId,
    /// 提交时所属册的销售负责人快照；当前授权须读取所属册当前责任。
    pub sales_owner_user_id: String,
    /// 提交时所属册的业务组织快照；不作为当前授权依据。
    pub business_org_unit_id: String,
    /// 准备批次。
    pub batch_id: String,
    /// 选品形态。
    pub form: SelectionForm,
    /// 提交方式，必须取自选品册。
    #[serde(default)]
    pub submit_mode: SubmitMode,
    /// 已提交会话版本。
    pub session_version: u64,
    /// 提交时间。
    pub submitted_at: Instant,
    /// 提交来源。
    pub source: ProposalSource,
    /// 按份采购合计。
    pub total_amount: Option<Amount>,
}

impl SalesSelectionProposal {
    /// 客户提交选择时用已冻结的客户及合计生成不可变方案表头。
    /// 返回不可改的方案表头。
    /// 编号、销售负责人或业务组织为空，或缺少合计时拒绝。
    ///
    /// 输入约定：
    /// - `id`：方案身份
    /// - `data`：创建字段
    pub fn new(id: SalesSelectionProposalId, data: SalesSelectionProposalData) -> Result<Self> {
        let proposal_no = data.proposal_no.trim();
        if proposal_no.is_empty() {
            return Err(Error::from("方案编号不能为空"));
        }
        if data.total_amount.is_none() {
            return Err(Error::from("方案合计不能为空"));
        }
        if data.sales_owner_user_id.trim().is_empty() {
            return Err(Error::from("销售负责人不能为空"));
        }
        if data.business_org_unit_id.trim().is_empty() {
            return Err(Error::from("业务组织不能为空"));
        }

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            proposal_no: proposal_no.to_string(),
            customer_id: data.customer_id,
            customer_name: data.customer_name,
            booklet_id: data.booklet_id,
            sales_owner_user_id: data.sales_owner_user_id.trim().to_string(),
            business_org_unit_id: data.business_org_unit_id.trim().to_string(),
            batch_id: data.batch_id,
            form: data.form,
            submit_mode: data.submit_mode,
            session_version: data.session_version,
            submitted_at: data.submitted_at,
            source: ProposalSource::PublicLink,
            total_amount: data.total_amount,
        })
    }
}

/// 由会话选择与陈列快照构建方案明细。
/// 返回陈列项行、SKU 行与按份采购合计。
/// 缺项、缺份数或金额不平衡时拒绝。
///
/// 输入约定：
/// - `proposal_id`：方案
/// - `choices`：已保存选择
/// - `items`：陈列项
/// - `next_id`：明细行身份生成
pub fn build_proposal_lines<F>(
    proposal_id: &SalesSelectionProposalId,
    choices: &[SessionChoice],
    items: &[SalesSelectionDisplayItem],
    mut next_id: F,
) -> Result<(Vec<SalesSelectionProposalDisplayLine>, Vec<SalesSelectionProposalSkuLine>, Amount)>
where
    F: FnMut() -> String,
{
    if choices.is_empty() {
        return Err(Error::from("未选任何陈列项，不能提交"));
    }

    let mut display_lines = Vec::new();
    let mut sku_lines = Vec::new();
    let mut display_total = Amount::zero();
    let mut sku_total = Amount::zero();
    for choice in choices {
        let item = items
            .iter()
            .find(|item| item.base.id == choice.display_item_id.as_ref() && item.is_publishable())
            .ok_or_else(|| Error::from("选择了无效陈列项"))?;
        let (display_line, item_sku_lines) = lines_for_choice(proposal_id, choice, item, &mut next_id)?;

        if let Some(amount) = display_line.line_amount {
            display_total = super::pricing::try_add(display_total, amount)?;
        }
        for sku_line in &item_sku_lines {
            if let Some(amount) = sku_line.line_amount {
                sku_total = super::pricing::try_add(sku_total, amount)?;
            }
        }
        display_lines.push(display_line);
        sku_lines.extend(item_sku_lines);
    }

    if display_total != sku_total {
        return Err(Error::from("陈列项金额与商品金额不一致"));
    }
    Ok((display_lines, sku_lines, display_total))
}

// 为一项选择生成陈列行与 SKU 行。
// 返回一行陈列与对应 SKU 行。不同套餐中相同 SKU 分别保留，不合并。
// 份数或金额非法时拒绝。
//
// 输入约定：
// - `proposal_id`：方案
// - `choice`：选择
// - `item`：陈列项
// - `next_id`：身份生成
fn lines_for_choice<F>(
    proposal_id: &SalesSelectionProposalId,
    choice: &SessionChoice,
    item: &SalesSelectionDisplayItem,
    next_id: &mut F,
) -> Result<(SalesSelectionProposalDisplayLine, Vec<SalesSelectionProposalSkuLine>)>
where
    F: FnMut() -> String,
{
    let copies = choice.quantity.ok_or_else(|| Error::from("按份采购必须填写份数"))?;
    let unit_price = item.price();
    let line_amount = try_mul_u32(unit_price, copies)?;
    let display_line = SalesSelectionProposalDisplayLine {
        base: BaseModel::new(next_id()),
        proposal_id: proposal_id.clone(),
        display_item_id: SalesSelectionDisplayItemId::new(item.base.id.clone()),
        cover_asset_id: item.cover_asset_id().map(ToOwned::to_owned),
        tier_id: match &item.kind {
            DisplayKind::Package { tier_id, .. } => Some(tier_id.clone()),
            DisplayKind::SingleSku { .. } => None,
        },
        quantity: Some(copies),
        unit_price,
        line_amount: Some(line_amount),
    };

    let sku_lines = sku_lines_for_item(proposal_id, item, copies, next_id)?;
    Ok((display_line, sku_lines))
}

// 按陈列项展开 SKU 行。
// 结果：单品一行；套餐每个成员一行，数量等于套餐份数。
// 金额溢出时拒绝。
//
// 输入约定：
// - `proposal_id`：方案
// - `item`：陈列项
// - `quantity`：套餐或单品份数
// - `next_id`：身份生成
fn sku_lines_for_item<F>(
    proposal_id: &SalesSelectionProposalId,
    item: &SalesSelectionDisplayItem,
    quantity: u32,
    next_id: &mut F,
) -> Result<Vec<SalesSelectionProposalSkuLine>>
where
    F: FnMut() -> String,
{
    let members = match &item.kind {
        DisplayKind::SingleSku { sku } => slice::from_ref(sku),
        DisplayKind::Package { members, .. } => members.as_slice(),
    };

    let mut lines = Vec::with_capacity(members.len());
    for sku in members {
        let line_amount = try_mul_u32(sku.sales_visible_price_gross, quantity)?;
        lines.push(SalesSelectionProposalSkuLine {
            base: BaseModel::new(next_id()),
            proposal_id: proposal_id.clone(),
            display_item_id: SalesSelectionDisplayItemId::new(item.base.id.clone()),
            sku_id: sku.sku_id.to_string(),
            sku_revision_id: sku.sku_revision_id.to_string(),
            name: sku.name.clone(),
            specification: sku.specification_attributes.clone(),
            unit: sku.unit.clone(),
            unit_price: sku.sales_visible_price_gross,
            quantity: Some(quantity),
            line_amount: Some(line_amount),
        });
    }
    Ok(lines)
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::Error;
    use erp_core::common::time::Instant;
    use erp_core::ids::{
        CustomerAccountId, ProductId, SalesSelectionBookletId, SalesSelectionDisplayItemId,
        SalesSelectionProposalId, SkuId, SkuRevisionId,
    };
    use erp_core::money::Amount;
    use rust_decimal::Decimal;

    use super::{
        SalesSelectionProposal, SalesSelectionProposalData, build_proposal_lines, sku_lines_for_item,
    };
    use crate::entity::sales_selection::display_item::{DisplayKind, SalesSelectionDisplayItem};
    use crate::entity::sales_selection::session::SessionChoice;
    use crate::entity::sales_selection::sku_snapshot::SkuSnapshot;
    use crate::entity::sales_selection::types::SubmitMode;
    use crate::entity::sales_selection::{PackageCoverRef, SelectionForm, SpecificationAttributeSnapshot};

    fn sku() -> SkuSnapshot {
        SkuSnapshot {
            sku_id: SkuId::new("sku-1"),
            sku_revision_id: SkuRevisionId::new("rev-1"),
            product_id: ProductId::new("p-1"),
            product_kind: "PHYSICAL".into(),
            category_id: None,
            name: "茶".into(),
            specification_attributes: Vec::new(),
            unit: "件".into(),
            image: None,
            reference_prices: None,
            sales_visible_price_gross: Amount::from_str("10.00").unwrap(),
        }
    }

    #[test]
    fn by_quantity_display_equals_sku_total() {
        let item = SalesSelectionDisplayItem::single_sku(
            SalesSelectionDisplayItemId::new("d1"),
            SalesSelectionBookletId::new("b1"),
            "batch".into(),
            sku(),
        )
        .unwrap();
        let proposal_id = SalesSelectionProposalId::new("p1");
        let choices = vec![SessionChoice {
            display_item_id: SalesSelectionDisplayItemId::new("d1"),
            quantity: Some(3),
        }];
        let mut seq = 0;
        let (display, sku_lines, total) = build_proposal_lines(&proposal_id, &choices, &[item], || {
            seq += 1;
            format!("id-{seq}")
        })
        .unwrap();

        assert_eq!(total, Amount::from_str("30.00").unwrap());
        assert_eq!(display[0].line_amount, Some(total));
        assert_eq!(sku_lines[0].quantity, Some(3));
        assert_eq!(sku_lines[0].line_amount, Some(total));

        assert_eq!(seq, 2);
        assert_eq!(display[0].base.id, "id-1");
        assert_eq!(sku_lines[0].base.id, "id-2");
    }

    fn proposal_data() -> SalesSelectionProposalData {
        SalesSelectionProposalData {
            proposal_no: "P1".into(),
            customer_id: CustomerAccountId::new("c1"),
            customer_name: "客户甲".into(),
            booklet_id: SalesSelectionBookletId::new("b1"),
            sales_owner_user_id: "sales-1".into(),
            business_org_unit_id: "org-1".into(),
            batch_id: "batch".into(),
            form: SelectionForm::SingleSku,
            submit_mode: SubmitMode::ByQuantity,
            session_version: 2,
            submitted_at: Instant::now(),
            total_amount: Some("30.00".parse().unwrap()),
        }
    }

    #[test]
    fn proposal_requires_total_amount_and_deserializes_legacy_fields() {
        let mut missing_total = proposal_data();
        missing_total.total_amount = None;
        assert!(SalesSelectionProposal::new(SalesSelectionProposalId::new("p0"), missing_total).is_err());

        let proposal =
            SalesSelectionProposal::new(SalesSelectionProposalId::new("p1"), proposal_data()).unwrap();
        let mut json = serde_json::to_value(&proposal).unwrap();
        json.as_object_mut()
            .unwrap()
            .insert("participant_id".into(), serde_json::Value::String(String::new()));
        json.as_object_mut().unwrap().insert("recipient".into(), serde_json::Value::Null);

        let restored: SalesSelectionProposal = serde_json::from_value(json).unwrap();
        assert_eq!(serde_json::to_value(&restored).unwrap(), serde_json::to_value(&proposal).unwrap());
    }

    fn package() -> SalesSelectionDisplayItem {
        let mut second = sku();
        second.sku_id = SkuId::new("sku-2");
        second.sku_revision_id = SkuRevisionId::new("rev-2");
        second.sales_visible_price_gross = "15.00".parse().unwrap();
        second.unit = "盒".into();
        second.specification_attributes =
            vec![SpecificationAttributeSnapshot { name: "包装".into(), value: "礼盒".into() }];
        SalesSelectionDisplayItem::package(
            SalesSelectionDisplayItemId::new("package-1"),
            SalesSelectionBookletId::new("b1"),
            "batch".into(),
            "tier-1".into(),
            vec![sku(), second],
            PackageCoverRef {
                file_asset_id: "cover-1".into(),
                content_checksum: "checksum".into(),
                storage_object_key: "selection/cover.png".into(),
                generator_version: "test".into(),
            },
        )
        .unwrap()
    }

    #[test]
    fn package_totals_expand_every_member() {
        let package = package();
        let choices = vec![SessionChoice {
            display_item_id: SalesSelectionDisplayItemId::new("package-1"),
            quantity: Some(3),
        }];
        let mut sequence = 0;
        let (display_lines, sku_lines, total) =
            build_proposal_lines(&SalesSelectionProposalId::new("p1"), &choices, &[package], || {
                sequence += 1;
                format!("line-{sequence}")
            })
            .unwrap();

        assert_eq!(total, "75.00".parse().unwrap());
        assert_eq!(display_lines[0].quantity, Some(3));
        assert_eq!(display_lines[0].line_amount, Some(total));
        assert_eq!(sku_lines.len(), 2);
        assert_eq!(sku_lines[0].quantity, Some(3));
        assert_eq!(sku_lines[0].line_amount, Some("30.00".parse().unwrap()));
        assert_eq!(sku_lines[1].quantity, Some(3));
        assert_eq!(sku_lines[1].line_amount, Some("45.00".parse().unwrap()));

        assert_eq!(sequence, 3);
        assert_eq!(display_lines[0].base.id, "line-1");
        assert_eq!(sku_lines[0].base.id, "line-2");
        assert_eq!(sku_lines[0].sku_id, "sku-1");
        assert_eq!(sku_lines[1].base.id, "line-3");
        assert_eq!(sku_lines[1].sku_id, "sku-2");
    }

    #[test]
    fn sku_expansion_preserves_member_order_and_snapshots() {
        let single = SalesSelectionDisplayItem::single_sku(
            SalesSelectionDisplayItemId::new("single"),
            SalesSelectionBookletId::new("b1"),
            "batch".into(),
            sku(),
        )
        .unwrap();
        let mut package = package();
        let DisplayKind::Package { members, .. } = &mut package.kind else { panic!("expected package") };
        members.reverse();

        for item in [single, package] {
            let expected = match &item.kind {
                DisplayKind::SingleSku { sku } => vec![sku.clone()],
                DisplayKind::Package { members, .. } => members.clone(),
            };
            let proposal_id = SalesSelectionProposalId::new("proposal");
            let mut calls = 0;
            let lines = sku_lines_for_item(&proposal_id, &item, 0, &mut || {
                calls += 1;
                format!("line-{calls}")
            })
            .unwrap();

            assert_eq!(calls, expected.len());
            assert_eq!(lines.len(), expected.len());
            for (index, (line, snapshot)) in lines.iter().zip(&expected).enumerate() {
                assert_eq!(line.base.id, format!("line-{}", index + 1));
                assert_eq!(line.proposal_id, proposal_id);
                assert_eq!(line.display_item_id.as_ref(), item.base.id.as_str());
                assert_eq!(line.sku_id, snapshot.sku_id.as_ref());
                assert_eq!(line.sku_revision_id, snapshot.sku_revision_id.as_ref());
                assert_eq!(line.name, snapshot.name);
                assert_eq!(line.specification, snapshot.specification_attributes);
                assert_eq!(line.unit, snapshot.unit);
                assert_eq!(line.unit_price, snapshot.sales_visible_price_gross);
                assert_eq!(line.quantity, Some(0));
                assert_eq!(line.line_amount, Some(Amount::zero()));
            }
        }
    }

    #[test]
    fn empty_package_expands_no_sku_lines_and_generates_no_ids() {
        let mut item = package();
        let DisplayKind::Package { members, .. } = &mut item.kind else { panic!("expected package") };
        members.clear();
        let mut calls = 0;
        let lines = sku_lines_for_item(&SalesSelectionProposalId::new("proposal"), &item, 3, &mut || {
            calls += 1;
            "unused".into()
        })
        .unwrap();
        assert!(lines.is_empty());
        assert_eq!(calls, 0);
    }

    #[test]
    fn sku_expansion_stops_at_overflow_before_generating_failed_or_later_ids() {
        let mut item = package();
        let DisplayKind::Package { members, .. } = &mut item.kind else { panic!("expected package") };
        members[1].sales_visible_price_gross = Amount::try_from(Decimal::MAX).unwrap();
        members.push(sku());
        let original = item.clone();
        let mut calls = 0;
        let error = sku_lines_for_item(&SalesSelectionProposalId::new("proposal"), &item, 2, &mut || {
            calls += 1;
            format!("line-{calls}")
        })
        .unwrap_err();

        assert_eq!(error.to_string(), "行金额超出可表示范围");
        assert!(matches!(error, Error::LogicError(_)));
        assert_eq!(calls, 1);
        assert_eq!(item, original);
    }
}
