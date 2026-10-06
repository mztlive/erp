//! 内部供应商申请任务简报；逐提交展示冻结原稿，保留历史语义。

use std::collections::{HashMap, HashSet};

use erp_catalog::portal::{CatalogPortalExt, DraftSubmission, NewProductDraft};
use erp_core::common::time::Instant;
use erp_supplier::SupplierPaymentTerm;
use erp_supplier::portal::{CooperationApplication, CooperationRepository, FrozenSubmission};
use erp_supply::portal::{
    FrozenOfferingSubmission, OfferingApplication, OfferingApplicationSnapshot, PortalAvailabilityStatus,
    PortalSupplyExt,
};
use erp_workflow::WorkflowAuthorizationPort;
use persistence_core::Executor;

use super::authority::supplier_portal::offering_kind_label;
use super::brief::{
    BRIEF_LINE_LIMIT, BriefLine, ObjectBriefSource, format_instant_datetime, format_quantity,
    join_list_summary, push_section,
};
use super::{ObjectKind, WorkbenchObjectFactMap, WorkbenchReadService, WorkbenchSubjectDisplay, object_ids};
use crate::Result;

/// 只含允许展示的供应商及申请人姓名。
struct PortalDisplayContext {
    suppliers: HashMap<String, String>,
    applicants: HashMap<String, String>,
}

impl<A: WorkflowAuthorizationPort> WorkbenchReadService<A> {
    /// 为授权页补齐每次申请原稿的业务类型、原因、价格或新品规格摘要。
    ///
    /// # 参数
    /// * `keys` - 当前页任务精确对象键。
    /// * `facts` - 已读申请归属及冻结版本。
    /// * `executor` - 原页面读取执行器。
    /// # 返回
    /// 原地补齐冻结提交简报，历史不会采用编辑后的草稿。
    /// # 错误
    /// 本域仓储、供应商名称或申请人姓名读取失败时返回错误。
    pub(super) async fn load_supplier_portal_briefs(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut WorkbenchObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::SupplierPortalRequest);
        if ids.is_empty() {
            return Ok(());
        }
        let offerings = self.db.portal_applications().list_active_by_ids(&ids, executor).await?;
        let products = self.db.new_product_drafts().list_active_by_ids(&ids, executor).await?;
        let cooperation = CooperationRepository::new(&self.db).list_active_by_ids(&ids, executor).await?;
        let context = self
            .portal_display_context(
                &supplier_ids(&offerings, &products, &cooperation),
                &applicant_ids(&offerings, &products, &cooperation),
                executor,
            )
            .await?;
        apply_offerings(facts, &offerings, &context);
        apply_products(facts, &products, &context);
        apply_cooperation(facts, &cooperation, &context);
        Ok(())
    }

    /// 按有限对象键装载展示姓名；只向任务投影写入 display_name。
    async fn portal_display_context(
        &self,
        supplier_ids: &[String],
        applicant_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<PortalDisplayContext> {
        let suppliers = self.supplier_display_names(supplier_ids, executor).await?;
        let applicants = self
            .auth
            .load_accounts(applicant_ids, executor)
            .await?
            .into_iter()
            .map(|account| (account.id, account.display_name))
            .collect();
        Ok(PortalDisplayContext { suppliers, applicants })
    }
}

/// 三类申请统一解析往来方名称，不扩大授权对象集合。
fn supplier_ids(
    offerings: &[OfferingApplication],
    products: &[NewProductDraft],
    cooperation: &[CooperationApplication],
) -> Vec<String> {
    offerings
        .iter()
        .map(|row| row.supplier_id.clone())
        .chain(products.iter().map(|row| row.supplier_id.clone()))
        .chain(cooperation.iter().map(|row| row.supplier_id.clone()))
        .collect()
}

/// 每次冻结提交的申请人独立解析，编辑草稿不替换历史提交人。
fn applicant_ids(
    offerings: &[OfferingApplication],
    products: &[NewProductDraft],
    cooperation: &[CooperationApplication],
) -> Vec<String> {
    offerings
        .iter()
        .flat_map(|row| row.submissions.iter().map(|s| s.submitted_by.clone()))
        .chain(products.iter().flat_map(|row| row.submissions.iter().map(|s| s.submitted_by.clone())))
        .chain(cooperation.iter().flat_map(|row| row.submissions.iter().map(|s| s.submitted_by.clone())))
        .collect::<HashSet<_>>()
        .into_iter()
        .collect()
}

