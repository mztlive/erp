//! 选品册与方案视图映射。

use super::SalesSelectionService;
use crate::dto::sales_selection::{
    DisplayItemView, DisplayMemberView, ProposalDisplayLineView, ProposalSkuLineView, PublicChoiceView,
    PublicDisplayItemView, PublicReceiptView, PublicSelectionPageKind, PublicSelectionPageView,
    SalesSelectionBookletListItemView, SalesSelectionBookletView, SalesSelectionProposalListItemView,
    SalesSelectionProposalView, TierReportView,
};
use crate::entity::sales_selection::{
    BookletStatus, DisplayKind, SalesSelectionBooklet, SalesSelectionDisplayItem, SalesSelectionPrepareTask,
    SalesSelectionProposal, SalesSelectionProposalDisplayLine, SalesSelectionProposalSkuLine,
    SalesSelectionSession, SkuSnapshot, TierSearchReport, abs_diff,
};

impl SalesSelectionService {
    /// 映射列表行。
    ///
    /// # 参数
    /// * `booklet` - 选品册
    ///
    /// # 返回
    /// 返回列表视图。
    ///
    /// # 错误
    /// 无。
    pub fn booklet_list_item(booklet: &SalesSelectionBooklet) -> SalesSelectionBookletListItemView {
        SalesSelectionBookletListItemView {
            id: booklet.base.id.clone(),
            book_id: booklet.base.id.clone(),
            version: booklet.base.version,
            customer_id: booklet.customer_id.to_string(),
            customer_name: booklet.customer_name.clone(),
            sales_owner_user_id: booklet.sales_owner_user_id.clone(),
            business_org_unit_id: booklet.business_org_unit_id.clone(),
            form: booklet.form,
            selection_form: booklet.form,
            submit_mode: booklet.submit_mode,
            status: booklet.status,
            proposal_id: booklet.proposal_id.as_ref().map(ToString::to_string),
            created_at: booklet.base.created_at,
        }
    }

    /// 映射详情。
    ///
    /// # 参数
    /// * `booklet` - 选品册
    /// * `items` - 陈列
    /// * `task` - 活动或最近任务
    /// * `public_path` - 可复制路径
    ///
    /// # 返回
    /// 返回管理端详情，不含令牌明文。
    ///
    /// # 错误
    /// 无。
    pub fn booklet_view(
        booklet: &SalesSelectionBooklet,
        items: &[SalesSelectionDisplayItem],
        task: Option<&SalesSelectionPrepareTask>,
        public_path: Option<String>,
    ) -> SalesSelectionBookletView {
        let display_count = items.iter().filter(|item| item.is_publishable()).count() as u32;
        let removed_count = items.iter().filter(|item| item.removed).count() as u32;
        let missing_image_count =
            items.iter().filter(|item| item.is_publishable() && item.cover_asset_id().is_none()).count()
                as u32;
        SalesSelectionBookletView {
            pool_filter: booklet.pool_source.filter.clone(),
            sku_ids: booklet.pool_source.sku_ids.iter().flatten().map(ToString::to_string).collect(),
            id: booklet.base.id.clone(),
            book_id: booklet.base.id.clone(),
            version: booklet.base.version,
            customer_id: booklet.customer_id.to_string(),
            customer_name: booklet.customer_name.clone(),
            sales_owner_user_id: booklet.sales_owner_user_id.clone(),
            sales_owner_name: None,
            business_org_unit_id: booklet.business_org_unit_id.clone(),
            form: booklet.form,
            selection_form: booklet.form,
            submit_mode: booklet.submit_mode,
            status: booklet.status,
            pool_source_kind: booklet.pool_source.kind,
            source_kind: booklet.pool_source.kind,
            tiers: booklet.tiers.clone(),
            batch_id: booklet.current_batch_id.clone(),
            eligibility_as_of: booklet.eligibility_as_of,
            prepared_at: booklet.prepared_at,
            display_count,
            removed_count,
            missing_image_count,
            last_prepare_failure: booklet.last_prepare_failure.clone(),
            prepare_stage: task.map(|item| item.stage),
            completed_tier_count: task.map(|item| item.completed_tier_count),
            tier_reports: task
                .map(|item| item.tier_reports.iter().map(tier_report_view).collect())
                .unwrap_or_default(),
            items: items.iter().map(|item| display_item_view(item, booklet)).collect(),
            link_expires_at: booklet.link_expires_at,
            link_revoked: booklet.link_revoked,
            proposal_id: booklet.proposal_id.as_ref().map(ToString::to_string),
            public_path: public_path.clone(),
            public_url: public_path,
            proposal_no: None,
            updated_at: erp_core::common::time::Instant::from_unix_secs(booklet.base.updated_at as i64),
        }
    }

