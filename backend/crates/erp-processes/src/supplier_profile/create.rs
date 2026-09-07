//! 供应商资料创建用例与事务载荷。

use erp_audit::AuditExt;
use erp_core::ids::{
    PartyAddressId, PartyBankAccountId, PartyContactId, PartyId, PartyRevisionId, PartyTaxProfileId,
    SupplierAccountId, SupplierCapabilityId, SupplierCapabilityRevisionId,
    SupplierCommercialProfileRevisionId, SupplierQualificationCapabilityId, SupplierQualificationId,
    SupplierQualificationRevisionId, SupplierRatingRevisionId,
};
use erp_party::PartyExt;
use erp_party::{
    AddressType, EffectiveRecordStatus, Party, PartyAddress, PartyAddressData, PartyBankAccount,
    PartyBankAccountData, PartyContact, PartyContactData, PartyData, PartyKind, PartyRevision,
    PartyRevisionData, PartyStatus, PartyTaxProfile, PartyTaxProfileData,
};
use erp_supplier::SupplierExt;
use erp_supplier::{
    SupplierAccount, SupplierCapability, SupplierCapabilityRevision, SupplierCommercialProfileRevision,
    SupplierCreationIds, SupplierCreationInputs, SupplierCreationQualificationIds,
    SupplierCreationQualificationInput, SupplierCreationRatingInput, SupplierPartySeed,
    SupplierProfileCommand, SupplierProfileCommandData, SupplierQualification,
    SupplierQualificationCapability, SupplierQualificationRevision, SupplierRatingRevision,
};
use id_generator::next_id;
use mongodb::Database;
use persistence_core::Transactional;

use std::sync::Arc;

use erp_support::{EmptyPendingAttachments, PendingAttachmentBatch};

use crate::{Error, Result};
use application_core::AuditActor;
use erp_audit::AuditActorLogs;

use super::{
    validation::resolve_supplier_file_references, SupplierProfileService, SupplierProfileWithAssetsResult,
};
use erp_supplier::{command_view, SaveSupplierProfileRequest, SupplierProfileMutationView};

impl SupplierProfileService {
    /// 创建 Party、Supplier 及其当前资料；全部写入与幂等结果原子提交。
    ///
    /// # Errors
    /// 输入无效、引用主体停用、附件不存在或敏感级别不匹配、身份重复或事务失败时返回错误。
    pub async fn create(
        &self,
        req: SaveSupplierProfileRequest,
        actor: &AuditActor,
    ) -> Result<SupplierProfileMutationView> {
        Ok(self
            .create_with_assets(req, Arc::new(EmptyPendingAttachments), actor)
            .await?
            .view)
    }

    /// 创建完整供应商资料，并把同一次 multipart 命令携带的资质文件原子登记。
    ///
    /// # Errors
    /// 输入无效、文件引用或敏感级别不匹配、身份重复或事务失败时返回错误。
    pub async fn create_with_assets(
        &self,
        mut req: SaveSupplierProfileRequest,
        pending_assets: Arc<dyn PendingAttachmentBatch>,
        actor: &AuditActor,
    ) -> Result<SupplierProfileWithAssetsResult> {
        req.validate_contract()?;
        if req.clear_contact || req.clear_address || req.clear_tax_profile || req.clear_bank_account {
            return Err(Error::ValidationError(
                "创建供应商时不能提交清空既有资料的意图".to_string(),
            ));
        }
        let request_fingerprint = req.fingerprint()?;
        if let Some(command) = self.command_record(&req.idempotency_key).await? {
            command
                .ensure_replayable("create", None, &request_fingerprint)
                .map_err(|e| Error::ConflictError(e.to_string()))?;
            return Ok(SupplierProfileWithAssetsResult {
                view: command_view(command),
                assets_committed: false,
            });
        }
        let used = resolve_supplier_file_references(&mut req, pending_assets.as_ref())?;
        pending_assets.ensure_all_used(&used)?;
        let party_no =
            SaveSupplierProfileRequest::required_create_identity(req.party_no.as_deref(), "主体编号")?;
        let supplier_no =
            SaveSupplierProfileRequest::required_create_identity(req.supplier_no.as_deref(), "供应商编号")?;
        self.ensure_party_active(&req.signing_entity_party_id).await?;
        self.ensure_party_active(&req.payment_entity_party_id).await?;
        self.ensure_attachment_references(&req.qualifications, &pending_assets)
            .await?;
        self.ensure_unique_inputs(&req)?;

        let idempotency_key = req.idempotency_key.clone();
        let prepared = self.prepare_create(
            req,
            party_no,
            supplier_no,
            request_fingerprint.clone(),
            actor,
            pending_assets,
        )?;
        let result = prepared.result.clone();
        let db = self.db.clone();
        let client = db.client().clone();
        let transaction_result = client
            .with_transaction(move |session| Box::pin(async move { prepared.persist(&db, session).await }))
            .await;
        self.resolve_transaction_result_with_assets(
            transaction_result,
            result,
            &idempotency_key,
            "create",
            None,
            &request_fingerprint,
        )
        .await
    }