/// 供给报价及变更按提交编号写入冻结简报。
fn apply_offerings(
    facts: &mut WorkbenchObjectFactMap,
    requests: &[OfferingApplication],
    context: &PortalDisplayContext,
) {
    for request in requests {
        for submission in &request.submissions {
            apply(
                facts,
                &request.base.id,
                &request.supplier_id,
                &submission.submitted_by,
                format!("offering:{}", submission.submission_no),
                offering_source(submission),
                context,
            );
        }
    }
}

/// 新品原稿及规格按不可变提交标识写入简报。
fn apply_products(
    facts: &mut WorkbenchObjectFactMap,
    requests: &[NewProductDraft],
    context: &PortalDisplayContext,
) {
    for request in requests {
        for submission in &request.submissions {
            apply(
                facts,
                &request.base.id,
                &request.supplier_id,
                &submission.submitted_by,
                format!("new_product:{}", submission.id),
                product_source(submission),
                context,
            );
        }
    }
}

/// 合作条款只展示对应冻结商务申请。
fn apply_cooperation(
    facts: &mut WorkbenchObjectFactMap,
    requests: &[CooperationApplication],
    context: &PortalDisplayContext,
) {
    for request in requests {
        for submission in &request.submissions {
            apply(
                facts,
                &request.base.id,
                &request.supplier_id,
                &submission.submitted_by,
                format!("cooperation:{}", submission.submission_no),
                cooperation_source(submission),
                context,
            );
        }
    }
}

/// 将姓名及对应冻结简报写入独立 subject display；不更新权威事实。
fn apply(
    facts: &mut WorkbenchObjectFactMap,
    id: &str,
    supplier_id: &str,
    applicant_id: &str,
    subject: String,
    mut source: ObjectBriefSource,
    context: &PortalDisplayContext,
) {
    let Some(fact) = facts.get_mut(&(ObjectKind::SupplierPortalRequest, id.into())) else {
        return;
    };
    if !fact.authority.subject_versions.accepts(&subject) {
        return;
    }
    let supplier = context.suppliers.get(supplier_id).cloned();
    source.submitter_name = context.applicants.get(applicant_id).cloned();
    push_section(&mut source.extra_sections, "供应商", supplier.as_deref(), false);
    fact.display.counterparty_label = supplier.clone();
    let impact_summary =
        fact.authority.subject_briefs.get(&subject).and_then(|brief| brief.impact_summary.clone());
    fact.display.subject_briefs.insert(
        subject,
        WorkbenchSubjectDisplay { counterparty_label: supplier, impact_summary, brief_source: Some(source) },
    );
}

/// 基础简报仅使用本次冻结提交事实。
fn source(kind: &str, reason: &str, submitted_at: Option<Instant>) -> ObjectBriefSource {
    let mut result = ObjectBriefSource::default();
    push_section(&mut result.extra_sections, "申请类型", Some(kind), false);
    push_section(&mut result.extra_sections, "申请原因", Some(reason), false);
    let submitted_at = submitted_at.map(format_instant_datetime);
    push_section(&mut result.extra_sections, "提交时间", submitted_at.as_deref(), false);
    result.list_summary = join_list_summary([Some(kind.into()), Some(reason.into())]);
    result
}

/// 报价只展示提交的拟生效值；不会被描述为当前正式价格。
fn offering_source(submission: &FrozenOfferingSubmission) -> ObjectBriefSource {
    let mut result = source(
        offering_kind_label(submission.snapshot.kind()),
        &submission.reason,
        Some(submission.submitted_at),
    );
    let terms = match &submission.snapshot {
        OfferingApplicationSnapshot::ExistingQuote {
            supplier_sku_code,
            terms,
            availability_status,
            available_quantity,
            availability_reported_at,
            ..
        } => {
            push_section(&mut result.extra_sections, "供应商订货编码", Some(supplier_sku_code), false);
            append_initial_availability(
                &mut result,
                *availability_status,
                available_quantity.as_deref(),
                *availability_reported_at,
            );
            Some(terms)
        },
        OfferingApplicationSnapshot::TermsChange { terms, .. } => Some(terms),
        OfferingApplicationSnapshot::StopSupply { .. } => None,
    };
    if let Some(terms) = terms {
        push_section(
            &mut result.extra_sections,
            "申请代发含税价",
            Some(&terms.dropship_supply_price_gross),
            true,
        );
        push_section(
            &mut result.extra_sections,
            "申请集采含税价",
            Some(&terms.bulk_supply_price_gross),
            true,
        );
        push_section(&mut result.extra_sections, "申请进项税率", Some(&terms.input_tax_rate), true);
        push_section(
            &mut result.extra_sections,
            "集采起订量",
            Some(&terms.bulk_minimum_order_quantity),
            true,
        );
    }
    result
}