    /// 映射方案详情。
    ///
    /// # 参数
    /// * `proposal` - 表头
    /// * `display_lines` - 陈列行
    /// * `sku_lines` - SKU 行
    ///
    /// # 返回
    /// 返回内部方案视图，不含令牌与成本。
    ///
    /// # 错误
    /// 无。
    pub fn proposal_view(
        proposal: &SalesSelectionProposal,
        display_lines: &[SalesSelectionProposalDisplayLine],
        sku_lines: &[SalesSelectionProposalSkuLine],
    ) -> SalesSelectionProposalView {
        SalesSelectionProposalView {
            id: proposal.base.id.clone(),
            proposal_no: proposal.proposal_no.clone(),
            customer_id: proposal.customer_id.to_string(),
            customer_name: proposal.customer_name.clone(),
            booklet_id: proposal.booklet_id.to_string(),
            sales_owner_user_id: proposal.sales_owner_user_id.clone(),
            business_org_unit_id: proposal.business_org_unit_id.clone(),
            form: proposal.form,
            submit_mode: proposal.submit_mode,
            submitted_at: proposal.submitted_at,
            source: proposal.source,
            total_amount: proposal.total_amount,
            display_lines: display_lines
                .iter()
                .map(|line| ProposalDisplayLineView {
                    display_item_id: line.display_item_id.to_string(),
                    tier_id: line.tier_id.clone(),
                    quantity: line.quantity,
                    unit_price: line.unit_price,
                    line_amount: line.line_amount,
                    cover_asset_id: line.cover_asset_id.clone(),
                })
                .collect(),
            sku_lines: sku_lines
                .iter()
                .map(|line| ProposalSkuLineView {
                    display_item_id: line.display_item_id.to_string(),
                    name: line.name.clone(),
                    specification: line.specification.clone(),
                    unit: line.unit.clone(),
                    quantity: line.quantity,
                    unit_price: line.unit_price,
                    line_amount: line.line_amount,
                })
                .collect(),
        }
    }

    /// 映射方案列表行。
    ///
    /// # 参数
    /// * `proposal` - 表头
    ///
    /// # 返回
    /// 返回列表行。
    ///
    /// # 错误
    /// 无。
    pub fn proposal_list_item(proposal: &SalesSelectionProposal) -> SalesSelectionProposalListItemView {
        SalesSelectionProposalListItemView {
            id: proposal.base.id.clone(),
            proposal_no: proposal.proposal_no.clone(),
            customer_name: proposal.customer_name.clone(),
            booklet_id: proposal.booklet_id.to_string(),
            sales_owner_user_id: proposal.sales_owner_user_id.clone(),
            business_org_unit_id: proposal.business_org_unit_id.clone(),
            form: proposal.form,
            submit_mode: proposal.submit_mode,
            submitted_at: proposal.submitted_at,
        }
    }

    /// 映射公开页。
    ///
    /// # 参数
    /// * `kind` - 页面种类
    /// * `booklet` - 选品册
    /// * `items` - 可展示陈列
    /// * `session` - 会话
    /// * `receipt` - 回执
    ///
    /// # 返回
    /// 返回不含内部字段的公开视图。
    ///
    /// # 错误
    /// 无。
    pub fn public_page(
        kind: PublicSelectionPageKind,
        booklet: &SalesSelectionBooklet,
        items: &[SalesSelectionDisplayItem],
        session: Option<&SalesSelectionSession>,
        receipt: Option<PublicReceiptView>,
    ) -> PublicSelectionPageView {
        if kind == PublicSelectionPageKind::Ended {
            return PublicSelectionPageView {
                kind,
                customer_name: None,
                form: None,
                submit_mode: None,
                session_version: None,
                items: Vec::new(),
                choices: Vec::new(),
                total_amount: None,
                receipt: None,
            };
        }

        PublicSelectionPageView {
            kind,
            customer_name: Some(booklet.customer_name.clone()),
            form: Some(booklet.form),
            submit_mode: Some(booklet.submit_mode),
            session_version: session.map(|item| item.session_version),
            items: items.iter().map(|item| public_item(item, booklet)).collect(),
            choices: session
                .map(|item| {
                    item.choices
                        .iter()
                        .map(|choice| PublicChoiceView {
                            item_id: choice.display_item_id.to_string(),
                            quantity: choice.quantity,
                            line_amount: None,
                        })
                        .collect()
                })
                .unwrap_or_default(),
            total_amount: None,
            receipt,
        }
    }
}

/// 映射档位报告。
///
/// # 参数
/// * `report` - 搜索报告
///
/// # 返回
/// 返回视图。
///
/// # 错误
/// 无。
fn tier_report_view(report: &TierSearchReport) -> TierReportView {
    TierReportView {
        tier_id: report.tier_id.clone(),
        expected_count: report.expected_count,
        actual_count: report.actual_count,
        stop_reason: report.stop_reason,
        stop_label: report.stop_reason.label().to_string(),
        image_failures: report.image_failures,
    }
}

