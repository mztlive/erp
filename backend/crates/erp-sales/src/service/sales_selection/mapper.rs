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
    SalesSelectionSession, SelectionForm, SkuSnapshot, SubmitMode, TierSearchReport, abs_diff,
};

/// 两处选品册视图共用的表头字段组（新旧兼容双字段由本结构一次搬运）。
struct BookletHead {
    /// 身份（`id` 与兼容字段 `book_id` 同值）。
    id: String,
    /// 版本。
    version: u64,
    /// 客户。
    customer_id: String,
    /// 客户名称。
    customer_name: String,
    /// 显式销售负责人。
    sales_owner_user_id: String,
    /// 业务组织。
    business_org_unit_id: String,
    /// 形态（`form` 与兼容字段 `selection_form` 同值）。
    form: SelectionForm,
    /// 提交方式。
    submit_mode: SubmitMode,
    /// 状态。
    status: BookletStatus,
}

/// 一次搬运两处视图共用的选品册表头（含新旧兼容双字段）。
fn booklet_head(booklet: &SalesSelectionBooklet) -> BookletHead {
    BookletHead {
        id: booklet.base.id.clone(),
        version: booklet.base.version,
        customer_id: booklet.customer_id.to_string(),
        customer_name: booklet.customer_name.clone(),
        sales_owner_user_id: booklet.sales_owner_user_id.clone(),
        business_org_unit_id: booklet.business_org_unit_id.clone(),
        form: booklet.form,
        submit_mode: booklet.submit_mode,
        status: booklet.status,
    }
}

/// 单遍统计陈列计数（可展示数、已删数、缺图数）。
///
/// 一次遍历同时累加三个计数；缺图仅统计可展示且无封面资产的行。
fn booklet_item_counts(items: &[SalesSelectionDisplayItem]) -> (u32, u32, u32) {
    items.iter().fold((0, 0, 0), |(display, removed, missing_image), item| {
        let publishable = item.is_publishable();
        (
            display + u32::from(publishable),
            removed + u32::from(item.removed),
            missing_image + u32::from(publishable && item.cover_asset_id().is_none()),
        )
    })
}

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
        let head = booklet_head(booklet);
        SalesSelectionBookletListItemView {
            book_id: head.id.clone(),
            id: head.id,
            version: head.version,
            customer_id: head.customer_id,
            customer_name: head.customer_name,
            sales_owner_user_id: head.sales_owner_user_id,
            business_org_unit_id: head.business_org_unit_id,
            selection_form: head.form,
            form: head.form,
            submit_mode: head.submit_mode,
            access_password_set: booklet.access_password_hash.is_some(),
            per_person_budget: booklet.per_person_budget,
            voucher_count: booklet.voucher_count,
            status: head.status,
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
        let (display_count, removed_count, missing_image_count) = booklet_item_counts(items);
        let head = booklet_head(booklet);
        SalesSelectionBookletView {
            pool_filter: booklet.pool_source.filter.clone(),
            sku_ids: booklet.pool_source.sku_ids.iter().flatten().map(ToString::to_string).collect(),
            book_id: head.id.clone(),
            id: head.id,
            version: head.version,
            customer_id: head.customer_id,
            customer_name: head.customer_name,
            sales_owner_user_id: head.sales_owner_user_id,
            sales_owner_name: None,
            business_org_unit_id: head.business_org_unit_id,
            selection_form: head.form,
            form: head.form,
            submit_mode: head.submit_mode,
            access_password_set: booklet.access_password_hash.is_some(),
            per_person_budget: booklet.per_person_budget,
            voucher_count: booklet.voucher_count,
            status: head.status,
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
            participant_id: proposal.participant_id.clone(),
            recipient: proposal.recipient.clone(),
            submitted_at: proposal.submitted_at,
            source: proposal.source,
            total_amount: proposal.total_amount,
            display_lines: display_lines.iter().map(proposal_display_line_view).collect(),
            sku_lines: sku_lines.iter().map(proposal_sku_line_view).collect(),
        }
    }

    /// 映射方案列表行。
    ///
    /// # 参数
    /// * `proposal` - 表头
    ///
    /// # 返回
    /// 返回不含个人收件信息的列表行。
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
            participant_id: proposal.participant_id.clone(),
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
        if matches!(kind, PublicSelectionPageKind::Ended | PublicSelectionPageKind::Locked) {
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
                voucher_required: kind == PublicSelectionPageKind::Locked
                    && booklet.submit_mode == SubmitMode::PickupVoucher,
                per_person_budget: None,
                participant_id: None,
                recipient: None,
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
            voucher_required: booklet.submit_mode == SubmitMode::PickupVoucher,
            per_person_budget: booklet.per_person_budget,
            participant_id: session.map(|item| item.participant_id.clone()),
            recipient: session.and_then(|item| item.recipient.clone()),
        }
    }
}