    /// 从已校验创建命令构造全部待写实体。
    fn prepare_create(
        &self,
        req: SaveSupplierProfileRequest,
        party_no: String,
        supplier_no: String,
        request_fingerprint: String,
        actor: &AuditActor,
        pending_assets: Arc<dyn PendingAttachmentBatch>,
    ) -> Result<PreparedCreate> {
        let party_id = PartyId::new(next_id());
        let supplier_id = SupplierAccountId::new(next_id());
        let profile_id = SupplierCommercialProfileRevisionId::new(next_id());
        let (ids, inputs) = allocate_creation_plan(
            &req,
            party_no,
            supplier_no,
            &party_id,
            &supplier_id,
            &profile_id,
            actor.id(),
        )?;
        let plan = erp_supplier::plan_supplier_creation(ids, inputs)
            .map_err(|e| Error::ValidationError(e.to_string()))?;
        let (party, party_revision) = party_from_seed(&plan.party_seed)?;
        let contact = self.create_contact(&req, &party_id, actor.id())?;
        let address = self.create_address(&req, &party_id, actor.id())?;
        let tax_profile = create_tax_profile(&req, &party_id, actor.id())?;
        let bank_account = self.create_bank_account(&req, &party_id, actor.id())?;
        let command = SupplierProfileCommand::new(
            next_id(),
            SupplierProfileCommandData {
                idempotency_key: req.idempotency_key,
                operation: "create".to_string(),
                request_fingerprint,
                supplier_id: supplier_id.to_string(),
                supplier_no: plan.supplier.supplier_no.clone(),
                revision_id: profile_id.to_string(),
                revision_no: 1,
                supplier_version: plan.supplier.base.version,
                effective_from: req.effective_from,
                change_reason: req.change_reason,
            },
        )?;
        let result = command_view(command.clone());
        let audit = actor.clone().resource_log(
            "supplier_profile.create",
            "supplier_profile",
            supplier_id.to_string(),
        )?;
        Ok(PreparedCreate {
            party,
            party_revision,
            supplier: plan.supplier,
            commercial_profile: plan.commercial_profile,
            contact,
            address,
            tax_profile,
            bank_account,
            capabilities: plan.capabilities,
            capability_revisions: plan.capability_revisions,
            qualifications: plan.qualifications,
            qualification_revisions: plan.qualification_revisions,
            qualification_links: plan.qualification_links,
            rating: plan.rating,
            command,
            audit,
            result,
            pending_assets,
        })
    }

    /// 构造并加密默认联系人。
    pub(super) fn create_contact(
        &self,
        req: &SaveSupplierProfileRequest,
        party_id: &PartyId,
        actor_id: &str,
    ) -> Result<Option<PartyContact>> {
        let Some(input) = &req.contact else {
            return Ok(None);
        };
        let mut contact = PartyContact::new(
            PartyContactId::new(next_id()),
            PartyContactData {
                party_id: party_id.clone(),
                contact_name: input.contact_name.clone(),
                title: None,
                mobile: input.mobile.clone(),
                telephone: input.telephone.clone(),
                email: input.email.clone(),
                valid_from: req.effective_from,
                valid_to: None,
                is_default: true,
                status: EffectiveRecordStatus::Active,
            },
            self.sensitive_data.fingerprint_key(),
            actor_id,
        )?;
        contact.mobile_ciphertext = self.sensitive_data.encrypt(&input.mobile)?;
        Ok(Some(contact))
    }

    /// 构造并加密默认经营地址。
    pub(super) fn create_address(
        &self,
        req: &SaveSupplierProfileRequest,
        party_id: &PartyId,
        actor_id: &str,
    ) -> Result<Option<PartyAddress>> {
        let Some(input) = &req.address else {
            return Ok(None);
        };
        let mut address = PartyAddress::new(
            PartyAddressId::new(next_id()),
            PartyAddressData {
                party_id: party_id.clone(),
                address_type: AddressType::Operating,
                contact_name: input.contact_name.clone(),
                address: input.address.clone(),
                valid_from: req.effective_from,
                valid_to: None,
                is_default: true,
                status: EffectiveRecordStatus::Active,
            },
            self.sensitive_data.fingerprint_key(),
            actor_id,
        )?;
        address.address_ciphertext = self.sensitive_data.encrypt(&input.address)?;
        Ok(Some(address))
    }