/// 映射管理端陈列卡片。
///
/// # 参数
/// * `item` - 陈列项
/// * `booklet` - 选品册
///
/// # 返回
/// 返回卡片。
///
/// # 错误
/// 无。
fn display_item_view(item: &SalesSelectionDisplayItem, booklet: &SalesSelectionBooklet) -> DisplayItemView {
    let (name, specification, members, tier_id) = match &item.kind {
        DisplayKind::SingleSku { sku } => {
            (sku.name.clone(), sku.specification_attributes.clone(), Vec::new(), None)
        },
        DisplayKind::Package { members, tier_id, .. } => (
            members.iter().map(|sku| sku.name.as_str()).collect::<Vec<_>>().join(" / "),
            Vec::new(),
            members.iter().map(member_view).collect(),
            Some(tier_id.clone()),
        ),
    };
    let target_delta = tier_id.as_ref().and_then(|tier_id| {
        booklet
            .tiers
            .iter()
            .find(|tier| &tier.tier_id == tier_id)
            .map(|tier| abs_diff(item.price(), tier.target_amount))
    });
    let spec_label = specification
        .iter()
        .map(|attr| format!("{}：{}", attr.name, attr.value))
        .collect::<Vec<_>>()
        .join(" / ");
    let kind = match &item.kind {
        DisplayKind::SingleSku { .. } => "SINGLE_SKU",
        DisplayKind::Package { .. } => "PACKAGE",
    };
    let unit = match &item.kind {
        DisplayKind::SingleSku { sku } => Some(sku.unit.clone()),
        DisplayKind::Package { .. } => None,
    };
    let tier_name = tier_id.as_ref().and_then(|tier_id| {
        booklet.tiers.iter().find(|tier| &tier.tier_id == tier_id).map(|tier| tier.name.clone())
    });
    let cover = item.cover_asset_id().map(ToOwned::to_owned);
    DisplayItemView {
        id: item.base.id.clone(),
        item_id: item.base.id.clone(),
        kind: kind.to_string(),
        removed: item.removed,
        tier_id,
        tier_name,
        name,
        specification,
        spec_label,
        price: item.price(),
        price_gross: item.price(),
        target_delta,
        cover_asset_id: cover.clone(),
        cover_image: cover
            .map(|id| format!("/admin/sales-selection-books/{}/images?ref={id}", booklet.base.id)),
        unit,
        members,
        missing_image: item.cover_asset_id().is_none(),
    }
}

/// 映射成员。
///
/// # 参数
/// * `sku` - 成员快照
///
/// # 返回
/// 返回成员视图。
///
/// # 错误
/// 无。
fn member_view(sku: &SkuSnapshot) -> DisplayMemberView {
    DisplayMemberView {
        sku_id: sku.sku_id.to_string(),
        name: sku.name.clone(),
        specification: sku.specification_attributes.clone(),
        unit: sku.unit.clone(),
        price: sku.sales_visible_price_gross,
    }
}

/// 映射公开卡片。
///
/// # 参数
/// * `item` - 陈列
/// * `booklet` - 选品册
///
/// # 返回
/// 返回公开卡片。
///
/// # 错误
/// 无。
fn public_item(item: &SalesSelectionDisplayItem, booklet: &SalesSelectionBooklet) -> PublicDisplayItemView {
    let view = display_item_view(item, booklet);
    PublicDisplayItemView {
        item_id: view.id,
        tier_name: view.tier_id.as_ref().and_then(|tier_id| {
            booklet.tiers.iter().find(|tier| &tier.tier_id == tier_id).map(|tier| tier.name.clone())
        }),
        name: view.name,
        specification: view.specification,
        price: view.price,
        cover_path: view.cover_asset_id,
        members: view
            .members
            .into_iter()
            .map(|member| crate::dto::sales_selection::PublicDisplayMemberView {
                name: member.name,
                specification: member.specification,
                unit: member.unit,
                price: member.price,
            })
            .collect(),
    }
}

/// 映射公开回执。
///
/// # 参数
/// * `proposal` - 方案
/// * `display_lines` - 陈列行
///
/// # 返回
/// 返回回执。
///
/// # 错误
/// 无。
pub fn public_receipt(
    proposal: &SalesSelectionProposal,
    display_lines: &[SalesSelectionProposalDisplayLine],
) -> PublicReceiptView {
    PublicReceiptView {
        proposal_no: proposal.proposal_no.clone(),
        submitted_at: proposal.submitted_at,
        customer_name: proposal.customer_name.clone(),
        items: display_lines
            .iter()
            .map(|line| PublicChoiceView {
                item_id: line.display_item_id.to_string(),
                quantity: line.quantity,
                line_amount: line.line_amount,
            })
            .collect(),
        total_amount: proposal.total_amount,
    }
}

/// 公开页种类。
///
/// # 参数
/// * `booklet` - 选品册
/// * `now` - 服务端时间
///
/// # 返回
/// 返回页面种类。
///
/// # 错误
/// 无。
pub fn public_kind(
    booklet: &SalesSelectionBooklet,
    now: erp_core::common::time::Instant,
) -> PublicSelectionPageKind {
    let expired = booklet.is_expired(now);
    if booklet.status.public_is_ended(booklet.link_revoked, expired) {
        return PublicSelectionPageKind::Ended;
    }
    if booklet.status.public_is_receipt(booklet.link_revoked, expired) {
        return PublicSelectionPageKind::Receipt;
    }
    if booklet.status == BookletStatus::Published {
        PublicSelectionPageKind::Selecting
    } else {
        PublicSelectionPageKind::Ended
    }
}