/// 映射方案陈列行。
///
/// # 参数
/// * `line` - 方案陈列行
///
/// # 返回
/// 返回行视图。
///
/// # 错误
/// 无。
fn proposal_display_line_view(line: &SalesSelectionProposalDisplayLine) -> ProposalDisplayLineView {
    ProposalDisplayLineView {
        display_item_id: line.display_item_id.to_string(),
        tier_id: line.tier_id.clone(),
        quantity: line.quantity,
        unit_price: line.unit_price,
        line_amount: line.line_amount,
        cover_asset_id: line.cover_asset_id.clone(),
    }
}

/// 映射方案 SKU 行。
///
/// # 参数
/// * `line` - 方案 SKU 行
///
/// # 返回
/// 返回行视图。
///
/// # 错误
/// 无。
fn proposal_sku_line_view(line: &SalesSelectionProposalSkuLine) -> ProposalSkuLineView {
    ProposalSkuLineView {
        display_item_id: line.display_item_id.to_string(),
        name: line.name.clone(),
        specification: line.specification.clone(),
        unit: line.unit.clone(),
        quantity: line.quantity,
        unit_price: line.unit_price,
        line_amount: line.line_amount,
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
    let cover = item.cover_asset_id();
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
        cover_image: cover
            .map(|id| format!("/admin/sales-selection-books/{}/images?ref={id}", booklet.base.id)),
        cover_asset_id: cover.map(ToOwned::to_owned),
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

#[cfg(test)]
mod tests {
    use erp_core::common::time::Instant;
    use erp_core::ids::{
        CustomerAccountId, SalesSelectionBookletId, SalesSelectionDisplayItemId, SalesSelectionProposalId,
        SalesSelectionSessionId,
    };

    use super::*;
    use crate::entity::sales_selection::{
        PoolFilterSnapshot, PoolSource, PoolSourceKind, SalesSelectionBookletData,
        SalesSelectionProposalData, SelectionRecipient, SessionChoice,
    };

    fn published_booklet() -> SalesSelectionBooklet {
        let mut book = SalesSelectionBooklet::new(
            SalesSelectionBookletId::new("book"),
            SalesSelectionBookletData {
                customer_id: CustomerAccountId::new("customer"),
                customer_no: "C1".into(),
                customer_name: "客户甲".into(),
                sales_owner_user_id: "sales".into(),
                business_org_unit_id: "org".into(),
                form: SelectionForm::SingleSku,
                submit_mode: SubmitMode::PickupVoucher,
                access_password_hash: Some("password-hash".into()),
                per_person_budget: Some("50.00".parse().unwrap()),
                voucher_count: Some(2),
                pool_source: PoolSource::new(
                    PoolSourceKind::Filter,
                    Some(PoolFilterSnapshot::default()),
                    None,
                )
                .unwrap(),
                tiers: Vec::new(),
                created_by: "sales".into(),
            },
        )
        .unwrap();
        book.status = BookletStatus::Published;
        book.link_expires_at = Some(Instant::from_unix_secs(200));
        book
    }

    #[test]
    fn locked_and_ended_views_redact_session_receipt_and_address() {
        let book = published_booklet();
        let mut session = SalesSelectionSession::new(
            SalesSelectionSessionId::new("session"),
            SalesSelectionBookletId::new("book"),
        );
        session.participant_id = "person".into();
        session.choices = vec![SessionChoice {
            display_item_id: SalesSelectionDisplayItemId::new("item"),
            quantity: Some(2),
        }];
        session.recipient = Some(SelectionRecipient {
            name: "张三".into(),
            phone: "13800138000".into(),
            province: "浙江省".into(),
            city: "杭州市".into(),
            district: "西湖区".into(),
            address: "文三路1号".into(),
        });
        let receipt = PublicReceiptView {
            proposal_no: "XP1".into(),
            submitted_at: Instant::from_unix_secs(100),
            customer_name: "客户甲".into(),
            items: vec![PublicChoiceView {
                item_id: "item".into(),
                quantity: Some(2),
                line_amount: Some("50.00".parse().unwrap()),
            }],
            total_amount: Some("50.00".parse().unwrap()),
        };
        for kind in [PublicSelectionPageKind::Locked, PublicSelectionPageKind::Ended] {
            let view =
                SalesSelectionService::public_page(kind, &book, &[], Some(&session), Some(receipt.clone()));
            assert!(view.items.is_empty() && view.choices.is_empty());
            assert!(view.customer_name.is_none() && view.submit_mode.is_none());
            assert!(view.participant_id.is_none() && view.recipient.is_none() && view.receipt.is_none());
            assert!(view.total_amount.is_none() && view.per_person_budget.is_none());
        }
    }

    #[test]
    fn close_revoke_and_expiry_end_public_pages() {
        let mut book = published_booklet();
        assert_eq!(public_kind(&book, Instant::from_unix_secs(100)), PublicSelectionPageKind::Selecting);
        assert_eq!(public_kind(&book, Instant::from_unix_secs(200)), PublicSelectionPageKind::Ended);
        book.link_revoked = true;
        assert_eq!(public_kind(&book, Instant::from_unix_secs(100)), PublicSelectionPageKind::Ended);
        book.link_revoked = false;
        book.status = BookletStatus::Closed;
        assert_eq!(public_kind(&book, Instant::from_unix_secs(100)), PublicSelectionPageKind::Ended);
    }

    #[test]
    fn proposal_list_omits_recipient_while_detail_preserves_snapshot() {
        let recipient = SelectionRecipient {
            name: "张三".into(),
            phone: "13800138000".into(),
            province: "浙江省".into(),
            city: "杭州市".into(),
            district: "西湖区".into(),
            address: "文三路1号".into(),
        };
        let proposal = SalesSelectionProposal::new(
            SalesSelectionProposalId::new("proposal-1"),
            SalesSelectionProposalData {
                proposal_no: "XP1".into(),
                customer_id: CustomerAccountId::new("customer-1"),
                customer_name: "客户甲".into(),
                booklet_id: SalesSelectionBookletId::new("book-1"),
                sales_owner_user_id: "sales-1".into(),
                business_org_unit_id: "org-1".into(),
                batch_id: "batch-1".into(),
                form: SelectionForm::SingleSku,
                submit_mode: SubmitMode::PickupVoucher,
                participant_id: "person-1".into(),
                recipient: Some(recipient.clone()),
                session_version: 2,
                submitted_at: Instant::from_unix_secs(100),
                total_amount: Some("50.00".parse().unwrap()),
            },
        )
        .unwrap();
        let list = serde_json::to_value(SalesSelectionService::proposal_list_item(&proposal)).unwrap();
        assert!(list.get("recipient").is_none());
        assert_eq!(list["participant_id"], "person-1");
        let list_json = serde_json::to_string(&list).unwrap();
        let recipient_json = serde_json::to_value(&recipient).unwrap();
        for value in recipient_json.as_object().unwrap().values() {
            assert!(!list_json.contains(value.as_str().unwrap()));
        }
        let detail = serde_json::to_value(SalesSelectionService::proposal_view(&proposal, &[], &[])).unwrap();
        assert_eq!(detail["recipient"], recipient_json);
    }
}
