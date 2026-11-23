//! 待审核与待付款申请使用同一真实申请身份，精确版本约束不引用旧应付任务。

use std::collections::{HashMap, HashSet};

use bpm::ApprovalProcessInstanceId;
use erp_finance::entity::payable::{PurchasePaymentRequest, PurchasePaymentRequestLine};
use erp_finance::repository::PurchasePaymentRequestExt;
use erp_workflow::entity::approval_integration::display_snapshot::{ApprovalBriefLine, ApprovalBriefSource};
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::entity::work_item::WorkItemSubjectVersions;
use erp_workflow::ports::{ObjectFact, ObjectFactMap, ObjectKind};
use erp_workflow::repository::{ApprovalIntegrationExt, ApprovalSubjectSnapshotRepositoryExt};
use persistence_core::Executor;

use super::authority::WorkItemFactsReader;
use super::brief::{BRIEF_LINE_LIMIT, push_section};
use super::presentation::format_yuan;
use super::{WorkbenchFactsReader, WorkbenchObjectFact, WorkbenchObjectFactMap, object_ids};
use crate::Result;

impl WorkItemFactsReader {
    // 读取付款申请工作台时，批量装载付款申请命令与任务授权共用的精确事实。
    //
    // 调用参数：
    // * `keys` / `facts` / `executor` - 当前对象集合、输出与原执行器。
    //
    // 写入申请身份、申请人和提交版本约束。
    //
    // 仓储失败或版本不能证明时拒绝。
    pub(in crate::workbench) async fn load_purchase_payment_request_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::PurchasePaymentRequest);
        if ids.is_empty() {
            return Ok(());
        }
        let history = request_history(&self.db, &ids, executor).await?;
        for request in self.db.purchase_payment_requests().list_active_by_ids(&ids, executor).await? {
            facts.insert(
                (ObjectKind::PurchasePaymentRequest, request.base.id.clone()),
                request_fact(&request, history.get(&request.base.id).map(Vec::as_slice).unwrap_or_default())?,
            );
        }
        Ok(())
    }
}

impl WorkbenchFactsReader<'_> {
    // 读取付款申请工作台时，工作台直接显示希望付款日、主体、脱敏账户和逐采购批准金额。
    //
    // 调用参数：
    // * `keys` / `facts` / `executor` - 本批对象、输出和原读取事务。
    //
    // 写入资金申请摘要；审批队列另由不可变提交快照覆盖。
    //
    // 申请或主体版本读取失败时拒绝。
    pub(super) async fn load_purchase_payment_request_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut WorkbenchObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::PurchasePaymentRequest);
        if ids.is_empty() {
            return Ok(());
        }

        let requests = self.db.purchase_payment_requests().list_active_by_ids(&ids, executor).await?;
        let history = request_history(self.db, &ids, executor).await?;
        let users = requests.iter().map(|request| request.created_by.clone()).collect::<Vec<_>>();
        let names = self.load_account_names(&users, executor).await?;

        for request in requests {
            let mut fact = WorkbenchObjectFact::from_authority(request_fact(
                &request,
                history.get(&request.base.id).map(Vec::as_slice).unwrap_or_default(),
            )?);
            fact.display.brief_source =
                Some(request_brief(&request, names.get(&request.created_by).cloned()));
            fact.display.approval_subject_version = Some(request.submission_version);
            facts.insert((ObjectKind::PurchasePaymentRequest, request.base.id.clone()), fact);
        }
        Ok(())
    }
}

// 处理付款申请工作台的版本事实时，历史任务仅引用当前付款申请已冻结的实例及提交版本，不读取当前草稿替代历史。
async fn request_history(
    db: &mongodb::Database,
    ids: &[String],
    executor: &mut dyn Executor,
) -> Result<HashMap<String, Vec<(ApprovalProcessInstanceId, u32)>>> {
    let objects = ids.iter().map(|id| (DocumentType::PurchasePaymentRequest, id.clone())).collect::<Vec<_>>();
    let mut history: HashMap<String, Vec<(ApprovalProcessInstanceId, u32)>> = HashMap::new();
    for snapshot in db.approval_subject_snapshots().list_by_business_objects(&objects, executor).await? {
        history
            .entry(snapshot.business_object_id)
            .or_default()
            .push((snapshot.approval_process_instance_id, snapshot.subject_version));
    }
    Ok(history)
}

fn request_fact(
    request: &PurchasePaymentRequest,
    history: &[(ApprovalProcessInstanceId, u32)],
) -> Result<ObjectFact> {
    let mut fact =
        ObjectFact::new(&request.base.id, format!("付款申请 {}", request.request_no), &request.created_by);
    fact.subject_versions =
        WorkItemSubjectVersions::constrained(vec![request.submission_version.to_string()])?;
    for (instance_id, version) in history {
        fact.subject_versions.register_rework_history(instance_id, *version);
    }

    fact.push_counterparty_id(request.data.supplier_id.clone());
    fact.counterparty_label = supplier_name(request);
    fact.impact_summary = Some(format!(
        "申请 {} · 待付 {}",
        format_yuan(&request.total_amount),
        format_yuan(&request.remaining())
    ));
    Ok(fact)
}

