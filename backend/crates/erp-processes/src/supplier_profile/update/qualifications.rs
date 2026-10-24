//! 资质事实读取、领域差异计划与新增、修订、停用准备。

use std::collections::{HashMap, HashSet};

use erp_core::field_update::FieldUpdate;
use erp_core::ids::{
    SupplierAccountId, SupplierCapabilityId, SupplierQualificationCapabilityId, SupplierQualificationId,
    SupplierQualificationRevisionId,
};
use erp_supplier::repository::prelude::*;
use erp_supplier::{
    PlannedQualificationInput, QualificationStatus, SaveSupplierProfileRequest, SupplierExt,
    SupplierProfileChangePlan, SupplierQualification, SupplierQualificationCapability,
    SupplierQualificationRevision, SupplierQualificationUpdate, profile_change, qualification_identity_key,
};
use id_generator::next_id;
use persistence_core::NoTransaction;

use super::super::SupplierProfileProcess;
use super::persist::QualificationChanges;
use crate::{Error, Result};

struct QualificationPlan {
    changes: SupplierProfileChangePlan,
    requested: HashMap<String, PlannedQualificationInput>,
    existing: HashMap<String, SupplierQualification>,
}

impl SupplierProfileProcess {
    /// 将资质集合解析为新增、更新、停用、快照及能力关联替换。
    ///
    /// # 参数
    /// * `supplier_id` - 供应商角色 ID
    /// * `req` - 已校验的根资料命令
    /// * `capability_ids` - 当前命令能力代码到稳定能力 ID 的映射
    /// * `actor_id` - 执行变更的账号 ID
    ///
    /// # 返回
    /// 返回待事务持久化的资质、修订及关联替换集合。
    ///
    /// # 错误
    /// 仓储查询、能力引用、领域更新或修订构造失败时返回错误。
    pub(super) async fn prepare_qualification_changes(
        &self,
        supplier_id: &SupplierAccountId,
        req: &SaveSupplierProfileRequest,
        capability_ids: &HashMap<String, SupplierCapabilityId>,
        actor_id: &str,
    ) -> Result<QualificationChanges> {
        let mut plan = self.qualification_plan(supplier_id, req, capability_ids).await?;
        let mut changes = QualificationChanges::default();
        for key in plan.changes.qualification_updates {
            let qualification = plan
                .existing
                .remove(&key)
                .ok_or_else(|| Error::Internal("资质计划与已加载事实不一致".to_string()))?;
            let input = plan
                .requested
                .get(&key)
                .ok_or_else(|| Error::Internal("资质请求与计划不一致".to_string()))?;
            self.prepare_qualification_update(qualification, input, capability_ids, actor_id, &mut changes)
                .await?;
        }
        for key in plan.changes.qualification_disables {
            let qualification = plan
                .existing
                .remove(&key)
                .ok_or_else(|| Error::Internal("资质停用计划与已加载事实不一致".to_string()))?;
            self.prepare_qualification_disable(qualification, actor_id, &mut changes).await?;
        }
        for input in plan.changes.qualification_creates {
            changes.append_created(supplier_id, &input, capability_ids, actor_id)?;
        }
        Ok(changes)
    }

    /// 先加载当前资质及关联，再交由供应商领域生成差异计划。
    async fn qualification_plan(
        &self,
        supplier_id: &SupplierAccountId,
        req: &SaveSupplierProfileRequest,
        capability_ids: &HashMap<String, SupplierCapabilityId>,
    ) -> Result<QualificationPlan> {
        let existing = self.db.supplier().list_qualifications(supplier_id, &mut NoTransaction).await?;
        let linked_capabilities = self.qualification_links(&existing).await?;
        let planned_inputs: Vec<PlannedQualificationInput> = req
            .qualifications
            .iter()
            .map(|input| PlannedQualificationInput {
                qualification_type: input.qualification_type,
                certificate_no: input.certificate_no.clone(),
                issuer: input.issuer.clone(),
                valid_from: input.valid_from,
                valid_to: input.valid_to,
                attachment_id: input.attachment_id.clone(),
                capability_codes: input.capability_codes.clone(),
            })
            .collect();
        let plan = SupplierProfileChangePlan::from_loaded(
            &[],
            &existing,
            &linked_capabilities,
            capability_ids,
            &[],
            &planned_inputs,
        )
        .map_err(|e| Error::ValidationError(e.to_string()))?;
        let requested_map: HashMap<String, PlannedQualificationInput> = planned_inputs
            .into_iter()
            .map(|input| (qualification_identity_key(input.qualification_type, &input.certificate_no), input))
            .collect();
        let existing_by_key: HashMap<String, SupplierQualification> =
            existing.into_iter().map(|qual| (qual.identity_key(), qual)).collect();
        Ok(QualificationPlan { changes: plan, requested: requested_map, existing: existing_by_key })
    }

