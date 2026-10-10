//! 待审核与待付款申请使用同一真实申请身份，精确版本约束不引用旧应付任务。

use std::collections::HashSet;

use erp_finance::entity::payable::{PurchasePaymentRequest, PurchasePaymentRequestLine};
use erp_finance::repository::PurchasePaymentRequestExt;
use erp_workflow::entity::approval_integration::display_snapshot::{ApprovalBriefLine, ApprovalBriefSource};
use erp_workflow::entity::work_item::WorkItemSubjectVersions;
use erp_workflow::ports::{ObjectFact, ObjectFactMap, ObjectKind};
use persistence_core::Executor;

use super::authority::WorkItemFactsReader;
use super::brief::{BRIEF_LINE_LIMIT, push_section};
use super::presentation::format_yuan;
use super::{WorkbenchObjectFact, WorkbenchObjectFactMap, WorkbenchReadService, object_ids};
use crate::Result;

impl WorkItemFactsReader {
    /// 批量装载付款申请命令与任务授权共用的精确事实。
    ///
    /// # 参数
    /// * `keys` / `facts` / `executor` - 当前对象集合、输出与原执行器。
    /// # 返回
    /// 写入申请身份、申请人和提交版本约束。
    /// # 错误
    /// 仓储失败或版本不能证明时拒绝。
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
        for request in self.db.purchase_payment_requests().list_active_by_ids(&ids, executor).await? {
            facts.insert(
                (ObjectKind::PurchasePaymentRequest, request.base.id.clone()),
                request_fact(&request)?,
            );
        }
        Ok(())
    }
}

impl<A: erp_workflow::WorkflowAuthorizationPort> WorkbenchReadService<A> {
    /// 工作台直接显示希望付款日、主体、脱敏账户和逐采购批准金额。
    ///
    /// # 参数
    /// * `keys` / `facts` / `executor` - 本批对象、输出和原读取事务。
    /// # 返回
    /// 写入资金申请摘要；审批队列另由不可变提交快照覆盖。
    /// # 错误
    /// 申请或主体版本读取失败时拒绝。
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
        let users = requests.iter().map(|request| request.created_by.clone()).collect::<Vec<_>>();
        let names = self.load_account_names(&users, executor).await?;
        for request in requests {
            let mut fact = WorkbenchObjectFact::from_authority(request_fact(&request)?);
            fact.display.brief_source =
                Some(request_brief(&request, names.get(&request.created_by).cloned()));
            fact.display.approval_subject_version = Some(request.submission_version);
            facts.insert((ObjectKind::PurchasePaymentRequest, request.base.id.clone()), fact);
        }
        Ok(())
    }
}

fn request_fact(request: &PurchasePaymentRequest) -> Result<ObjectFact> {
    let mut fact =
        ObjectFact::new(&request.base.id, format!("付款申请 {}", request.request_no), &request.created_by);
    fact.subject_versions =
        WorkItemSubjectVersions::constrained(vec![request.submission_version.to_string()])?;
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