    /// 构造并加密默认银行账户。
    pub(super) fn create_bank_account(
        &self,
        req: &SaveSupplierProfileRequest,
        party_id: &PartyId,
        actor_id: &str,
    ) -> Result<Option<PartyBankAccount>> {
        let Some(input) = &req.bank_account else {
            return Ok(None);
        };
        let account_number = input.account_number.trim();
        let mut account = PartyBankAccount::new(
            PartyBankAccountId::new(next_id()),
            PartyBankAccountData {
                bank_account_no: format!("BA-{}", next_id()),
                party_id: party_id.clone(),
                account_name: req.legal_name.clone(),
                bank_name: input.bank_name.clone(),
                bank_branch_name: None,
                account_number: account_number.to_string(),
                valid_from: req.effective_from,
                valid_to: None,
                is_default: true,
                status: EffectiveRecordStatus::Active,
            },
            self.sensitive_data.fingerprint_key(),
            actor_id,
        )?;
        account.account_number_ciphertext = self.sensitive_data.encrypt(account_number)?;
        Ok(Some(account))
    }
}

/// 已校验并完成实体构造的创建事务载荷。
struct PreparedCreate {
    party: Party,
    party_revision: PartyRevision,
    supplier: SupplierAccount,
    commercial_profile: SupplierCommercialProfileRevision,
    contact: Option<PartyContact>,
    address: Option<PartyAddress>,
    tax_profile: Option<PartyTaxProfile>,
    bank_account: Option<PartyBankAccount>,
    capabilities: Vec<SupplierCapability>,
    capability_revisions: Vec<SupplierCapabilityRevision>,
    qualifications: Vec<SupplierQualification>,
    qualification_revisions: Vec<SupplierQualificationRevision>,
    qualification_links: Vec<SupplierQualificationCapability>,
    rating: Option<SupplierRatingRevision>,
    command: SupplierProfileCommand,
    audit: erp_audit::AuditLog,
    result: SupplierProfileMutationView,
    pending_assets: Arc<dyn PendingAttachmentBatch>,
}

impl PreparedCreate {
    /// 将完整供应商资料与幂等结果写入同一事务。
    async fn persist(self, db: &Database, session: &mut mongodb::ClientSession) -> Result<()> {
        self.pending_assets.persist(db, session).await?;
        db.party_revisions().create(&self.party_revision, session).await?;
        db.parties().create(&self.party, session).await?;
        db.supplier()
            .create_supplier_with_initial_profile(&self.supplier, &self.commercial_profile, session)
            .await?;
        if let Some(contact) = &self.contact {
            db.party_contacts().create(contact, session).await?;
        }
        if let Some(address) = &self.address {
            db.party_addresses().create(address, session).await?;
        }
        if let Some(tax_profile) = &self.tax_profile {
            db.party_tax_profiles().create(tax_profile, session).await?;
        }
        if let Some(bank_account) = &self.bank_account {
            db.party_bank_accounts().create(bank_account, session).await?;
        }
        for revision in &self.capability_revisions {
            db.supplier_capability_revisions()
                .create(revision, session)
                .await?;
        }
        for capability in &self.capabilities {
            db.supplier_capabilities().create(capability, session).await?;
        }
        for revision in &self.qualification_revisions {
            db.supplier_qualification_revisions()
                .create(revision, session)
                .await?;
        }
        for qualification in &self.qualifications {
            db.supplier_qualifications()
                .create(qualification, session)
                .await?;
        }
        for link in &self.qualification_links {
            db.supplier_qualification_capabilities()
                .create(link, session)
                .await?;
        }
        if let Some(rating) = &self.rating {
            db.supplier_rating_revisions().create(rating, session).await?;
        }
        db.supplier_profile_commands()
            .create(&self.command, session)
            .await?;
        db.audit_logs().create(&self.audit, session).await?;
        Ok(())
    }
}

/// Construct party identity from the supplier-domain seed.
fn party_from_seed(seed: &SupplierPartySeed) -> Result<(Party, PartyRevision)> {
    let mut party = Party::new(
        seed.party_id.clone(),
        PartyData {
            party_no: seed.party_no.clone(),
            party_kind: PartyKind::Enterprise,
            unified_credit_code: seed.unified_credit_code.clone(),
            status: PartyStatus::Active,
        },
        seed.actor_id.clone(),
    )?;
    party.stable.current_revision_id = Some(seed.party_revision_id.to_string());
    let revision = PartyRevision::new(
        seed.party_revision_id.clone(),
        PartyRevisionData {
            party_id: seed.party_id.clone(),
            revision_no: 1,
            legal_name: seed.legal_name.clone(),
            short_name: seed.short_name.clone(),
            change_reason: seed.change_reason.clone(),
        },
    )?;
    Ok((party, revision))
}