    async fn qualification_links(
        &self,
        existing: &[SupplierQualification],
    ) -> Result<HashMap<String, HashSet<String>>> {
        let qualification_ids: Vec<SupplierQualificationId> =
            existing.iter().map(|item| SupplierQualificationId::new(&item.base.id)).collect();
        let existing_links = self
            .db
            .supplier_qualification_capabilities()
            .list_by_qualification_ids(&qualification_ids, &mut NoTransaction)
            .await?;
        let mut linked_capabilities: HashMap<String, HashSet<String>> = HashMap::new();
        for link in existing_links {
            linked_capabilities
                .entry(link.qualification_id.to_string())
                .or_default()
                .insert(link.capability_id.to_string());
        }
        Ok(linked_capabilities)
    }

    /// 对一个更新计划执行领域变更后，按原顺序读取修订号并替换能力关联。
    async fn prepare_qualification_update(
        &self,
        mut qualification: SupplierQualification,
        input: &PlannedQualificationInput,
        capability_ids: &HashMap<String, SupplierCapabilityId>,
        actor_id: &str,
        changes: &mut QualificationChanges,
    ) -> Result<()> {
        profile_change::apply_qualification_input(
            &mut qualification,
            input.issuer.clone(),
            input.valid_from,
            input.valid_to,
            input.attachment_id.clone(),
            actor_id,
        )
        .map_err(|e| Error::BusinessLogicError(e.to_string()))?;
        let revision = self.qualification_revision(&mut qualification).await?;
        let link_ids = input
            .capability_codes
            .iter()
            .map(|_| SupplierQualificationCapabilityId::new(next_id()))
            .collect();
        let links = SupplierQualificationCapability::links_for_qualification(
            SupplierQualificationId::new(&qualification.base.id),
            &input.capability_codes,
            capability_ids,
            link_ids,
        )
        .map_err(|e| Error::ValidationError(e.to_string()))?;
        changes.replacements.push((SupplierQualificationId::new(&qualification.base.id), links));
        changes.updated.push(qualification);
        changes.revisions.push(revision);
        Ok(())
    }

    async fn prepare_qualification_disable(
        &self,
        mut qualification: SupplierQualification,
        actor_id: &str,
        changes: &mut QualificationChanges,
    ) -> Result<()> {
        qualification
            .update(
                SupplierQualificationUpdate {
                    issuer: FieldUpdate::Unchanged,
                    attachment_id: FieldUpdate::Unchanged,
                    valid_from: FieldUpdate::Unchanged,
                    valid_to: FieldUpdate::Unchanged,
                    status: Some(QualificationStatus::Disabled),
                },
                actor_id,
            )
            .map_err(|e| Error::BusinessLogicError(e.to_string()))?;
        let revision = self.qualification_revision(&mut qualification).await?;
        changes.updated.push(qualification);
        changes.revisions.push(revision);
        Ok(())
    }

    /// 修订号读取成功后才分配新修订 ID，不改变失败时的 ID 分配顺序。
    async fn qualification_revision(
        &self,
        qualification: &mut SupplierQualification,
    ) -> Result<SupplierQualificationRevision> {
        let revision_no = self
            .db
            .supplier()
            .next_qualification_revision_no(
                &qualification.supplier_id,
                qualification.qualification_type,
                &qualification.certificate_no,
                &mut NoTransaction,
            )
            .await?;
        let revision_id = SupplierQualificationRevisionId::new(next_id());
        qualification.stable.current_revision_id = Some(revision_id.to_string());
        qualification
            .snapshot_revision(revision_id, revision_no)
            .map_err(|e| Error::BusinessLogicError(e.to_string()))
    }
}