fn supplier_name(request: &PurchasePaymentRequest) -> Option<String> {
    request.lines.first()?.source_snapshot.purchase_preview.get("supplier_name")?.as_str().map(str::to_owned)
}

fn request_brief(request: &PurchasePaymentRequest, submitter_name: Option<String>) -> ApprovalBriefSource {
    let mut sections = Vec::new();
    push_section(&mut sections, "供应商", supplier_name(request).as_deref(), false);
    push_section(&mut sections, "我方付款主体", Some(&request.payer_party_name), false);
    push_section(&mut sections, "付款方式", Some(request.data.execution_method.label()), false);
    push_section(&mut sections, "待付金额", Some(&format_yuan(&request.remaining())), true);
    push_section(&mut sections, "申请金额", Some(&format_yuan(&request.total_amount)), true);
    push_section(&mut sections, "希望付款日", Some(&request.data.desired_payment_date.to_string()), false);
    push_section(&mut sections, "申请说明", Some(&request.data.reason), false);

    if let Some(account) = &request.payee_account {
        push_section(
            &mut sections,
            "收款账户",
            Some(&format!(
                "{} · {} · {}",
                account.account_name, account.bank_name, account.masked_account_number
            )),
            false,
        );
    }

    let lines = request.lines.iter().take(BRIEF_LINE_LIMIT).map(brief_line).collect();
    ApprovalBriefSource {
        customer: None,
        amount_label: None,
        lines,
        more_count: u32::try_from(request.lines.len().saturating_sub(BRIEF_LINE_LIMIT)).unwrap_or(u32::MAX),
        submitter_name,
        list_summary: format!(
            "{} · {} · {}",
            supplier_name(request).unwrap_or_default(),
            format_yuan(&request.total_amount),
            request.data.desired_payment_date
        ),
        extra_sections: sections,
    }
}

fn brief_line(line: &PurchasePaymentRequestLine) -> ApprovalBriefLine {
    ApprovalBriefLine {
        title: line
            .source_snapshot
            .purchase_preview
            .get("purchase_no")
            .and_then(|value| value.as_str())
            .unwrap_or("采购单号未保存")
            .into(),
        quantity: Some(format!(
            "本次申请 {} · 已付 {} · 待付 {}",
            format_yuan(&line.requested_amount),
            format_yuan(&line.paid_amount),
            format_yuan(&line.remaining())
        )),
        due_label: None,
    }
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::BusinessDate;
    use erp_finance::entity::payable::{PaymentRequestLineData, PurchasePaymentRequestData};
    use serde_json::json;

    use super::*;

    #[test]
    fn payment_brief_uses_remaining_after_paid_and_closed_amounts() {
        let mut request = PurchasePaymentRequest::new(
            "request".into(),
            PurchasePaymentRequestData {
                supplier_id: "supplier".into(),
                payee_bank_account_id: "bank".into(),
                execution_method: Default::default(),
                lines: vec![PaymentRequestLineData {
                    payable_account_id: "account".into(),
                    amount: "100.01".parse().unwrap(),
                }],
                desired_payment_date: BusinessDate::from_ymd(2026, 11, 6).unwrap(),
                reason: "采购付款".into(),
                attachment_ids: vec![],
            },
            "buyer",
        )
        .unwrap();
        request.payer_party_name = "付款公司".into();
        request.lines = vec![serde_json::from_value(json!({
            "payable_account_id": "account", "purchase_order_id": "purchase", "purchase_revision_id": "revision",
            "requested_amount": "100.01", "paid_amount": "30.01", "closed_amount": "20.00",
            "gross_total": "100.01", "settled_total": "30.01", "open_total": "70.00", "other_reserved_total": "0",
            "payment_terms": {}, "source_snapshot": {"purchase_preview": {}, "sales_sources": [], "allowed_attachment_ids": []}
        })).unwrap()];

        let brief = request_brief(&request, None);
        let field = brief.extra_sections.iter().find(|field| field.label == "待付金额").unwrap();
        assert_eq!(field.value, format_yuan(&"50.00".parse().unwrap()));
        assert!(field.numeric);
        assert!(
            brief
                .extra_sections
                .iter()
                .any(|field| field.label == "我方付款主体" && field.value == "付款公司")
        );

        request.lines[0].closed_amount = "70.00".parse().unwrap();
        let brief = request_brief(&request, None);
        assert_eq!(
            brief.extra_sections.iter().find(|field| field.label == "待付金额").unwrap().value,
            format_yuan(&"0".parse().unwrap())
        );
    }
}