/// 初始供给填报只取提交原稿中的实际状态、可选数量与填报时间。
fn append_initial_availability(
    result: &mut ObjectBriefSource,
    status: PortalAvailabilityStatus,
    quantity: Option<&str>,
    reported_at: Instant,
) {
    let status = match status {
        PortalAvailabilityStatus::Available => "有货",
        PortalAvailabilityStatus::OutOfStock => "临时缺货",
    };
    push_section(&mut result.extra_sections, "初始可供状态", Some(status), false);
    push_section(&mut result.extra_sections, "初始可供数量", quantity, true);
    push_section(
        &mut result.extra_sections,
        "实际填报时间",
        Some(&format_instant_datetime(reported_at)),
        false,
    );
}

/// 新品简报保留供应商名称、原始字典和可供填报，规格行数有界。
fn product_source(submission: &DraftSubmission) -> ObjectBriefSource {
    let mut result = source("新品提报", "新品资料与首次供给待采购确认", Some(submission.submitted_at));
    push_section(&mut result.extra_sections, "商品名称", Some(&submission.input.name), false);
    push_section(&mut result.extra_sections, "品牌原稿", Some(&submission.input.brand.raw_name), false);
    push_section(&mut result.extra_sections, "分类原稿", Some(&submission.input.category.raw_name), false);
    push_section(
        &mut result.extra_sections,
        "销售上架",
        Some("新建 SKU 未上架，销售定价及上架另行执行"),
        false,
    );
    result.lines = submission
        .input
        .skus
        .iter()
        .take(BRIEF_LINE_LIMIT)
        .map(|sku| BriefLine {
            title: sku.name.clone(),
            quantity: sku
                .available_quantity
                .as_ref()
                .map(|quantity| format!("可供 {}", format_quantity(quantity, Some(&sku.unit.raw_name)))),
            due_label: None,
        })
        .collect();
    result.more_count =
        u32::try_from(submission.input.skus.len().saturating_sub(BRIEF_LINE_LIMIT)).unwrap_or(u32::MAX);
    result.list_summary = join_list_summary([
        Some("新品提报".into()),
        Some(submission.input.name.clone()),
        Some(format!("{} 个规格", submission.input.skus.len())),
    ]);
    result
}

/// 合作条款属于供应商商务档案，独立展示后续采购影响。
fn cooperation_source(submission: &FrozenSubmission) -> ObjectBriefSource {
    let at =
        i64::try_from(submission.submitted_at).ok().and_then(|value| Instant::try_from_unix_secs(value).ok());
    let mut result = source("合作条款变更", &submission.proposal.reason, at);
    push_section(
        &mut result.extra_sections,
        "申请结算方式",
        Some(submission.proposal.settlement_mode.label()),
        false,
    );
    push_section(
        &mut result.extra_sections,
        "申请对账周期",
        Some(submission.proposal.reconciliation_cycle.label()),
        false,
    );
    let payment_term = SupplierPaymentTerm::parse(&submission.proposal.payment_term)
        .map(|term| term.label())
        .unwrap_or_else(|_| "付款条件待核对".into());
    push_section(&mut result.extra_sections, "申请付款条件", Some(&payment_term), false);
    push_section(
        &mut result.extra_sections,
        "影响范围",
        Some("后续采购使用新商务档案，已冻结采购单付款条件保持不变"),
        false,
    );
    result
}

#[cfg(test)]
mod tests {
    use erp_catalog::portal::{NewProductDraft, NewProductInput};
    use erp_workflow::entity::work_item::WorkItemSubjectVersions;
    use erp_workflow::ports::ObjectFact;
    use serde_json::json;

    use super::super::WorkbenchObjectFact;
    use super::super::authority::supplier_portal::new_product_request_fact;
    use super::*;

