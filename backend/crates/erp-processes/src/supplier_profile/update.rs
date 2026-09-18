//! 供应商资料修订用例与事务载荷。

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use application_core::AuditActor;
use erp_audit::{AuditActorLogs, AuditExt};
use erp_core::field_update::FieldUpdate;
use erp_core::ids::{
    PartyId, PartyRevisionId, SupplierAccountId, SupplierCapabilityId, SupplierCapabilityRevisionId,
    SupplierCommercialProfileRevisionId, SupplierQualificationCapabilityId, SupplierQualificationId,
    SupplierQualificationRevisionId, SupplierRatingRevisionId,
};
use erp_party::repository::prelude::*;
use erp_party::{
    Party, PartyAddress, PartyBankAccount, PartyContact, PartyExt, PartyRevision, PartyTaxProfile,
};
use erp_supplier::repository::prelude::*;
use erp_supplier::{
    QualificationStatus, SaveSupplierProfileRequest, SupplierAccount, SupplierCapability,
    SupplierCapabilityRevision, SupplierCapabilityUpdate, SupplierCommercialProfileRevision, SupplierExt,
    SupplierProfileCommand, SupplierProfileCommandData, SupplierProfileMutationView,
    SupplierProfileUpdateViolation, SupplierQualification, SupplierQualificationCapability,
    SupplierQualificationRevision, SupplierQualificationUpdate, SupplierRatingRevision,
    SupplierRatingRevisionData, command_view, next_supplier_revision_no, profile_change,
    qualification_identity_key,
};
use erp_support::{EmptyPendingAttachments, PendingAttachmentBatch};
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction, Transactional};

use super::create::create_tax_profile;
use super::validation::resolve_supplier_file_references;
use super::{SupplierProfileService, SupplierProfileWithAssetsResult, party_change};
use crate::{Error, Result};

impl SupplierProfileService {
    /// 修订完整供应商资料；全部写入与幂等结果原子提交。
    ///
    /// # Errors
    /// 输入无效、乐观锁冲突、引用失效、附件不存在或敏感级别不匹配时返回错误。
    pub async fn update(
        &self,
        supplier_id: &str,
        req: SaveSupplierProfileRequest,
        actor: &AuditActor,
    ) -> Result<SupplierProfileMutationView> {
        Ok(self.update_with_assets(supplier_id, req, Arc::new(EmptyPendingAttachments), actor).await?.view)
    }

