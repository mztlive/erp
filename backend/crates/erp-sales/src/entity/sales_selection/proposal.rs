//! 销售方案：客户提交后不可改明细。

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
    /// 份数；商城兑换为空。
    pub quantity: Option<u32>,
    /// 售价。
    pub unit_price: Amount,
    /// 行金额；商城兑换为空。
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
    /// 数量；商城兑换为空。P0 套餐成员一件，数量等于套餐份数。
    pub quantity: Option<u32>,
    /// 行金额；商城兑换为空。
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
    /// 准备批次。
    pub batch_id: String,
    /// 选品形态。
    pub form: SelectionForm,
    /// 提交方式，必须取自选品册。
    pub submit_mode: SubmitMode,
    /// 已提交会话版本。
    pub session_version: u64,
    /// 提交时间。
    pub submitted_at: Instant,
    /// 按份采购合计；商城兑换为空。
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
    /// 准备批次。
    pub batch_id: String,
    /// 选品形态。
    pub form: SelectionForm,
    /// 提交方式，必须取自选品册。
    pub submit_mode: SubmitMode,
    /// 已提交会话版本。
    pub session_version: u64,
    /// 提交时间。
    pub submitted_at: Instant,
    /// 提交来源。
    pub source: ProposalSource,
    /// 按份采购合计；商城兑换为空。
    pub total_amount: Option<Amount>,
}

impl SalesSelectionProposal {
    /// 创建方案表头。
    ///
    /// # 参数
    /// * `id` - 方案身份
    /// * `data` - 创建字段
    ///
    /// # 返回
    /// 返回不可改的方案表头。
    ///
    /// # 错误
    /// 编号为空或合计与提交方式不一致时拒绝。
    pub fn new(id: SalesSelectionProposalId, data: SalesSelectionProposalData) -> Result<Self> {
        let proposal_no = data.proposal_no.trim();
        if proposal_no.is_empty() {
            return Err(Error::from("方案编号不能为空"));
        }
        if data.submit_mode.requires_quantity() != data.total_amount.is_some() {
            return Err(Error::from("方案合计必须与提交方式一致"));
        }
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            proposal_no: proposal_no.to_string(),
            customer_id: data.customer_id,
            customer_name: data.customer_name,
            booklet_id: data.booklet_id,
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
///
/// # 参数
/// * `proposal_id` - 方案
/// * `submit_mode` - 提交方式
/// * `choices` - 已保存选择
/// * `items` - 陈列项
/// * `next_id` - 明细行身份生成
///
/// # 返回
/// 返回陈列项行、SKU 行与按份采购合计。
///
/// # 错误
/// 缺项、份数与方式不符或金额不平衡时拒绝。
pub fn build_proposal_lines<F>(
    proposal_id: &SalesSelectionProposalId,
    submit_mode: SubmitMode,
    choices: &[SessionChoice],
    items: &[SalesSelectionDisplayItem],
    mut next_id: F,
) -> Result<(Vec<SalesSelectionProposalDisplayLine>, Vec<SalesSelectionProposalSkuLine>, Option<Amount>)>
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
        let (display_line, item_sku_lines) =
            lines_for_choice(proposal_id, submit_mode, choice, item, &mut next_id)?;
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
    let total = match submit_mode {
        SubmitMode::ByQuantity => {
            if display_total != sku_total {
                return Err(Error::from("陈列项金额与商品金额不一致"));
            }
            Some(display_total)
        },
        SubmitMode::MallRedeem => None,
    };
    Ok((display_lines, sku_lines, total))
}

/// 为一项选择生成陈列行与 SKU 行。
///
/// # 参数
/// * `proposal_id` - 方案
/// * `submit_mode` - 提交方式
/// * `choice` - 选择
/// * `item` - 陈列项
/// * `next_id` - 身份生成
///
/// # 返回
/// 返回一行陈列与对应 SKU 行。不同套餐中相同 SKU 分别保留，不合并。
///
/// # 错误
/// 份数或金额非法时拒绝。
fn lines_for_choice<F>(
    proposal_id: &SalesSelectionProposalId,
    submit_mode: SubmitMode,
    choice: &SessionChoice,
    item: &SalesSelectionDisplayItem,
    next_id: &mut F,
) -> Result<(SalesSelectionProposalDisplayLine, Vec<SalesSelectionProposalSkuLine>)>
where
    F: FnMut() -> String,
{
    let quantity = choice.quantity;
    if submit_mode.requires_quantity() != quantity.is_some() {
        return Err(Error::from("提交方式与份数不一致"));
    }
    let unit_price = item.price();
    let line_amount = match quantity {
        Some(copies) => Some(super::pricing::try_mul_u32(unit_price, copies)?),
        None => None,
    };
    let display_line = SalesSelectionProposalDisplayLine {
        base: BaseModel::new(next_id()),
        proposal_id: proposal_id.clone(),
        display_item_id: SalesSelectionDisplayItemId::new(item.base.id.clone()),
        cover_asset_id: item.cover_asset_id().map(ToOwned::to_owned),
        tier_id: match &item.kind {
            DisplayKind::Package { tier_id, .. } => Some(tier_id.clone()),
            DisplayKind::SingleSku { .. } => None,
        },
        quantity,
        unit_price,
        line_amount,
    };
    let sku_lines = sku_lines_for_item(proposal_id, item, quantity, next_id)?;
    Ok((display_line, sku_lines))
}

/// 按陈列项展开 SKU 行。
///
/// # 参数
/// * `proposal_id` - 方案
/// * `item` - 陈列项
/// * `quantity` - 套餐或单品份数
/// * `next_id` - 身份生成
///
/// # 返回
/// 单品一行；套餐每个成员一行，数量等于套餐份数。
///
/// # 错误
/// 金额溢出时拒绝。
fn sku_lines_for_item<F>(
    proposal_id: &SalesSelectionProposalId,
    item: &SalesSelectionDisplayItem,
    quantity: Option<u32>,
    next_id: &mut F,
) -> Result<Vec<SalesSelectionProposalSkuLine>>
where
    F: FnMut() -> String,
{
    let members = match &item.kind {
        DisplayKind::SingleSku { sku } => vec![sku],
        DisplayKind::Package { members, .. } => members.iter().collect(),
    };
    let mut lines = Vec::with_capacity(members.len());
    for sku in members {
        let line_amount = match quantity {
            Some(copies) => Some(super::pricing::try_mul_u32(sku.sales_visible_price_gross, copies)?),
            None => None,
        };
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
            quantity,
            line_amount,
        });
    }
    Ok(lines)
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::ids::{
        ProductId, SalesSelectionBookletId, SalesSelectionDisplayItemId, SalesSelectionProposalId, SkuId,
        SkuRevisionId,
    };
    use erp_core::money::Amount;

    use super::build_proposal_lines;
    use crate::entity::sales_selection::display_item::SalesSelectionDisplayItem;
    use crate::entity::sales_selection::session::SessionChoice;
    use crate::entity::sales_selection::sku_snapshot::SkuSnapshot;
    use crate::entity::sales_selection::types::SubmitMode;

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
        let (display, sku_lines, total) =
            build_proposal_lines(&proposal_id, SubmitMode::ByQuantity, &choices, &[item], || {
                seq += 1;
                format!("id-{seq}")
            })
            .unwrap();
        assert_eq!(total, Some(Amount::from_str("30.00").unwrap()));
        assert_eq!(display[0].line_amount, total);
        assert_eq!(sku_lines[0].quantity, Some(3));
        assert_eq!(sku_lines[0].line_amount, total);
    }
}