impl QualificationChanges {
    /// 仅分配主键并接入领域工厂；工厂成功后才追加待持久化变更。
    fn append_created(
        &mut self,
        supplier_id: &SupplierAccountId,
        input: &PlannedQualificationInput,
        capability_ids: &HashMap<String, SupplierCapabilityId>,
        actor_id: &str,
    ) -> Result<()> {
        let qualification_id = SupplierQualificationId::new(next_id());
        let revision_id = SupplierQualificationRevisionId::new(next_id());
        let link_ids = input
            .capability_codes
            .iter()
            .map(|_| SupplierQualificationCapabilityId::new(next_id()))
            .collect();
        let (qualification, revision, links) =
            profile_change::new_qualification(profile_change::NewQualificationParams {
                supplier_id,
                qualification_type: input.qualification_type,
                certificate_no: input.certificate_no.clone(),
                issuer: input.issuer.clone(),
                valid_from: input.valid_from,
                valid_to: input.valid_to,
                attachment_id: input.attachment_id.clone(),
                capability_codes: &input.capability_codes,
                capability_ids,
                actor_id,
                qualification_id: qualification_id.clone(),
                revision_id,
                link_ids,
            })
            .map_err(|e| Error::BusinessLogicError(e.to_string()))?;
        self.replacements.push((SupplierQualificationId::new(&qualification.base.id), links));
        self.created.push(qualification);
        self.revisions.push(revision);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::BusinessDate;
    use erp_supplier::{CapabilityCode, QualificationType};

    use super::*;

    fn qualification_input() -> PlannedQualificationInput {
        PlannedQualificationInput {
            qualification_type: QualificationType::Certificate,
            certificate_no: "CERT-1".to_string(),
            issuer: Some("发证单位".to_string()),
            valid_from: Some(BusinessDate::from_ymd(2026, 1, 1).unwrap()),
            valid_to: None,
            attachment_id: None,
            capability_codes: vec![CapabilityCode::Physical, CapabilityCode::Virtual],
        }
    }

    fn capability_ids() -> HashMap<String, SupplierCapabilityId> {
        HashMap::from([
            ("physical".to_string(), SupplierCapabilityId::new("cap-physical")),
            ("virtual".to_string(), SupplierCapabilityId::new("cap-virtual")),
        ])
    }

    #[test]
    fn created_qualification_keeps_snapshot_identity_and_input_link_order() {
        let mut changes = QualificationChanges::default();
        let supplier_id = SupplierAccountId::new("supplier-1");
        changes.append_created(&supplier_id, &qualification_input(), &capability_ids(), "actor-1").unwrap();

        assert_eq!(changes.created.len(), 1);
        assert_eq!(changes.revisions.len(), 1);
        assert_eq!(changes.replacements.len(), 1);
        assert!(changes.updated.is_empty());
        let qualification = &changes.created[0];
        let revision = &changes.revisions[0];
        let (replacement_id, links) = &changes.replacements[0];
        assert_eq!(qualification.supplier_id, supplier_id);
        assert_eq!(qualification.certificate_no, "CERT-1");
        assert_eq!(qualification.stable.current_revision_id.as_deref(), Some(revision.base.id.as_str()));
        assert_eq!(revision.revision.revision_no, 1);
        assert_eq!(revision.certificate_no, qualification.certificate_no);
        assert_eq!(revision.valid_from, qualification.valid_from);
        assert_eq!(replacement_id.as_ref(), qualification.base.id.as_str());
        assert_eq!(links.len(), 2);
        assert!(links.iter().all(|link| link.qualification_id == *replacement_id));
        assert_eq!(links[0].capability_id.as_ref(), "cap-physical");
        assert_eq!(links[1].capability_id.as_ref(), "cap-virtual");
        assert_ne!(links[0].base.id, links[1].base.id);
    }

    #[test]
    fn failed_qualification_does_not_append_partial_prepared_changes() {
        let mut changes = QualificationChanges::default();
        let supplier_id = SupplierAccountId::new("supplier-1");
        let input = qualification_input();
        changes.append_created(&supplier_id, &input, &capability_ids(), "actor-1").unwrap();
        let before_created = changes.created.clone();
        let before_revisions = changes.revisions.clone();
        let before_replacements = changes.replacements.clone();
        let mut incomplete = capability_ids();
        incomplete.remove("virtual");

        let error = changes.append_created(&supplier_id, &input, &incomplete, "actor-1").unwrap_err();
        assert!(matches!(error, Error::BusinessLogicError(message) if message == "资质适用能力不存在"));
        assert_eq!(changes.created, before_created);
        assert_eq!(changes.revisions, before_revisions);
        assert_eq!(changes.replacements, before_replacements);
        assert!(changes.updated.is_empty());
    }
}