    /// 构造可比较的供应商原稿，四个不同规格保留零值与未知数量。
    fn submission() -> DraftSubmission {
        let input: NewProductInput = serde_json::from_value(json!({
            "name": "供应商原稿饮料", "product_kind": "PHYSICAL",
            "brand": { "raw_name": "原稿品牌", "selected_id": null, "expected_version": null },
            "category": { "raw_name": "食品/饮料", "selected_id": null, "expected_version": null },
            "model": null, "description": null, "image_asset_ids": [], "file_asset_ids": [],
            "skus": (0..4).map(|index| json!({
                "row_id": format!("row-{index}"), "name": format!("原稿规格 {index}"),
                "spec_entries": [{"attribute_code":"容量", "attribute_value_code": format!("{} ml", index + 1)}],
                "unit": {"raw_name":"瓶", "selected_id": null, "expected_version": null},
                "barcode": null, "image_asset_id": null, "ordering_code": format!("code-{index}"),
                "supply_terms": {}, "available_quantity": if index == 0 { Some("0") } else { None },
                "reported_at": 100,
            })).collect::<Vec<_>>()
        })).unwrap();
        DraftSubmission {
            id: "submission-1".into(),
            input,
            submitted_by: "external".into(),
            submitted_at: Instant::from_unix_secs(200),
            task_id: "task".into(),
            decision: None,
        }
    }

    #[test]
    fn product_brief_keeps_raw_snapshot_unknown_quantity_and_unlisted_semantics() {
        let submission = submission();
        let source = product_source(&submission);
        assert!(
            source
                .extra_sections
                .iter()
                .any(|section| section.label == "商品名称" && section.value == "供应商原稿饮料")
        );
        assert!(
            source
                .extra_sections
                .iter()
                .any(|section| section.label == "销售上架" && section.value.contains("未上架"))
        );
        assert_eq!(source.lines.len(), BRIEF_LINE_LIMIT);
        assert_eq!(source.more_count, 1);
        assert_eq!(source.lines[0].quantity.as_deref(), Some("可供 0 瓶"));
        assert!(source.lines[1].quantity.is_none());
        assert!(source.amount_label.is_none());
    }

    #[test]
    fn product_history_uses_frozen_submission_after_current_draft_changes() {
        let submission = submission();
        let mut request = NewProductDraft::new(
            "request".into(),
            "supplier".into(),
            "external".into(),
            submission.input.clone(),
        )
        .unwrap();
        request.submissions.push(submission.clone());
        request.draft.name = "编辑后的草稿".into();
        let authority = new_product_request_fact(&request).unwrap().unwrap();
        assert!(authority.subject_versions.accepts("new_product:submission-1"));
        assert_eq!(authority.created_by, "external");
        let mut facts = WorkbenchObjectFactMap::from([(
            (ObjectKind::SupplierPortalRequest, "request".into()),
            WorkbenchObjectFact::from_authority(authority),
        )]);
        let context = PortalDisplayContext {
            suppliers: HashMap::from([("supplier".into(), "供应商甲".into())]),
            applicants: HashMap::from([("external".into(), "供应商提交人".into())]),
        };
        apply_products(&mut facts, &[request], &context);
        let source = facts[&(ObjectKind::SupplierPortalRequest, "request".into())].display.subject_briefs["new_product:submission-1"].brief_source.as_ref().unwrap();
        assert!(source.list_summary.contains("供应商原稿饮料"));
        assert!(!source.list_summary.contains("编辑后的草稿"));
        assert_eq!(source.submitter_name.as_deref(), Some("供应商提交人"));
    }

    #[test]
    fn subject_brief_cannot_be_attached_to_a_different_frozen_version() {
        let mut authority = ObjectFact::new("request", "申请", "external");
        authority.subject_versions = WorkItemSubjectVersions::constrained(vec!["offering:1".into()]).unwrap();
        let mut facts = WorkbenchObjectFactMap::from([(
            (ObjectKind::SupplierPortalRequest, "request".into()),
            WorkbenchObjectFact::from_authority(authority),
        )]);
        let context = PortalDisplayContext { suppliers: HashMap::new(), applicants: HashMap::new() };
        apply(
            &mut facts,
            "request",
            "supplier",
            "external",
            "offering:2".into(),
            ObjectBriefSource::default(),
            &context,
        );
        assert!(
            facts[&(ObjectKind::SupplierPortalRequest, "request".into())].display.subject_briefs.is_empty()
        );
    }
}
