//! 供应商付款、退款与付款冲正任务简报。

use std::collections::HashSet;

use database::{PayableExt, ReturnsExt};
use persistence_core::Executor;

use super::super::brief::{
    format_instant_date, join_list_summary, non_empty, push_section, ObjectBriefSource, BRIEF_LINE_LIMIT,
};
use super::super::presentation::format_yuan;
use super::super::{object_ids, ObjectFact, ObjectFactMap, ObjectKind, WorkItemService};
use super::mapping::{amount_reason_brief, append_funds_origin};
use crate::errors::Result;

impl WorkItemService {
    /// 供应商付款事实简报。
    ///
    /// # 参数
    /// * `keys` - 本批任务引用的对象键
    /// * `facts` - 输出的对象事实表
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 成功时写入付款单号、供应商、金额和凭证。
    ///
    /// # 错误
    /// 仓储查询失败时返回错误。
    pub(in crate::work_item) async fn load_supplier_payment_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::SupplierPayment);
        if ids.is_empty() {
            return Ok(());
        }
        let payments = self
            .db
            .supplier_payments()
            .list_active_by_ids(&ids, executor)
            .await?;
        let created_by = self
            .load_created_by_from_audit(
                "supplier_payment",
                &ids.iter().cloned().collect::<HashSet<_>>(),
                executor,
            )
            .await?;
        let supplier_ids = payments
            .iter()
            .map(|item| item.supplier_id.to_string())
            .collect::<Vec<_>>();
        let supplier_names = self.supplier_display_names(&supplier_ids, executor).await?;
        let allocation_lines = self.payment_allocation_lines(&payments, executor).await?;
        for payment in payments {
            let supplier = supplier_names.get(&payment.supplier_id.to_string()).cloned();
            let lines = allocation_lines
                .get(&payment.base.id)
                .cloned()
                .unwrap_or_default();
            let mut fact = ObjectFact::new(
                payment.base.id.clone(),
                format!("供应商付款 {}", payment.payment_no),
                created_by.get(&payment.base.id).cloned().unwrap_or_default(),
            );
            fact.counterparty_label = supplier.clone();
            fact.impact_summary = Some("付款已登记并过账；纠错须走付款冲正或供应商退款".to_string());
            let mut sections = Vec::new();
            push_section(&mut sections, "供应商", supplier.as_deref(), false);
            push_section(
                &mut sections,
                "含税金额",
                Some(format_yuan(&payment.amount)).as_deref(),
                true,
            );
            push_section(
                &mut sections,
                "付款日",
                Some(format_instant_date(payment.paid_at)).as_deref(),
                false,
            );
            push_section(&mut sections, "凭证", payment.bank_reference.as_deref(), false);
            if !lines.is_empty() {
                push_section(
                    &mut sections,
                    "核销事实",
                    Some(format!("{} 笔", lines.len())).as_deref(),
                    false,
                );
            }
            let more_count = lines.len().saturating_sub(BRIEF_LINE_LIMIT) as u32;
            let lines = lines.into_iter().take(BRIEF_LINE_LIMIT).collect();
            fact.brief_source = Some(ObjectBriefSource {
                customer: None,
                amount_label: Some(format_yuan(&payment.amount)),
                extra_sections: sections,
                list_summary: join_list_summary([
                    supplier.clone(),
                    Some(format_yuan(&payment.amount)),
                    payment.bank_reference.clone().and_then(|text| non_empty(&text)),
                ]),
                lines,
                more_count,
                submitter_name: None,
            });
            facts.insert((ObjectKind::SupplierPayment, payment.base.id.clone()), fact);
        }
        Ok(())
    }

    /// 供应商退款审批任务的对象事实。
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
    pub(in crate::work_item) async fn load_supplier_refund_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::SupplierRefund);
        if ids.is_empty() {
            return Ok(());
        }
        let refunds = self
            .db
            .supplier_refunds()
            .list_active_by_ids(&ids, executor)
            .await?;
        let created_by = self
            .load_created_by_from_audit(
                "supplier_refund",
                &ids.iter().cloned().collect::<HashSet<_>>(),
                executor,
            )
            .await?;
        let supplier_ids = refunds
            .iter()
            .map(|item| item.supplier_id.to_string())
            .collect::<Vec<_>>();
        let supplier_names = self.supplier_display_names(&supplier_ids, executor).await?;
        let payment_ids = refunds
            .iter()
            .filter_map(|refund| refund.original_payment_id.as_ref().map(ToString::to_string))
            .collect::<Vec<_>>();
        let payment_origins = self.supplier_payment_origins(&payment_ids, executor).await?;
        let entry_ids = refunds
            .iter()
            .filter_map(|refund| refund.original_payable_entry_id.as_ref().map(ToString::to_string))
            .collect::<Vec<_>>();
        let entry_origins = self.payable_entry_origins(&entry_ids, executor).await?;
        for refund in refunds {
            let supplier = supplier_names.get(&refund.supplier_id.to_string()).cloned();
            let origin = refund
                .original_payment_id
                .as_ref()
                .and_then(|id| payment_origins.get(&id.to_string()))
                .or_else(|| {
                    refund
                        .original_payable_entry_id
                        .as_ref()
                        .and_then(|id| entry_origins.get(&id.to_string()))
                });
            let mut fact = ObjectFact::new(
                refund.base.id.clone(),
                format!("供应商退款 {}", refund.refund_no),
                created_by.get(&refund.base.id).cloned().unwrap_or_default(),
            );
            fact.counterparty_label = supplier.clone();
            fact.impact_summary = Some("不审批则供应商退款不能过账".to_string());
            let mut brief = amount_reason_brief(
                format_yuan(&refund.amount),
                vec![
                    ("供应商", supplier.clone()),
                    ("原因", non_empty(&refund.reason_text)),
                    ("发生日", Some(format_instant_date(refund.occurred_at))),
                ],
                join_list_summary([
                    supplier,
                    Some(format_yuan(&refund.amount)),
                    non_empty(&refund.reason_text),
                ]),
            );
            append_funds_origin(
                &mut brief,
                origin,
                refund.evidence_attachment_id.is_some(),
                "通过后追加应付冲减；已付款部分追加反向付款分配，原付款或应付事实保留",
            );
            fact.brief_source = Some(brief);
            facts.insert((ObjectKind::SupplierRefund, refund.base.id.clone()), fact);
        }
        Ok(())
    }

    /// 付款冲正审批任务的对象事实。
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
    pub(in crate::work_item) async fn load_payment_reversal_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::PaymentReversal);
        if ids.is_empty() {
            return Ok(());
        }
        let reversals = self
            .db
            .payment_reversals()
            .list_active_by_ids(&ids, executor)
            .await?;
        let created_by = self
            .load_created_by_from_audit(
                "payment_reversal",
                &ids.iter().cloned().collect::<HashSet<_>>(),
                executor,
            )
            .await?;
        let payment_ids = reversals
            .iter()
            .map(|reversal| reversal.original_supplier_payment_id.to_string())
            .collect::<Vec<_>>();
        let origins = self.supplier_payment_origins(&payment_ids, executor).await?;
        for reversal in reversals {
            let origin = origins.get(&reversal.original_supplier_payment_id.to_string());
            let mut fact = ObjectFact::new(
                reversal.base.id.clone(),
                format!("付款冲正 {}", reversal.reversal_no),
                created_by.get(&reversal.base.id).cloned().unwrap_or_default(),
            );
            fact.counterparty_label = origin.and_then(|item| item.counterparty.clone());
            fact.impact_summary = Some("不审批则付款冲正不能过账".to_string());
            let mut brief = amount_reason_brief(
                format_yuan(&reversal.amount),
                vec![
                    ("供应商", origin.and_then(|item| item.counterparty.clone())),
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
                "通过后追加反向付款与反向核销，原付款事实保留并标记已冲正",
            );
            fact.brief_source = Some(brief);
            facts.insert((ObjectKind::PaymentReversal, reversal.base.id.clone()), fact);
        }
        Ok(())
    }
}
