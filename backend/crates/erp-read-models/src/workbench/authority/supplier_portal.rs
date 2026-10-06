//! 供应商申请任务的真实归属与各次冻结提交版本。

use std::collections::{HashMap, HashSet};

use erp_catalog::portal::{CatalogPortalExt, NewProductDraft};
use erp_supplier::portal::{CooperationApplication, CooperationRepository};
use erp_supply::portal::{ApplicationKind, OfferingApplication, PortalSupplyExt};
use erp_workflow::entity::work_item::WorkItemSubjectVersions;
use erp_workflow::ports::{ObjectFact, ObjectFactMap, ObjectKind, SubjectBrief};
use persistence_core::Executor;

use super::{WorkItemFactsReader, object_ids};
use crate::{Error, Result};

impl WorkItemFactsReader {
    /// 有界读取三类申请的权威事实，不读取全库候选或正式商品资料。
    ///
    /// # 参数
    /// * `keys` - 当前任务批次的精确对象键。
    /// * `facts` - 输出的权威对象事实。
    /// * `executor` - 任务命令或查询沿用的执行器。
    /// # 返回
    /// 仅真实冻结提交进入事实表；草稿无确认任务事实。
    /// # 错误
    /// 仓储失败、冻结版本损坏或跨类型身份冲突时拒绝。
    pub(in crate::workbench) async fn load_supplier_portal_request_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::SupplierPortalRequest);
        if ids.is_empty() {
            return Ok(());
        }
        for request in self.db.portal_applications().list_active_by_ids(&ids, executor).await? {
            insert_request(facts, &request.base.id, offering_request_fact(&request)?)?;
        }
        for request in self.db.new_product_drafts().list_active_by_ids(&ids, executor).await? {
            insert_request(facts, &request.base.id, new_product_request_fact(&request)?)?;
        }
        for request in CooperationRepository::new(&self.db).list_active_by_ids(&ids, executor).await? {
            insert_request(facts, &request.base.id, cooperation_request_fact(&request)?)?;
        }
        Ok(())
    }
}

/// 同一申请键不得指向两个领域的不同申请。
fn insert_request(facts: &mut ObjectFactMap, id: &str, fact: Option<ObjectFact>) -> Result<()> {
    if let Some(fact) = fact
        && facts.insert((ObjectKind::SupplierPortalRequest, id.into()), fact).is_some()
    {
        return Err(Error::Internal("供应商申请身份重复，无法证明任务来源".into()));
    }
    Ok(())
}

/// 返回供给申请的固定展示类型，不采用内部事件名。
pub(in crate::workbench) fn offering_kind_label(kind: ApplicationKind) -> &'static str {
    match kind {
        ApplicationKind::ExistingQuote => "已有商品报价",
        ApplicationKind::TermsChange => "供给条款变更",
        ApplicationKind::StopSupply => "停止供给申请",
    }
}

/// 供给申请仅接受真实提交编号，不把草稿持久化版本当成提交版本。
pub(in crate::workbench) fn offering_request_fact(
    request: &OfferingApplication,
) -> Result<Option<ObjectFact>> {
    let subjects = request
        .submissions
        .iter()
        .map(|submission| {
            (
                format!("offering:{}", submission.submission_no),
                SubjectBrief {
                    counterparty_label: None,
                    impact_summary: Some(format!(
                        "{}待采购确认；确认前继续使用当前正式事实",
                        offering_kind_label(submission.snapshot.kind())
                    )),
                },
            )
        })
        .collect();
    request_fact(&request.base.id, request.created_by.as_str(), "供应商供给申请", subjects)
}

/// 新品任务所有历史提交分别冻结，重提不使既往任务失去可回看版本。
pub(in crate::workbench) fn new_product_request_fact(
    request: &NewProductDraft,
) -> Result<Option<ObjectFact>> {
    let subjects = request
        .submissions
        .iter()
        .map(|submission| {
            (
                format!("new_product:{}", submission.id),
                SubjectBrief {
                    counterparty_label: None,
                    impact_summary: Some(format!(
                        "{}共 {} 个规格待确认；新建 SKU 保持未上架",
                        submission.input.name,
                        submission.input.skus.len()
                    )),
                },
            )
        })
        .collect();
    request_fact(&request.base.id, request.created_by.as_str(), "供应商新品申请", subjects)
}