    /// 修订完整供应商资料，并把同一次 multipart 命令携带的资质文件原子登记。
    ///
    /// # Errors
    /// 输入无效、文件引用或敏感级别不匹配、乐观锁冲突或事务失败时返回错误。
    pub async fn update_with_assets(
        &self,
        supplier_id: &str,
        mut req: SaveSupplierProfileRequest,
        pending_assets: Arc<dyn PendingAttachmentBatch>,
        actor: &AuditActor,
    ) -> Result<SupplierProfileWithAssetsResult> {
        req.validate_contract()?;
        let request_fingerprint = req.fingerprint()?;
        if let Some(command) = self.command_record(&req.idempotency_key).await? {
            command
                .ensure_replayable("update", Some(supplier_id), &request_fingerprint)
                .map_err(|e| Error::ConflictError(e.to_string()))?;
            return Ok(SupplierProfileWithAssetsResult {
                view: command_view(command),
                assets_committed: false,
            });
        }
        let used = resolve_supplier_file_references(&mut req, pending_assets.as_ref())?;
        pending_assets.ensure_all_used(&used)?;
        self.ensure_party_active(&req.signing_entity_party_id).await?;
        self.ensure_party_active(&req.payment_entity_party_id).await?;
        self.ensure_attachment_references(&req.qualifications, &pending_assets).await?;
        self.ensure_unique_inputs(&req)?;
        let idempotency_key = req.idempotency_key.clone();
        let prepared =
            self.prepare_update(supplier_id, req, request_fingerprint.clone(), actor, pending_assets).await?;
        let result = prepared.result.clone();
        let db = self.db.clone();
        let client = db.client().clone();
        let rbac = self.require_rbac()?.clone();
        let actor = actor.clone();
        let scoped_id = supplier_id.to_string();
        let transaction_result = client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    crate::adapters::supplier_access(db.clone(), rbac)
                        .require_with(&actor, "update", &scoped_id, executor)
                        .await?;
                    prepared.persist(&db, executor).await
                })
            })
            .await;
        self.resolve_transaction_result_with_assets(
            transaction_result,
            result,
            &idempotency_key,
            "update",
            Some(supplier_id),
            &request_fingerprint,
        )
        .await
    }

    /// 加载当前聚合并构造修订事务载荷。
    async fn prepare_update(
        &self,
        supplier_id: &str,
        req: SaveSupplierProfileRequest,
        request_fingerprint: String,
        actor: &AuditActor,
        pending_assets: Arc<dyn PendingAttachmentBatch>,
    ) -> Result<PreparedUpdate> {
        let mut supplier = self.load_supplier_for_update(supplier_id, &req).await?;
        let mut party = self.load_party_for_update(&supplier, &req).await?;
        let party_id = supplier.party_id.clone();
        let supplier_id = SupplierAccountId::new(supplier_id);
        let party_revision_no = self.next_party_revision_no(&party_id).await?;
        let profile_revision_no = self.next_profile_revision_no(&supplier_id).await?;
        let party_revision_id = PartyRevisionId::new(next_id());
        let commercial_profile_id = SupplierCommercialProfileRevisionId::new(next_id());
        let party_revision = party_change::plan_party_revision(party_change::PlanPartyRevisionParams {
            party: &mut party,
            unified_credit_code: req.unified_credit_code.clone(),
            legal_name: req.legal_name.clone(),
            short_name: req.short_name.clone(),
            change_reason: req.change_reason.clone(),
            revision_id: party_revision_id,
            revision_no: party_revision_no,
            actor_id: actor.id(),
        })
        .map_err(|e| Error::BusinessLogicError(e.to_string()))?;
        let commercial_profile = profile_change::plan_commercial_profile_revision(
            profile_change::PlanCommercialProfileRevisionParams {
                supplier: &mut supplier,
                settlement_mode: req.settlement_mode,
                reconciliation_cycle: req.reconciliation_cycle,
                payment_term_snapshot: req.payment_term_snapshot.clone(),
                business_category: req.business_category.clone(),
                invoice_type: req.invoice_type,
                invoice_tax_rate: req.invoice_tax_rate,
                invoice_tax_rates: req.invoice_tax_rates.clone(),
                signing_entity_party_id: req.signing_entity_party_id.clone(),
                payment_entity_party_id: req.payment_entity_party_id.clone(),
                change_reason: req.change_reason.clone(),
                revision_id: commercial_profile_id,
                revision_no: profile_revision_no,
                actor_id: actor.id(),
            },
        )
        .map_err(|e| Error::BusinessLogicError(e.to_string()))?;
        let facts = self.prepare_party_facts(&party_id, &req, actor.id()).await?;
        let capabilities = self.prepare_capability_changes(&supplier_id, &req, actor.id()).await?;
        let qualifications =
            self.prepare_qualification_changes(&supplier_id, &req, &capabilities.ids, actor.id()).await?;
        let ratings = self.prepare_rating_changes(&supplier_id, &req).await?;
        PreparedUpdate::new(
            PreparedUpdateContext {
                party,
                party_revision,
                supplier,
                commercial_profile,
                facts,
                capabilities,
                qualifications,
                ratings,
            },
            req.idempotency_key,
            request_fingerprint,
            req.effective_from,
            req.change_reason,
            actor,
            pending_assets,
        )
    }

    /// 加载并校验供应商资料修订门禁。
    ///
    /// # 参数
    /// * `supplier_id` - 待修订供应商角色 ID
    /// * `req` - 携带期望供应商版本的根资料命令
    ///
    /// # 返回
    /// 版本一致且启用的供应商实体。
    ///
    /// # 错误
    /// 供应商不存在、版本冲突、已停用或仓储查询失败时返回错误。
    async fn load_supplier_for_update(
        &self,
        supplier_id: &str,
        req: &SaveSupplierProfileRequest,
    ) -> Result<SupplierAccount> {
        let supplier = self
            .db
            .supplier()
            .account(&SupplierAccountId::new(supplier_id), &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商不存在".to_string()))?;
        let expected =
            SaveSupplierProfileRequest::required_update_version(req.expected_supplier_version, "供应商")?;
        match supplier.profile_update_violation(expected) {
            None => {
                supplier
                    .ensure_responsibility()
                    .map_err(|error| Error::ValidationError(error.to_string()))?;
                Ok(supplier)
            },
            Some(SupplierProfileUpdateViolation::VersionMismatch) => {
                Err(Error::ConflictError("数据已被其他请求修改，请刷新后重试".to_string()))
            },
            Some(SupplierProfileUpdateViolation::SupplierDisabled) => {
                Err(Error::BusinessLogicError("供应商已停用，不能修订资料".to_string()))
            },
        }
    }

    /// 加载并校验供应商关联主体乐观锁与启停状态。
    ///
    /// # 参数
    /// * `supplier` - 已通过修订门禁的供应商实体
    /// * `req` - 携带期望主体版本的根资料命令
    ///
    /// # 返回
    /// 版本一致且启用的关联主体实体。
    ///
    /// # 错误
    /// 主体不存在、版本冲突、已停用或仓储查询失败时返回错误。
    async fn load_party_for_update(
        &self,
        supplier: &SupplierAccount,
        req: &SaveSupplierProfileRequest,
    ) -> Result<Party> {
        let party = self
            .db
            .party()
            .party(&supplier.party_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商关联主体不存在".to_string()))?;
        let expected =
            SaveSupplierProfileRequest::required_update_version(req.expected_party_version, "主体")?;
        SaveSupplierProfileRequest::ensure_version(party.base.version, expected)?;
        if !party.is_active() {
            return Err(Error::BusinessLogicError("供应商关联主体已停用".to_string()));
        }
        Ok(party)
    }

    /// 查询下一主体修订号。
    ///
    /// # 参数
    /// * `party_id` - 稳定主体 ID
    ///
    /// # 返回
    /// 返回无历史时为一、否则为当前最大值加一的修订号。
    ///
    /// # 错误
    /// 仓储查询、反序列化或修订号溢出时返回错误。
    async fn next_party_revision_no(&self, party_id: &PartyId) -> Result<u32> {
        Ok(self.db.party_revisions().next_revision_no(party_id, &mut NoTransaction).await?)
    }

    /// 查询下一商务资料修订号。
    ///
    /// # 参数
    /// * `supplier_id` - 供应商角色 ID
    ///
    /// # 返回
    /// 返回无历史时为一、否则为当前最大值加一的修订号。
    ///
    /// # 错误
    /// 仓储查询、反序列化或修订号溢出时返回错误。
    async fn next_profile_revision_no(&self, supplier_id: &SupplierAccountId) -> Result<u32> {
        Ok(self.db.supplier().next_commercial_profile_revision_no(supplier_id, &mut NoTransaction).await?)
    }

    /// 构造联系人、地址、税务与银行账户事实的追加或停用变更。
    ///
    /// # 参数
    /// * `party_id` - 供应商关联主体 ID
    /// * `req` - 根资料命令中的事实替换与清空意图
    /// * `actor_id` - 执行变更的账号 ID
    ///
    /// # 返回
    /// 返回待在同一事务中持久化的主体事实变更集合。
    ///
    /// # 错误
    /// 仓储查询、事实状态迁移、加密或实体构造失败时返回错误。
    async fn prepare_party_facts(
        &self,
        party_id: &PartyId,
        req: &SaveSupplierProfileRequest,
        actor_id: &str,
    ) -> Result<PartyFactChanges> {
        let mut changes = PartyFactChanges::default();
        if req.contact.is_some() || req.clear_contact {
            changes.contacts = self.db.party_contacts().list_by_party(party_id, &mut NoTransaction).await?;
            party_change::disable_contacts(&mut changes.contacts, actor_id)
                .map_err(|e| Error::BusinessLogicError(e.to_string()))?;
            changes.new_contact = self.create_contact(req, party_id, actor_id)?;
        }
        if req.address.is_some() || req.clear_address {
            changes.addresses = self.db.party_addresses().list_by_party(party_id, &mut NoTransaction).await?;
            party_change::disable_addresses(&mut changes.addresses, actor_id)
                .map_err(|e| Error::BusinessLogicError(e.to_string()))?;
            changes.new_address = self.create_address(req, party_id, actor_id)?;
        }
        if req.clear_tax_profile || req.tax_no.as_deref().is_some_and(|value| !value.trim().is_empty()) {
            changes.tax_profiles =
                self.db.party_tax_profiles().list_by_party(party_id, &mut NoTransaction).await?;
            party_change::disable_tax_profiles(&mut changes.tax_profiles, actor_id)
                .map_err(|e| Error::BusinessLogicError(e.to_string()))?;
            changes.new_tax_profile = create_tax_profile(req, party_id, actor_id)?;
        }
        if req.bank_account.is_some() || req.clear_bank_account {
            changes.bank_accounts =
                self.db.party_bank_accounts().list_by_party(party_id, &mut NoTransaction).await?;
            party_change::disable_bank_accounts(&mut changes.bank_accounts, actor_id)
                .map_err(|e| Error::BusinessLogicError(e.to_string()))?;
            changes.new_bank_account = self.create_bank_account(req, party_id, actor_id)?;
        }
        Ok(changes)
    }

    /// 将能力代码集合解析为新增、启停与不可变快照。
    ///
    /// # 参数
    /// * `supplier_id` - 供应商角色 ID
    /// * `req` - 已校验的根资料命令
    /// * `actor_id` - 执行变更的账号 ID
    ///
    /// # 返回
    /// 返回待事务持久化的能力实体、修订和稳定 ID 映射。
    ///
    /// # 错误
    /// 仓储查询、状态迁移、修订号生成或实体构造失败时返回错误。
    async fn prepare_capability_changes(
        &self,
        supplier_id: &SupplierAccountId,
        req: &SaveSupplierProfileRequest,
        actor_id: &str,
    ) -> Result<CapabilityChanges> {
        let existing = self.db.supplier().list_capabilities(supplier_id, &mut NoTransaction).await?;
        let plan = profile_change::SupplierProfileChangePlan::from_loaded(
            &existing,
            &[],
            &HashMap::new(),
            &HashMap::new(),
            &req.capability_codes,
            &[],
        )
        .map_err(|e| Error::ValidationError(e.to_string()))?;
        let mut changes = CapabilityChanges::default();
        let mut existing_by_code: HashMap<String, SupplierCapability> =
            existing.into_iter().map(|cap| (cap.capability_code.as_str().to_string(), cap)).collect();
        for cap in existing_by_code.values() {
            changes
                .ids
                .insert(cap.capability_code.as_str().to_string(), SupplierCapabilityId::new(&cap.base.id));
        }
        for toggle in plan.capability_toggles {
            let mut capability = existing_by_code
                .remove(toggle.code.as_str())
                .ok_or_else(|| Error::Internal("能力计划与已加载事实不一致".to_string()))?;
            capability
                .update(
                    SupplierCapabilityUpdate {
                        service_region: FieldUpdate::Unchanged,
                        owner_user_id: None,
                        fulfillment_note: FieldUpdate::Unchanged,
                        valid_to: FieldUpdate::Unchanged,
                        status: Some(toggle.target_status),
                    },
                    actor_id,
                )
                .map_err(|e| Error::BusinessLogicError(e.to_string()))?;
            let revision_no = self
                .db
                .supplier()
                .next_capability_revision_no(
                    &capability.supplier_id,
                    capability.capability_code,
                    &mut NoTransaction,
                )
                .await?;
            let revision_id = SupplierCapabilityRevisionId::new(next_id());
            capability.stable.current_revision_id = Some(revision_id.to_string());
            let revision = capability
                .snapshot_revision(revision_id, revision_no)
                .map_err(|e| Error::BusinessLogicError(e.to_string()))?;
            changes.updated.push(capability);
            changes.revisions.push(revision);
        }
        for code in plan.capability_creates {
            let capability_id = SupplierCapabilityId::new(next_id());
            let revision_id = SupplierCapabilityRevisionId::new(next_id());
            let owner = req
                .capability_owners
                .iter()
                .find(|item| item.capability_code == code && !item.owner_user_id.trim().is_empty())
                .map(|item| item.owner_user_id.trim().to_string())
                .ok_or_else(|| Error::ValidationError("供给能力负责人必须显式指定".to_string()))?;
            let (capability, revision) = profile_change::new_capability(
                supplier_id,
                code,
                req.effective_from,
                &owner,
                actor_id,
                capability_id.clone(),
                revision_id,
            )
            .map_err(|e| Error::BusinessLogicError(e.to_string()))?;
            changes.ids.insert(code.as_str().to_string(), SupplierCapabilityId::new(&capability.base.id));
            changes.created.push(capability);
            changes.revisions.push(revision);
        }
        Ok(changes)
    }

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
    async fn prepare_qualification_changes(
        &self,
        supplier_id: &SupplierAccountId,
        req: &SaveSupplierProfileRequest,
        capability_ids: &HashMap<String, SupplierCapabilityId>,
        actor_id: &str,
    ) -> Result<QualificationChanges> {
        let existing = self.db.supplier().list_qualifications(supplier_id, &mut NoTransaction).await?;
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
        let planned_inputs: Vec<profile_change::PlannedQualificationInput> = req
            .qualifications
            .iter()
            .map(|input| profile_change::PlannedQualificationInput {
                qualification_type: input.qualification_type,
                certificate_no: input.certificate_no.clone(),
                issuer: input.issuer.clone(),
                valid_from: input.valid_from,
                valid_to: input.valid_to,
                attachment_id: input.attachment_id.clone(),
                capability_codes: input.capability_codes.clone(),
            })
            .collect();
        let plan = profile_change::SupplierProfileChangePlan::from_loaded(
            &[],
            &existing,
            &linked_capabilities,
            capability_ids,
            &[],
            &planned_inputs,
        )
        .map_err(|e| Error::ValidationError(e.to_string()))?;
        let requested_map: HashMap<String, &profile_change::PlannedQualificationInput> = planned_inputs
            .iter()
            .map(|input| (qualification_identity_key(input.qualification_type, &input.certificate_no), input))
            .collect();
        let mut existing_by_key: HashMap<String, SupplierQualification> =
            existing.into_iter().map(|qual| (qual.identity_key(), qual)).collect();
        let mut changes = QualificationChanges::default();
        for key in plan.qualification_updates {
            let mut qualification = existing_by_key
                .remove(&key)
                .ok_or_else(|| Error::Internal("资质计划与已加载事实不一致".to_string()))?;
            let input =
                requested_map.get(&key).ok_or_else(|| Error::Internal("资质请求与计划不一致".to_string()))?;
            profile_change::apply_qualification_input(
                &mut qualification,
                input.issuer.clone(),
                input.valid_from,
                input.valid_to,
                input.attachment_id.clone(),
                actor_id,
            )
            .map_err(|e| Error::BusinessLogicError(e.to_string()))?;
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
            let revision = qualification
                .snapshot_revision(revision_id, revision_no)
                .map_err(|e| Error::BusinessLogicError(e.to_string()))?;
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
        }
        for key in plan.qualification_disables {
            let mut qualification = existing_by_key
                .remove(&key)
                .ok_or_else(|| Error::Internal("资质停用计划与已加载事实不一致".to_string()))?;
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
            let revision = qualification
                .snapshot_revision(revision_id, revision_no)
                .map_err(|e| Error::BusinessLogicError(e.to_string()))?;
            changes.updated.push(qualification);
            changes.revisions.push(revision);
        }
        for input in plan.qualification_creates {
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
            changes.replacements.push((SupplierQualificationId::new(&qualification.base.id), links));
            changes.created.push(qualification);
            changes.revisions.push(revision);
        }
        Ok(changes)
    }

    /// 构造评级开放区间关闭与下一评级版本。
    ///
    /// # 参数
    /// * `supplier_id` - 供应商角色 ID
    /// * `req` - 可选携带评级输入的根资料命令
    ///
    /// # 返回
    /// 无评级或评级未变化时返回空变更，否则返回待关闭版本与新版本。
    ///
    /// # 错误
    /// 历史查询、修订号溢出、区间关闭或实体构造失败时返回错误。
    async fn prepare_rating_changes(
        &self,
        supplier_id: &SupplierAccountId,
        req: &SaveSupplierProfileRequest,
    ) -> Result<RatingChanges> {
        let Some(input) = &req.rating else {
            return Ok(RatingChanges::default());
        };
        let history = self.db.supplier().list_rating_history(supplier_id, &mut NoTransaction).await?;
        let next_no = next_supplier_revision_no(history.iter().map(|item| item.revision.revision_no))?;
        let mut current = history.last().cloned();
        if current.as_ref().is_some_and(|previous| {
            previous.rating == input.rating && previous.current_score == input.current_score
        }) {
            return Ok(RatingChanges::default());
        }
        if let Some(previous) = current.as_mut() {
            previous.close_before(input.valid_from)?;
        }
        let created = SupplierRatingRevision::new(
            SupplierRatingRevisionId::new(next_id()),
            SupplierRatingRevisionData {
                supplier_id: supplier_id.clone(),
                revision_no: next_no,
                initial_score: (next_no == 1).then_some(input.initial_score).flatten(),
                rating: input.rating,
                current_score: input.current_score,
                valid_from: input.valid_from,
                valid_to: None,
                change_reason: req.change_reason.clone(),
            },
        )?;
        Ok(RatingChanges { current, created: Some(created) })
    }
}

/// 主体从属事实的追加式变更。
#[derive(Default)]
struct PartyFactChanges {
    contacts: Vec<PartyContact>,
    new_contact: Option<PartyContact>,
    addresses: Vec<PartyAddress>,
    new_address: Option<PartyAddress>,
    tax_profiles: Vec<PartyTaxProfile>,
    new_tax_profile: Option<PartyTaxProfile>,
    bank_accounts: Vec<PartyBankAccount>,
    new_bank_account: Option<PartyBankAccount>,
}

/// 能力当前集合变更及其不可变快照。
#[derive(Default)]
struct CapabilityChanges {
    ids: HashMap<String, SupplierCapabilityId>,
    created: Vec<SupplierCapability>,
    updated: Vec<SupplierCapability>,
    revisions: Vec<SupplierCapabilityRevision>,
}

/// 资质当前集合变更、不可变快照与能力关联替换。
#[derive(Default)]
struct QualificationChanges {
    created: Vec<SupplierQualification>,
    updated: Vec<SupplierQualification>,
    revisions: Vec<SupplierQualificationRevision>,
    replacements: Vec<(SupplierQualificationId, Vec<SupplierQualificationCapability>)>,
}

/// 评级开放区间关闭与下一版本。
#[derive(Default)]
struct RatingChanges {
    current: Option<SupplierRatingRevision>,
    created: Option<SupplierRatingRevision>,
}

/// 供应商资料修订的主体与变更集合。
///
/// # 用途
/// 将 Party/供应商根与事实差异打包，供 [`PreparedUpdate::new`] 构造事务载荷。
///
/// # 参数
/// 无
///
/// # 返回
/// 无
///
/// # 错误
/// 无
///
/// # 关键业务约束
/// 主体与差异必须已完成实体构造，本结构不再二次校验。
struct PreparedUpdateContext {
    /// 已更新的 Party 根。
    party: Party,
    /// 新 Party 修订。
    party_revision: PartyRevision,
    /// 已更新的供应商根。
    supplier: SupplierAccount,
    /// 新商业资料修订。
    commercial_profile: SupplierCommercialProfileRevision,
    /// 从属事实差异。
    facts: PartyFactChanges,
    /// 能力差异。
    capabilities: CapabilityChanges,
    /// 资质差异。
    qualifications: QualificationChanges,
    /// 评级差异。
    ratings: RatingChanges,
}

/// 已校验并完成实体构造的修订事务载荷。
struct PreparedUpdate {
    party: Party,
    party_revision: PartyRevision,
    supplier: SupplierAccount,
    commercial_profile: SupplierCommercialProfileRevision,
    facts: PartyFactChanges,
    capabilities: CapabilityChanges,
    qualifications: QualificationChanges,
    ratings: RatingChanges,
    command: SupplierProfileCommand,
    audit: erp_audit::AuditLog,
    result: SupplierProfileMutationView,
    pending_assets: Arc<dyn PendingAttachmentBatch>,
}

impl PreparedUpdate {
    /// 构造修订结果、幂等记录与根审计。
    ///
    /// # 用途
    /// 由已构造主体与变更集合生成幂等命令、审计与稳定结果。
    ///
    /// # 参数
    /// * `context` - Party/供应商根与变更集合
    /// * `idempotency_key` - 客户端幂等键
    /// * `request_fingerprint` - 请求摘要
    /// * `effective_from` - 生效起始日
    /// * `change_reason` - 变更原因
    /// * `actor` - 审计操作人
    ///
    /// # 返回
    /// 返回可落库的修订事务载荷。
    ///
    /// # 错误
    /// 命令字段非法时返回错误。
    ///
    /// # 关键业务约束
    /// 供应商版本按当前根版本加一写入命令。
    fn new(
        context: PreparedUpdateContext,
        idempotency_key: String,
        request_fingerprint: String,
        effective_from: erp_core::common::time::BusinessDate,
        change_reason: String,
        actor: &AuditActor,
        pending_assets: Arc<dyn PendingAttachmentBatch>,
    ) -> Result<Self> {
        let PreparedUpdateContext {
            party,
            party_revision,
            supplier,
            commercial_profile,
            facts,
            capabilities,
            qualifications,
            ratings,
        } = context;
        let command = SupplierProfileCommand::new(
            next_id(),
            SupplierProfileCommandData {
                idempotency_key,
                operation: "update".to_string(),
                request_fingerprint,
                supplier_id: supplier.base.id.clone(),
                supplier_no: supplier.supplier_no.clone(),
                revision_id: commercial_profile.base.id.clone(),
                revision_no: commercial_profile.revision.revision_no,
                supplier_version: supplier.base.version + 1,
                effective_from,
                change_reason,
            },
        )?;
        let result = command_view(command.clone());
        let audit = actor.clone().resource_log(
            "supplier_profile.update",
            "supplier_profile",
            result.supplier_id.clone(),
        )?;
        Ok(Self {
            party,
            party_revision,
            supplier,
            commercial_profile,
            facts,
            capabilities,
            qualifications,
            ratings,
            command,
            audit,
            result,
            pending_assets,
        })
    }

    /// 将完整资料修订与幂等结果写入同一事务。
    async fn persist(mut self, db: &Database, executor: &mut dyn Executor) -> Result<()> {
        self.pending_assets.persist(db, executor).await?;
        self.persist_roots(db, executor).await?;
        self.facts.persist(db, executor).await?;
        self.capabilities.persist(db, executor).await?;
        self.qualifications.persist(db, executor).await?;
        self.ratings.persist(db, executor).await?;
        db.supplier_profile_commands().create(&self.command, executor).await?;
        db.audit_logs().create(&self.audit, executor).await?;
        Ok(())
    }

    /// 写入 Party/Supplier 根及新修订。
    async fn persist_roots(&mut self, db: &Database, executor: &mut dyn Executor) -> Result<()> {
        db.party_revisions().create(&self.party_revision, executor).await?;
        db.parties().update(&mut self.party, executor).await?;
        db.supplier_commercial_profile_revisions().create(&self.commercial_profile, executor).await?;
        db.supplier_accounts().update(&mut self.supplier, executor).await?;
        Ok(())
    }
}

impl PartyFactChanges {
    /// 写入从属事实的停用与新事实行。
    async fn persist(mut self, db: &Database, executor: &mut dyn Executor) -> Result<()> {
        for item in &mut self.contacts {
            db.party_contacts().update(item, executor).await?;
        }
        if let Some(item) = &self.new_contact {
            db.party_contacts().create(item, executor).await?;
        }
        for item in &mut self.addresses {
            db.party_addresses().update(item, executor).await?;
        }
        if let Some(item) = &self.new_address {
            db.party_addresses().create(item, executor).await?;
        }
        for item in &mut self.tax_profiles {
            db.party_tax_profiles().update(item, executor).await?;
        }
        if let Some(item) = &self.new_tax_profile {
            db.party_tax_profiles().create(item, executor).await?;
        }
        for item in &mut self.bank_accounts {
            db.party_bank_accounts().update(item, executor).await?;
        }
        if let Some(item) = &self.new_bank_account {
            db.party_bank_accounts().create(item, executor).await?;
        }
        Ok(())
    }
}

impl CapabilityChanges {
    /// 写入能力快照及当前实体变更。
    async fn persist(mut self, db: &Database, executor: &mut dyn Executor) -> Result<()> {
        for revision in &self.revisions {
            db.supplier_capability_revisions().create(revision, executor).await?;
        }
        for capability in &self.created {
            db.supplier_capabilities().create(capability, executor).await?;
        }
        for capability in &mut self.updated {
            db.supplier_capabilities().update(capability, executor).await?;
        }
        Ok(())
    }
}

impl QualificationChanges {
    /// 写入资质快照、当前实体和整体替换后的能力关联。
    async fn persist(mut self, db: &Database, executor: &mut dyn Executor) -> Result<()> {
        for revision in &self.revisions {
            db.supplier_qualification_revisions().create(revision, executor).await?;
        }
        for qualification in &self.created {
            db.supplier_qualifications().create(qualification, executor).await?;
        }
        for qualification in &mut self.updated {
            db.supplier_qualifications().update(qualification, executor).await?;
        }
        for (qualification_id, links) in self.replacements {
            db.supplier().replace_qualification_capabilities(&qualification_id, links, executor).await?;
        }
        Ok(())
    }
}

impl RatingChanges {
    /// 关闭上一开放区间并写入下一评级版本。
    async fn persist(mut self, db: &Database, executor: &mut dyn Executor) -> Result<()> {
        if let Some(current) = self.current.as_mut() {
            db.supplier_rating_revisions().update(current, executor).await?;
        }
        if let Some(created) = &self.created {
            db.supplier_rating_revisions().create(created, executor).await?;
        }
        Ok(())
    }
}