/// 分配根资料创建所需的全部主键并组织领域工厂输入（保留在 Service）。
///
/// Service 负责 ID 生成、审计操作人与 DTO 到领域输入的映射；纯构造与校验
/// 由 `erp_supplier::creation_plan` 承担。
///
/// # 参数
/// * `req` - 已校验的根资料创建请求
/// * `party_no` - 已规范化的主体编号
/// * `supplier_no` - 已规范化的供应商编号
/// * `party_id` - 已分配的主体主键
/// * `supplier_id` - 已分配的供应商角色主键
/// * `profile_id` - 已分配的首版商务资料主键
/// * `actor_id` - 操作人 ID
///
/// # 返回
/// 返回已分配主键与领域工厂输入。
///
/// # 错误
/// 无；ID 分配不失败，字段校验由领域工厂返回。
///
/// # 约束
/// 本函数可分配 ID、读取 DTO 与操作人；不得实现实体不变式。
fn allocate_creation_plan(
    req: &SaveSupplierProfileRequest,
    party_no: String,
    supplier_no: String,
    party_id: &PartyId,
    supplier_id: &SupplierAccountId,
    profile_id: &SupplierCommercialProfileRevisionId,
    actor_id: &str,
) -> Result<(SupplierCreationIds, SupplierCreationInputs)> {
    let capability_ids = req
        .capability_codes
        .iter()
        .copied()
        .map(|code| {
            (
                code,
                SupplierCapabilityId::new(next_id()),
                SupplierCapabilityRevisionId::new(next_id()),
            )
        })
        .collect();
    let qualification_ids = req
        .qualifications
        .iter()
        .map(|input| SupplierCreationQualificationIds {
            qualification_id: SupplierQualificationId::new(next_id()),
            revision_id: SupplierQualificationRevisionId::new(next_id()),
            link_ids: input
                .capability_codes
                .iter()
                .map(|_| SupplierQualificationCapabilityId::new(next_id()))
                .collect(),
        })
        .collect();
    let rating_id = req
        .rating
        .as_ref()
        .map(|_| SupplierRatingRevisionId::new(next_id()));
    let qualifications = req
        .qualifications
        .iter()
        .map(|input| SupplierCreationQualificationInput {
            qualification_type: input.qualification_type,
            certificate_no: input.certificate_no.clone(),
            issuer: input.issuer.clone(),
            valid_from: input.valid_from,
            valid_to: input.valid_to,
            attachment_id: input.attachment_id.clone(),
            capability_codes: input.capability_codes.clone(),
        })
        .collect();
    let rating = req.rating.as_ref().map(|input| SupplierCreationRatingInput {
        initial_score: input.initial_score,
        rating: input.rating,
        current_score: input.current_score,
        valid_from: input.valid_from,
    });
    let ids = SupplierCreationIds {
        party_id: party_id.clone(),
        party_revision_id: PartyRevisionId::new(next_id()),
        supplier_id: supplier_id.clone(),
        commercial_profile_id: profile_id.clone(),
        capability_ids,
        qualification_ids,
        rating_id,
    };
    let inputs = SupplierCreationInputs {
        party_no,
        supplier_no,
        legal_name: req.legal_name.clone(),
        short_name: req.short_name.clone(),
        unified_credit_code: req.unified_credit_code.clone(),
        settlement_mode: req.settlement_mode,
        reconciliation_cycle: req.reconciliation_cycle,
        payment_term_snapshot: req.payment_term_snapshot.clone(),
        business_category: req.business_category.clone(),
        invoice_type: req.invoice_type,
        invoice_tax_rate: req.invoice_tax_rate,
        signing_entity_party_id: req.signing_entity_party_id.clone(),
        payment_entity_party_id: req.payment_entity_party_id.clone(),
        capability_codes: req.capability_codes.clone(),
        qualifications,
        rating,
        effective_from: req.effective_from,
        change_reason: req.change_reason.clone(),
        actor_id: actor_id.to_string(),
    };
    Ok((ids, inputs))
}

/// 创建可选税务事实。
pub(super) fn create_tax_profile(
    req: &SaveSupplierProfileRequest,
    party_id: &PartyId,
    actor_id: &str,
) -> Result<Option<PartyTaxProfile>> {
    let Some(tax_no) = req
        .tax_no
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    Ok(Some(PartyTaxProfile::new(
        PartyTaxProfileId::new(next_id()),
        PartyTaxProfileData {
            party_id: party_id.clone(),
            tax_no: tax_no.to_string(),
            valid_from: req.effective_from,
            valid_to: None,
            is_default: true,
            status: EffectiveRecordStatus::Active,
        },
        actor_id,
    )?))
}
