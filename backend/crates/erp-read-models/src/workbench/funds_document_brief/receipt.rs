//! 回款、客户退款与回款冲正任务简报。

use std::collections::HashSet;

use persistence_core::Executor;

use super::super::brief::{format_instant_date, join_list_summary, non_empty};
use super::super::presentation::format_yuan;
use super::super::WorkbenchReadService;
use super::super::{object_ids, ObjectKind, WorkbenchObjectFact, WorkbenchObjectFactMap as ObjectFactMap};
use super::mapping::{
    amount_reason_brief, append_funds_origin, funds_fact_display, receipt_brief_source, select_funds_origin,
};
use crate::errors::Result;
use crate::workbench::authority::funds::mapping as authority_mapping;

impl<A: erp_workflow::WorkflowAuthorizationPort> WorkbenchReadService<A> {
    /// 回款审批任务的对象事实：任务对象是回款单本身。
    ///
    /// 回款单实体不记录创建人，创建操作人从 `customer_receipt.create` 审计事实取，
    /// 缺失时为空参与权（创建人无管理权限时不得仅凭创建事实看到任务）。
    ///
    /// # 参数
    /// * `keys` - 本批任务引用的对象键
    /// * `facts` - 输出的对象事实表
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 成功时写入回款单号、往来主体、金额和待核销销售单。
    ///
    /// # 错误
    /// 仓储查询失败时返回错误。
    pub(crate) async fn load_customer_receipt_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::CustomerReceipt);
        if ids.is_empty() {
            return Ok(());
        }
        let receipts = self.funds_reader().read_customer_receipts(&ids, executor).await?;
        if receipts.is_empty() {
            return Ok(());
        }
        let created_by = self
            .load_created_by_from_audit(
                "customer_receipt",
                &ids.iter().cloned().collect::<HashSet<_>>(),
                executor,
            )
            .await?;
        let party_ids = receipts
            .iter()
            .map(|item| item.counterparty_party_id.to_string())
            .collect::<Vec<_>>();
        let party_names = self.party_legal_names(&party_ids, executor).await?;
        let allocation_lines = self.receipt_allocation_lines(&receipts, executor).await?;
        for receipt in receipts {
            let counterparty = party_names
                .get(&receipt.counterparty_party_id.to_string())
                .cloned();
            let lines = allocation_lines
                .get(&receipt.base.id)
                .cloned()
                .unwrap_or_default();
            let mut fact = WorkbenchObjectFact::from_authority(authority_mapping::customer_receipt_fact(
                &receipt,
                created_by.get(&receipt.base.id),
                counterparty.clone(),
            ));

            fact.display.brief_source = Some(receipt_brief_source(&receipt, counterparty.as_deref(), lines));
            fact.display.approval_subject_version = (!receipt.status.as_str().eq_ignore_ascii_case("draft"))
                .then_some(receipt.approval_subject_version);
            facts.insert((ObjectKind::CustomerReceipt, receipt.base.id.clone()), fact);
        }
        Ok(())
    }

    /// 客户退款审批任务的对象事实。
    ///
    /// # 参数
    /// * `keys` - 本批任务引用的对象键
    /// * `facts` - 输出的对象事实表
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 成功时写入退款单号、金额和原因。
    ///
    /// # 错误
    /// 仓储查询失败时返回错误。
    pub(crate) async fn load_customer_refund_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::CustomerRefund);
        if ids.is_empty() {
            return Ok(());
        }
        let refunds = self.funds_reader().read_customer_refunds(&ids, executor).await?;
        let created_by = self
            .load_created_by_from_audit(
                "customer_refund",
                &ids.iter().cloned().collect::<HashSet<_>>(),
                executor,
            )
            .await?;
        let customer_ids = refunds
            .iter()
            .map(|refund| refund.customer_id.to_string())
            .collect::<Vec<_>>();
        let customer_names = self.customer_display_names(&customer_ids, executor).await?;
        let receipt_ids = refunds
            .iter()
            .filter_map(|refund| refund.original_receipt_id.as_ref().map(ToString::to_string))
            .collect::<Vec<_>>();
        let receipt_origins = self.customer_receipt_origins(&receipt_ids, executor).await?;
        let entry_ids = refunds
            .iter()
            .filter_map(|refund| {
                refund
                    .original_receivable_entry_id
                    .as_ref()
                    .map(ToString::to_string)
            })
            .collect::<Vec<_>>();
        let entry_origins = self.receivable_entry_origins(&entry_ids, executor).await?;
        for refund in refunds {
            let customer = customer_names.get(&refund.customer_id.to_string()).cloned();
            let origin = select_funds_origin(
                refund.original_receipt_id.as_ref(),
                &receipt_origins,
                refund.original_receivable_entry_id.as_ref(),
                &entry_origins,
            );
            let mut fact = funds_fact_display(
                authority_mapping::customer_refund_fact(
                    &refund,
                    created_by.get(&refund.base.id),
                    customer.clone(),
                    &receipt_origins.counterparties,
                    &entry_origins.counterparties,
                ),
                customer
                    .clone()
                    .or_else(|| origin.and_then(|item| item.counterparty.clone())),
            );
            let mut brief = amount_reason_brief(
                "退款金额",
                format_yuan(&refund.amount),
                vec![
                    ("客户", customer.clone()),
                    ("原因", non_empty(&refund.reason_text)),
                    ("发生日", Some(format_instant_date(refund.occurred_at))),
                ],
                join_list_summary([
                    customer,
                    Some(format_yuan(&refund.amount)),
                    non_empty(&refund.reason_text),
                ]),
            );
            append_funds_origin(
                &mut brief,
                origin,
                refund.evidence_attachment_id.is_some(),
                "通过后追加应收冲减与反向核销，原回款或应收事实保留",
            );
            fact.display.brief_source = Some(brief);
            fact.display.approval_subject_version = (!refund.status.as_str().eq_ignore_ascii_case("draft"))
                .then_some(refund.approval_subject_version);
            facts.insert((ObjectKind::CustomerRefund, refund.base.id.clone()), fact);
        }
        Ok(())
    }

    /// 回款冲正审批任务的对象事实。
    ///
    /// # 参数
    /// * `keys` - 本批任务引用的对象键
    /// * `facts` - 输出的对象事实表
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 成功时写入冲正单号、金额和原因。
    ///
    /// # 错误
    /// 仓储查询失败时返回错误。
    pub(crate) async fn load_receipt_reversal_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::ReceiptReversal);
        if ids.is_empty() {
            return Ok(());
        }
        let reversals = self.funds_reader().read_receipt_reversals(&ids, executor).await?;
        let created_by = self
            .load_created_by_from_audit(
                "receipt_reversal",
                &ids.iter().cloned().collect::<HashSet<_>>(),
                executor,
            )
            .await?;
        let receipt_ids = reversals
            .iter()
            .map(|reversal| reversal.original_customer_receipt_id.to_string())
            .collect::<Vec<_>>();
        let origins = self.customer_receipt_origins(&receipt_ids, executor).await?;
        for reversal in reversals {
            let origin = origins
                .briefs
                .get(&reversal.original_customer_receipt_id.to_string());
            let mut fact = funds_fact_display(
                authority_mapping::receipt_reversal_fact(
                    &reversal,
                    created_by.get(&reversal.base.id),
                    &origins.counterparties,
                ),
                origin.and_then(|item| item.counterparty.clone()),
            );
            let mut brief = amount_reason_brief(
                "冲正金额",
                format_yuan(&reversal.amount),
                vec![
                    ("往来主体", origin.and_then(|item| item.counterparty.clone())),
                    ("原因", non_empty(&reversal.reason_text)),
                    ("发生日", Some(format_instant_date(reversal.occurred_at))),
                ],
                join_list_summary([
                    origin.and_then(|item| item.counterparty.clone()),
                    Some(format_yuan(&reversal.amount)),
                    non_empty(&reversal.reason_text),
                ]),
            );
            append_funds_origin(
                &mut brief,
                origin,
                reversal.evidence_attachment_id.is_some(),
                "通过后追加反向回款与反向核销，原回款事实保留并标记已冲正",
            );
            fact.display.brief_source = Some(brief);
            fact.display.approval_subject_version = (!reversal.status.as_str().eq_ignore_ascii_case("draft"))
                .then_some(reversal.approval_subject_version);
            facts.insert((ObjectKind::ReceiptReversal, reversal.base.id.clone()), fact);
        }
        Ok(())
    }
}