/// 合作条款任务关联冻结商务申请，不能被供给调价任务版本替代。
pub(in crate::workbench) fn cooperation_request_fact(
    request: &CooperationApplication,
) -> Result<Option<ObjectFact>> {
    let subjects = request
        .submissions
        .iter()
        .map(|submission| {
            (
                format!("cooperation:{}", submission.submission_no),
                SubjectBrief {
                    counterparty_label: None,
                    impact_summary: Some("合作条款待确认；通过后影响后续采购，已冻结付款条件保持不变".into()),
                },
            )
        })
        .collect();
    request_fact(&request.base.id, request.created_by.as_str(), "供应商合作条款申请", subjects)
}

/// 创建人保持真实供应商身份，具体内部任务责任由工作项独立校验。
fn request_fact(
    id: &str,
    created_by: &str,
    label: &str,
    subjects: HashMap<String, SubjectBrief>,
) -> Result<Option<ObjectFact>> {
    if subjects.is_empty() {
        return Ok(None);
    }
    let mut fact = ObjectFact::new(id, label, created_by);
    fact.subject_versions = WorkItemSubjectVersions::constrained(subjects.keys().cloned().collect())?;
    fact.subject_briefs = subjects;
    Ok(Some(fact))
}

#[cfg(test)]
mod tests {
    use application_core::AuditActor;
    use erp_core::AccountKind;
    use erp_core::common::time::Instant;
    use erp_supplier::portal::CooperationRequest;
    use erp_supplier::{ReconciliationCycle, SettlementMode};
    use erp_supply::portal::OfferingApplicationSnapshot;

    use super::*;

    #[test]
    fn offering_facts_keep_each_frozen_version_and_true_supplier_creator() {
        let actor = AuditActor::new("supplier-user".into(), "external".into(), AccountKind::Supplier);
        let mut request = OfferingApplication::new(
            "request".into(),
            "supplier",
            &actor,
            OfferingApplicationSnapshot::StopSupply {
                offering_id: "offering".into(),
                expected_offering_version: 2,
                expected_revision_no: 1,
            },
            "停止供应",
        )
        .unwrap();
        assert!(offering_request_fact(&request).unwrap().is_none());
        request.submit(&actor, "reviewer", "task-1", 1, Instant::from_unix_secs(100)).unwrap();
        request.withdraw(&actor).unwrap();
        request.submit(&actor, "replacement", "task-2", 1, Instant::from_unix_secs(200)).unwrap();
        let fact = offering_request_fact(&request).unwrap().unwrap();
        assert_eq!(fact.root_document_id, "request");
        assert_eq!(fact.created_by, "supplier-user");
        assert!(fact.subject_versions.accepts("offering:1"));
        assert!(fact.subject_versions.accepts("offering:2"));
        assert!(!fact.subject_versions.accepts("cooperation:1"));
        assert!(!fact.subject_versions.accepts("3"));
        assert_eq!(fact.subject_briefs.len(), 2);
    }

    #[test]
    fn duplicate_request_identity_fails_closed() {
        let mut facts = ObjectFactMap::new();
        insert_request(&mut facts, "request", Some(ObjectFact::new("request", "供给", "supplier"))).unwrap();
        assert!(
            insert_request(&mut facts, "request", Some(ObjectFact::new("request", "新品", "supplier")))
                .is_err()
        );
    }

    #[test]
    fn cooperation_subjects_keep_supplier_creator_and_separate_offering_versions() {
        let actor = AuditActor::new("external".into(), "supplier-account".into(), AccountKind::Supplier);
        let mut request = CooperationApplication::new(
            "cooperation".into(),
            "supplier",
            CooperationRequest {
                expected_supplier_version: 1,
                expected_profile_id: "profile-1".into(),
                settlement_mode: SettlementMode::Prepayment,
                reconciliation_cycle: ReconciliationCycle::None,
                payment_term: "PREPAY_100".into(),
                reason: "改为全额预付".into(),
            },
            &actor,
        )
        .unwrap();
        assert!(cooperation_request_fact(&request).unwrap().is_none());
        request.submit("supplier", 1, "reviewer", "task", &actor, 100).unwrap();
        let fact = cooperation_request_fact(&request).unwrap().unwrap();
        assert_eq!(fact.created_by, "external");
        assert!(fact.subject_versions.accepts("cooperation:1"));
        assert!(!fact.subject_versions.accepts("offering:1"));
        assert!(!fact.subject_versions.accepts("new_product:1"));
    }
}
