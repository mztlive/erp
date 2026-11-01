//! 询价任务仅读当前需求身份，不装载销售参考价或采购内部报价。

use std::collections::HashSet;

use erp_sales::entity::sales_inquiry::{InquiryStatus, SalesInquiry};
use erp_sales::repository::sales_inquiry::SalesInquiryRepository;
use erp_workflow::entity::work_item::{WorkItemSubjectVersions, inquiry_subject_version};
use persistence_core::Executor;

use super::{ObjectFact, ObjectFactMap, ObjectKind, object_ids};
use crate::Result;

impl super::WorkItemFactsReader {
    /// 装载询价当前任务可用的需求版本与稳定业务行白名单。
    ///
    /// # 参数
    /// `keys` 为精确对象集合，`facts` 为输出，`executor` 为调用方快照。
    /// # 返回
    /// 当前待处理需求的安全身份事实；草稿和终态不形成任务授权。
    /// # 错误
    /// 仓储读取或需求版本损坏时返回错误。
    pub(in crate::workbench) async fn load_inquiry_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::SalesInquiry);
        if ids.is_empty() {
            return Ok(());
        }
        for inquiry in SalesInquiryRepository::new(&self.db).by_ids(&ids, executor).await? {
            if let Some(fact) = inquiry_fact(&inquiry)? {
                facts.insert((ObjectKind::SalesInquiry, inquiry.base.id), fact);
            }
        }
        Ok(())
    }
}

fn inquiry_fact(inquiry: &SalesInquiry) -> Result<Option<ObjectFact>> {
    if !matches!(inquiry.status, InquiryStatus::AwaitingProcurement | InquiryStatus::ProcurementProcessing) {
        return Ok(None);
    }
    let requirement = inquiry.current_requirement()?;
    let mut fact = ObjectFact::new(
        &inquiry.base.id,
        format!("询价 {}", inquiry.inquiry_no),
        &inquiry.sales_owner_user_id,
    );
    fact.subject_versions =
        WorkItemSubjectVersions::constrained(vec![inquiry_subject_version(requirement.version)])?;
    fact.responsibility_scope_ids =
        Some(requirement.data.lines.iter().map(|line| line.line_id.clone()).collect());
    fact.counterparty_label = Some(inquiry.customer_name.clone());
    fact.impact_summary = Some("当前责任商品需要采购供给结论".into());
    Ok(Some(fact))
}
