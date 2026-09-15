//! 审批展示快照的事务内捕获与授权后装载。
use std::collections::HashSet;

use erp_workflow::entity::approval_integration::ApprovalSubjectSnapshot;
use erp_workflow::entity::approval_integration::display_snapshot::ApprovalDisplaySnapshot;
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::entity::work_item::WorkItemType;
use erp_workflow::{ApprovalIntegrationExt, WorkflowAuthorizationPort};
use persistence_core::Executor;

use super::facts::object_policy;
use super::{ObjectKind, WorkbenchObjectFactMap, WorkbenchReadService, WorkbenchSubjectDisplay};
use crate::errors::{Error, Result};

/// 在已授权提交的事务内捕获当前业务展示，不读取历史覆盖，也不授予任何权限。
///
/// # 参数
/// * `db` - 当前数据库
/// * `document_type` / `id` - 由正式提交用例确定的业务对象
/// * `executor` - 必须使用审批启动事务的执行器
/// # 返回
/// 返回与责任队列采用同一定义的不可变展示。
/// # 错误
/// 业务对象或展示缺失、字段越界、仓储读取失败时中止提交。
pub async fn capture_approval_display(
    db: &mongodb::Database,
    document_type: DocumentType,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<ApprovalDisplaySnapshot> {
    let policy = object_policy(WorkItemType::DocumentApproval, document_type.as_str())
        .ok_or_else(|| Error::ValidationError("单据未注册审批摘要".into()))?;
    let key = (policy.object_kind, id.to_owned());
    let reader =
        WorkbenchReadService::new(db.clone(), erp_workflow::ports::FailClosedWorkflowAuthorizationPort);
    let mut facts = reader.load_live_object_facts(&HashSet::from([key.clone()]), executor).await?;
    let fact = facts.remove(&key).ok_or_else(|| Error::ValidationError("审批单据不存在".into()))?;
    let snapshot = ApprovalDisplaySnapshot {
        root_document_id: fact.display.root_document_id,
        counterparty_label: fact.display.counterparty_label,
        impact_summary: fact.display.impact_summary,
        source: fact.display.brief_source.ok_or_else(|| Error::ValidationError("审批单据摘要缺失".into()))?,
    };
    snapshot.validate()?;
    Ok(snapshot)
}

impl<A: WorkflowAuthorizationPort> WorkbenchReadService<A> {
    /// 批量加载不可变展示；只覆盖已存在业务事实的同类型、同版本摘要，不改变授权事实。
    pub(super) async fn load_approval_displays(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut WorkbenchObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let objects = DocumentType::ALL
            .iter()
            .flat_map(|kind| {
                let policy = object_policy(WorkItemType::DocumentApproval, kind.as_str());
                keys.iter()
                    .filter(move |(object_kind, _)| policy.is_some_and(|p| p.object_kind == *object_kind))
                    .map(move |(_, id)| (*kind, id.clone()))
            })
            .collect::<Vec<_>>();
        for snapshot in
            self.db.approval_subject_snapshots().list_by_business_objects(&objects, executor).await?
        {
            apply_snapshot(facts, snapshot);
        }
        Ok(())
    }
}

/// 以注册对象类型与业务身份精确关联；历史快照缺失保持原缺失规则。
fn apply_snapshot(facts: &mut WorkbenchObjectFactMap, snapshot: ApprovalSubjectSnapshot) {
    let Some(policy) = object_policy(WorkItemType::DocumentApproval, snapshot.document_type.as_str()) else {
        return;
    };
    let Some(fact) = facts.get_mut(&(policy.object_kind, snapshot.business_object_id.clone())) else {
        return;
    };
    if snapshot.display.is_none() {
        apply_legacy(fact, &snapshot);
        return;
    }
    let Some(display) = snapshot.display.filter(|display| display.validate().is_ok()) else {
        return;
    };
    fact.display.subject_briefs.insert(
        snapshot.subject_version.to_string(),
        WorkbenchSubjectDisplay {
            counterparty_label: display.counterparty_label,
            impact_summary: display.impact_summary,
            brief_source: Some(display.source),
        },
    );
}

/// 旧快照只展示实际保存的金额、数量与提交时间；不得从当前单据补历史字段。
fn apply_legacy(fact: &mut super::WorkbenchObjectFact, snapshot: &ApprovalSubjectSnapshot) {
    use super::brief::{ObjectBriefSource, format_instant_datetime, format_quantity, push_section};
    if super::approval_list::document_summary(fact, Some(snapshot.subject_version)).is_some() {
        return;
    }
    let label = match snapshot.document_type {
        DocumentType::CustomerReceipt => "回款金额",
        DocumentType::CustomerRefund | DocumentType::SupplierRefund => "退款金额",
        DocumentType::ReceiptReversal | DocumentType::PaymentReversal => "冲正金额",
        DocumentType::StockAdjustment => "调整成本",
        _ => return,
    };
    let mut source = ObjectBriefSource::default();
    push_section(
        &mut source.extra_sections,
        "历史资料",
        Some("旧审批仅保留基础快照，未保存的明细和业务字段无法还原"),
        false,
    );
    if let Some(amount) = &snapshot.payload.total_amount {
        push_section(
            &mut source.extra_sections,
            label,
            Some(&super::presentation::format_yuan(amount)),
            true,
        );
    }
    if let Some(quantity) = &snapshot.payload.total_quantity {
        push_section(&mut source.extra_sections, "数量", Some(&format_quantity(quantity, None)), true);
    }
    push_section(
        &mut source.extra_sections,
        "提交时间",
        Some(&format_instant_datetime(snapshot.payload.submitted_at)),
        false,
    );
    source.list_summary = "历史审批 · 部分资料未保存".into();
    fact.display.subject_briefs.insert(
        snapshot.subject_version.to_string(),
        WorkbenchSubjectDisplay {
            counterparty_label: None,
            impact_summary: None,
            brief_source: Some(source),
        },
    );
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::Instant;
    use erp_workflow::entity::approval_integration::ApprovalSubjectSnapshotPayload;

    use super::super::brief::{BriefSection, ObjectBriefSource};
    use super::*;

    /// 六类原本可被草稿覆盖的单据，都必须优先读取对应版本的冻结字段。
    #[test]
    fn snapshots_preserve_history_across_all_mutable_document_types() {
        for kind in [
            DocumentType::CustomerReceipt,
            DocumentType::CustomerRefund,
            DocumentType::SupplierRefund,
            DocumentType::ReceiptReversal,
            DocumentType::PaymentReversal,
            DocumentType::StockAdjustment,
        ] {
            let snapshot = snapshot(kind);
            let policy = object_policy(WorkItemType::DocumentApproval, kind.as_str()).unwrap();
            let mut fact = super::super::WorkbenchObjectFact::from_authority(
                erp_workflow::ports::ObjectFact::new("document", "当前单据", "owner"),
            );
            fact.display.counterparty_label = Some("当前往来方".into());
            fact.display.brief_source =
                Some(ObjectBriefSource { list_summary: "当前草稿".into(), ..Default::default() });
            let mut facts = WorkbenchObjectFactMap::from([((policy.object_kind, "document".into()), fact)]);
            apply_snapshot(&mut facts, snapshot.clone());
            let fact = facts.get_mut(&(policy.object_kind, "document".into())).unwrap();
            fact.display.brief_source.as_mut().unwrap().list_summary = "再次修改".into();
            let old = super::super::approval_list::document_summary(fact, Some(1)).unwrap();
            assert_eq!(old.counterparty_label.as_deref(), Some("原往来方"));
            assert!(old.summary_sections.iter().any(|s| s.value == "原始原因"));
            assert!(!old.list_summary.contains("修改"));
            assert!(super::super::approval_list::document_summary(fact, Some(2)).is_none());
            let mut missing = snapshot;
            missing.subject_version = 3;
            missing.display = None;
            apply_snapshot(&mut facts, missing);
            let old = super::super::approval_list::document_summary(
                &facts[&(policy.object_kind, "document".into())],
                Some(3),
            )
            .unwrap();
            assert!(old.counterparty_label.is_none());
            assert!(old.summary_sections.iter().any(|s| s.label == "历史资料"));
        }
    }

    /// 快照没有授权对象时不得制造可读事实。
    #[test]
    fn snapshot_does_not_create_authority() {
        let mut facts = WorkbenchObjectFactMap::new();
        apply_snapshot(&mut facts, snapshot(DocumentType::CustomerReceipt));
        assert!(facts.is_empty());
    }

    fn snapshot(kind: DocumentType) -> ApprovalSubjectSnapshot {
        let mut snapshot = ApprovalSubjectSnapshot::new(
            erp_core::ids::ApprovalSubjectSnapshotId::new("snapshot"),
            bpm::ApprovalProcessInstanceId::new("instance"),
            kind,
            "document",
            1,
            ApprovalSubjectSnapshotPayload {
                document_no: "NO-1".into(),
                responsible_org_id: "org".into(),
                submitted_by: "owner".into(),
                submitted_at: Instant::from_unix_secs(1800000000),
                counterparty: None,
                total_amount: Some("100".parse().unwrap()),
                total_quantity: Some("2".parse().unwrap()),
                line_count: 1,
            },
        )
        .unwrap();
        snapshot.display = Some(ApprovalDisplaySnapshot {
            root_document_id: "document".into(),
            counterparty_label: Some("原往来方".into()),
            impact_summary: None,
            source: ObjectBriefSource {
                extra_sections: vec![BriefSection {
                    label: "原因".into(),
                    value: "原始原因".into(),
                    numeric: false,
                    object_id: None,
                }],
                ..Default::default()
            },
        });
        snapshot
    }
}
