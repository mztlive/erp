//! 供应商资料根级命令。
//!
//! 页面只调用本服务维护 Party、Supplier、当前事实与独立资质；服务在一个
//! MongoDB 事务中提交全部写入，并把幂等结果与业务数据一并落库。

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use database::{AccessControlExt, FileAssetExt, NoTransaction, PartyExt, SupplierExt, Transactional};
use entities::{
    common::time::Instant,
    field_update::FieldUpdate,
    ids::{
        PartyAddressId, PartyBankAccountId, PartyContactId, PartyId, PartyRevisionId, PartyTaxProfileId,
        SupplierAccountId, SupplierCapabilityId, SupplierCapabilityRevisionId,
        SupplierCommercialProfileRevisionId, SupplierQualificationCapabilityId, SupplierQualificationId,
        SupplierQualificationRevisionId, SupplierRatingRevisionId,
    },
    party::{
        AddressType, EffectiveRecordStatus, Party, PartyAddress, PartyAddressData, PartyAddressUpdate,
        PartyBankAccount, PartyBankAccountData, PartyBankAccountUpdate, PartyContact, PartyContactData,
        PartyContactUpdate, PartyData, PartyKind, PartyRevision, PartyRevisionData, PartyStatus,
        PartyTaxProfile, PartyTaxProfileData, PartyTaxProfileUpdate, PartyUpdate,
    },
    supplier::{
        CapabilityStatus, QualificationStatus, SupplierAccount, SupplierAccountData, SupplierAccountStatus,
        SupplierAccountUpdate, SupplierCapability, SupplierCapabilityData, SupplierCapabilityRevision,
        SupplierCapabilityRevisionData, SupplierCapabilityUpdate, SupplierCommercialProfileRevision,
        SupplierCommercialProfileRevisionData, SupplierProfileCommand, SupplierProfileCommandData,
        SupplierQualification, SupplierQualificationCapability, SupplierQualificationCapabilityData,
        SupplierQualificationData, SupplierQualificationRevision, SupplierQualificationRevisionData,
        SupplierQualificationUpdate, SupplierRatingRevision, SupplierRatingRevisionData,
    },
};
use id_generator::next_id;
use mongodb::{bson::doc, Database};
use sha2::{Digest, Sha256};
use validator::Validate;

use crate::{
    audit::AuditActor,
    errors::{Error, Result},
    file_asset::PendingFileAssetRequest,
    party::{SensitiveDataCodec, SensitiveFieldKind},
    pending_file_assets::PendingFileAssets,
};

use super::{
    RevealSupplierSensitiveRequest, SaveSupplierProfileRequest, SupplierProfileMutationView,
    SupplierProfileQualificationInput, SupplierSensitiveRevealView,
};

/// 完整供应商资料的根级写服务。
pub struct SupplierProfileService {
    db: Database,
    sensitive_data: Arc<SensitiveDataCodec>,
}

/// 携带文件资产的供应商根命令执行结果。
pub struct SupplierProfileWithAssetsResult {
    /// 稳定业务结果。
    pub view: SupplierProfileMutationView,
    /// 本次上传对象是否已随业务事务登记；幂等重放时为 `false`。
    pub assets_committed: bool,
}

impl SupplierProfileService {
    /// 创建根级供应商资料服务。
    pub fn new(db: Database, sensitive_data: Arc<SensitiveDataCodec>) -> Self {
        Self { db, sensitive_data }
    }

    /// 创建 Party、Supplier 及其当前资料；全部写入与幂等结果原子提交。
    ///
    /// # Errors
    /// 输入无效、引用主体停用、附件不存在或敏感级别不匹配、身份重复或事务失败时返回错误。
    pub async fn create(
        &self,
        req: SaveSupplierProfileRequest,
        actor: &AuditActor,
    ) -> Result<SupplierProfileMutationView> {
        Ok(self.create_with_assets(req, Vec::new(), actor).await?.view)
    }

    /// 创建完整供应商资料，并把同一次 multipart 命令携带的资质文件原子登记。
    ///
    /// # Errors
    /// 输入无效、文件引用或敏感级别不匹配、身份重复或事务失败时返回错误。
    pub async fn create_with_assets(
        &self,
        mut req: SaveSupplierProfileRequest,
        asset_requests: Vec<PendingFileAssetRequest>,
        actor: &AuditActor,
    ) -> Result<SupplierProfileWithAssetsResult> {
        self.validate_request(&req)?;
        if req.clear_contact || req.clear_address || req.clear_tax_profile || req.clear_bank_account {
            return Err(Error::ValidationError(
                "创建供应商时不能提交清空既有资料的意图".to_string(),
            ));
        }
        let request_fingerprint = request_fingerprint(&req)?;
        if let Some(command) = self.command_record(&req.idempotency_key).await? {
            return Ok(SupplierProfileWithAssetsResult {
                view: replay_command(command, "create", None, &request_fingerprint)?,
                assets_committed: false,
            });
        }
        let pending_assets = PendingFileAssets::prepare(asset_requests, actor)?;
        let used = resolve_supplier_file_references(&mut req, &pending_assets)?;
        pending_assets.ensure_all_used(&used)?;
        let party_no = required_create_identity(req.party_no.as_deref(), "主体编号")?;
        let supplier_no = required_create_identity(req.supplier_no.as_deref(), "供应商编号")?;
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
        Ok(self
            .update_with_assets(supplier_id, req, Vec::new(), actor)
            .await?
            .view)
    }

    /// 修订完整供应商资料，并把同一次 multipart 命令携带的资质文件原子登记。
    ///
    /// # Errors
    /// 输入无效、文件引用或敏感级别不匹配、乐观锁冲突或事务失败时返回错误。
    pub async fn update_with_assets(
        &self,
        supplier_id: &str,
        mut req: SaveSupplierProfileRequest,
        asset_requests: Vec<PendingFileAssetRequest>,
        actor: &AuditActor,
    ) -> Result<SupplierProfileWithAssetsResult> {
        self.validate_request(&req)?;
        let request_fingerprint = request_fingerprint(&req)?;
        if let Some(command) = self.command_record(&req.idempotency_key).await? {
            return Ok(SupplierProfileWithAssetsResult {
                view: replay_command(command, "update", Some(supplier_id), &request_fingerprint)?,
                assets_committed: false,
            });
        }
        let pending_assets = PendingFileAssets::prepare(asset_requests, actor)?;
        let used = resolve_supplier_file_references(&mut req, &pending_assets)?;
        pending_assets.ensure_all_used(&used)?;
        self.ensure_party_active(&req.signing_entity_party_id).await?;
        self.ensure_party_active(&req.payment_entity_party_id).await?;
        self.ensure_attachment_references(&req.qualifications, &pending_assets)
            .await?;
        self.ensure_unique_inputs(&req)?;
        let idempotency_key = req.idempotency_key.clone();
        let prepared = self
            .prepare_update(
                supplier_id,
                req,
                request_fingerprint.clone(),
                actor,
                pending_assets,
            )
            .await?;
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
            "update",
            Some(supplier_id),
            &request_fingerprint,
        )
        .await
    }

    /// 按幂等键查询已成功的根级命令结果。
    ///
    /// # Errors
    /// 查询失败时返回仓储错误。
    pub async fn command_result(&self, idempotency_key: &str) -> Result<Option<SupplierProfileMutationView>> {
        Ok(self.command_record(idempotency_key).await?.map(command_view))
    }

    /// 加载幂等命令实体，供请求一致性与并发恢复校验使用。
    async fn command_record(&self, idempotency_key: &str) -> Result<Option<SupplierProfileCommand>> {
        Ok(self
            .db
            .supplier_profile_commands()
            .find_by_idempotency_key(idempotency_key, &mut NoTransaction)
            .await?)
    }

    /// 事务失败时查询同幂等键结果；并发首请求已提交时返回该稳定结果。
    async fn resolve_transaction_result_with_assets(
        &self,
        transaction_result: Result<()>,
        intended_result: SupplierProfileMutationView,
        idempotency_key: &str,
        operation: &str,
        supplier_id: Option<&str>,
        request_fingerprint: &str,
    ) -> Result<SupplierProfileWithAssetsResult> {
        match transaction_result {
            Ok(()) => Ok(SupplierProfileWithAssetsResult {
                view: intended_result,
                assets_committed: true,
            }),
            Err(error) => {
                let assets_may_be_committed = matches!(&error, Error::OutcomeUnknown(_));
                match self.command_record(idempotency_key).await? {
                    Some(command) => Ok(SupplierProfileWithAssetsResult {
                        view: replay_command(command, operation, supplier_id, request_fingerprint)?,
                        assets_committed: assets_may_be_committed,
                    }),
                    None => Err(error),
                }
            }
        }
    }

    /// 验证短时令牌、归属与权限入口后解密单个敏感字段并记录审计。
    ///
    /// # Errors
    /// 令牌非法/过期、记录不属于令牌供应商、旧数据无密文或审计写入失败时返回错误。
    pub async fn reveal_sensitive(
        &self,
        req: RevealSupplierSensitiveRequest,
        actor: &AuditActor,
    ) -> Result<SupplierSensitiveRevealView> {
        req.validate()?;
        let now = u64::try_from(Instant::now().unix_secs())
            .map_err(|_| Error::Internal("系统时间非法".to_string()))?;
        let scope = self.sensitive_data.verify_reveal_token(&req.reveal_token, now)?;
        let supplier = self
            .db
            .supplier_accounts()
            .find_by_id(&scope.supplier_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商不存在".to_string()))?;
        let ciphertext = match scope.kind {
            SensitiveFieldKind::ContactMobile => {
                let record = self
                    .db
                    .party_contacts()
                    .find_by_id(&scope.record_id, &mut NoTransaction)
                    .await?
                    .ok_or_else(|| Error::NotFound("联系人不存在".to_string()))?;
                ensure_sensitive_party(&record.party_id, &supplier.party_id)?;
                record.mobile_ciphertext
            }
            SensitiveFieldKind::Address => {
                let record = self
                    .db
                    .party_addresses()
                    .find_by_id(&scope.record_id, &mut NoTransaction)
                    .await?
                    .ok_or_else(|| Error::NotFound("地址不存在".to_string()))?;
                ensure_sensitive_party(&record.party_id, &supplier.party_id)?;
                record.address_ciphertext
            }
            SensitiveFieldKind::BankAccountNumber => {
                let record = self
                    .db
                    .party_bank_accounts()
                    .find_by_id(&scope.record_id, &mut NoTransaction)
                    .await?
                    .ok_or_else(|| Error::NotFound("银行账户不存在".to_string()))?;
                ensure_sensitive_party(&record.party_id, &supplier.party_id)?;
                record.account_number_ciphertext
            }
        };
        let value = self.sensitive_data.decrypt(&ciphertext)?;
        let audit =
            actor
                .clone()
                .resource_log("supplier_sensitive.reveal", "supplier_sensitive", scope.record_id)?;
        self.db.audit_logs().create(&audit, &mut NoTransaction).await?;
        Ok(SupplierSensitiveRevealView { value })
    }

    /// 校验根级 DTO 及嵌套输入。
    fn validate_request(&self, req: &SaveSupplierProfileRequest) -> Result<()> {
        req.validate()?;
        if req.clear_contact && req.contact.is_some() {
            return Err(Error::ValidationError("联系人不能同时替换和清空".to_string()));
        }
        if req.clear_address && req.address.is_some() {
            return Err(Error::ValidationError("经营地址不能同时替换和清空".to_string()));
        }
        if req.clear_tax_profile
            && req
                .tax_no
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty())
        {
            return Err(Error::ValidationError("税务档案不能同时替换和清空".to_string()));
        }
        if req.clear_bank_account && req.bank_account.is_some() {
            return Err(Error::ValidationError("银行账户不能同时替换和清空".to_string()));
        }
        if let Some(contact) = &req.contact {
            contact.validate()?;
        }
        if let Some(address) = &req.address {
            address.validate()?;
        }
        if let Some(bank_account) = &req.bank_account {
            bank_account.validate()?;
        }
        for qualification in &req.qualifications {
            qualification.validate()?;
        }
        Ok(())
    }

    /// 校验签约/付款主体存在且启用。
    async fn ensure_party_active(&self, party_id: &PartyId) -> Result<()> {
        let party = self
            .db
            .parties()
            .find_by_id(party_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("签约或付款主体不存在".to_string()))?;
        if !party.is_active() {
            return Err(Error::BusinessLogicError("签约或付款主体已停用".to_string()));
        }
        Ok(())
    }

    /// 校验资质附件存在且敏感级别符合附件用途。
    async fn ensure_attachment_references(
        &self,
        qualifications: &[SupplierProfileQualificationInput],
        pending_assets: &PendingFileAssets,
    ) -> Result<()> {
        for qualification in qualifications {
            let Some(attachment_id) = qualification.attachment_id.as_ref() else {
                continue;
            };
            let sensitivity = match pending_assets.sensitivity(attachment_id) {
                Some(sensitivity) => sensitivity,
                None => {
                    self.db
                        .file_assets()
                        .find_by_id(attachment_id, &mut NoTransaction)
                        .await?
                        .ok_or_else(|| Error::NotFound("资质附件不存在，请先上传文件".to_string()))?
                        .sensitivity_class
                }
            };
            ensure_qualification_sensitivity(qualification, sensitivity)?;
        }
        Ok(())
    }

    /// 拒绝重复能力代码、重复资质身份以及引用未勾选能力。
    fn ensure_unique_inputs(&self, req: &SaveSupplierProfileRequest) -> Result<()> {
        let mut capability_codes = req.capability_codes.clone();
        capability_codes.sort_by_key(|code| code.as_str());
        let before = capability_codes.len();
        capability_codes.dedup();
        if capability_codes.len() != before {
            return Err(Error::ValidationError("供应商能力不能重复".to_string()));
        }
        let mut qualification_keys: Vec<String> = req
            .qualifications
            .iter()
            .map(|qualification| {
                format!(
                    "{}::{}",
                    qualification.qualification_type.as_str(),
                    qualification.certificate_no.trim()
                )
            })
            .collect();
        qualification_keys.sort();
        let before = qualification_keys.len();
        qualification_keys.dedup();
        if qualification_keys.len() != before {
            return Err(Error::ValidationError("同类资质编号不能重复".to_string()));
        }
        for code in req
            .qualifications
            .iter()
            .flat_map(|qualification| &qualification.capability_codes)
        {
            if !capability_codes.contains(code) {
                return Err(Error::ValidationError("资质引用了未启用的供应商能力".to_string()));
            }
        }
        Ok(())
    }

    /// 从已校验创建命令构造全部待写实体。
    fn prepare_create(
        &self,
        req: SaveSupplierProfileRequest,
        party_no: String,
        supplier_no: String,
        request_fingerprint: String,
        actor: &AuditActor,
        pending_assets: PendingFileAssets,
    ) -> Result<PreparedCreate> {
        let party_id = PartyId::new(next_id());
        let supplier_id = SupplierAccountId::new(next_id());
        let profile_id = SupplierCommercialProfileRevisionId::new(next_id());
        let (party, party_revision) = create_party_entities(&req, &party_id, party_no, actor.id())?;
        let (supplier, commercial_profile) = create_supplier_entities(
            &req,
            &supplier_id,
            &party_id,
            &profile_id,
            supplier_no,
            actor.id(),
        )?;
        let contact = self.create_contact(&req, &party_id, actor.id())?;
        let address = self.create_address(&req, &party_id, actor.id())?;
        let tax_profile = create_tax_profile(&req, &party_id, actor.id())?;
        let bank_account = self.create_bank_account(&req, &party_id, actor.id())?;
        let capabilities = create_capabilities(&req, &supplier_id, actor.id())?;
        let (qualifications, qualification_revisions, qualification_links) =
            create_qualifications(&req, &supplier_id, &capabilities.ids, actor.id())?;
        let rating = create_rating(&req, &supplier_id)?;
        let command = SupplierProfileCommand::new(
            next_id(),
            SupplierProfileCommandData {
                idempotency_key: req.idempotency_key,
                operation: "create".to_string(),
                request_fingerprint,
                supplier_id: supplier_id.to_string(),
                supplier_no: supplier.supplier_no.clone(),
                revision_id: profile_id.to_string(),
                revision_no: 1,
                supplier_version: supplier.base.version,
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
            supplier,
            commercial_profile,
            contact,
            address,
            tax_profile,
            bank_account,
            capabilities: capabilities.items,
            capability_revisions: capabilities.revisions,
            qualifications,
            qualification_revisions,
            qualification_links,
            rating,
            command,
            audit,
            result,
            pending_assets,
        })
    }

    /// 构造并加密默认联系人。
    fn create_contact(
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
    fn create_address(
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
    fn create_bank_account(
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

    /// 加载当前聚合并构造修订事务载荷。
    async fn prepare_update(
        &self,
        supplier_id: &str,
        req: SaveSupplierProfileRequest,
        request_fingerprint: String,
        actor: &AuditActor,
        pending_assets: PendingFileAssets,
    ) -> Result<PreparedUpdate> {
        let mut supplier = self.load_supplier_for_update(supplier_id, &req).await?;
        let mut party = self.load_party_for_update(&supplier, &req).await?;
        let party_id = supplier.party_id.clone();
        let supplier_id = SupplierAccountId::new(supplier_id);
        let party_revision_no = self.next_party_revision_no(&party_id).await?;
        let profile_revision_no = self.next_profile_revision_no(&supplier_id).await?;
        let party_revision = update_party(&mut party, &req, party_revision_no, actor.id())?;
        let commercial_profile =
            update_commercial_profile(&mut supplier, &req, profile_revision_no, actor.id())?;
        let facts = self.prepare_party_facts(&party_id, &req, actor.id()).await?;
        let capabilities = self
            .prepare_capability_changes(&supplier_id, &req, actor.id())
            .await?;
        let qualifications = self
            .prepare_qualification_changes(&supplier_id, &req, &capabilities.ids, actor.id())
            .await?;
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

    /// 加载并校验供应商乐观锁。
    async fn load_supplier_for_update(
        &self,
        supplier_id: &str,
        req: &SaveSupplierProfileRequest,
    ) -> Result<SupplierAccount> {
        let supplier = self
            .db
            .supplier_accounts()
            .find_by_id(supplier_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商不存在".to_string()))?;
        let expected = required_update_version(req.expected_supplier_version, "供应商")?;
        ensure_version(supplier.base.version, expected)?;
        if !supplier.is_active() {
            return Err(Error::BusinessLogicError(
                "供应商已停用，不能修订资料".to_string(),
            ));
        }
        Ok(supplier)
    }

    /// 加载并校验主体乐观锁。
    async fn load_party_for_update(
        &self,
        supplier: &SupplierAccount,
        req: &SaveSupplierProfileRequest,
    ) -> Result<Party> {
        let party = self
            .db
            .parties()
            .find_by_id(&supplier.party_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商关联主体不存在".to_string()))?;
        let expected = required_update_version(req.expected_party_version, "主体")?;
        ensure_version(party.base.version, expected)?;
        if !party.is_active() {
            return Err(Error::BusinessLogicError("供应商关联主体已停用".to_string()));
        }
        Ok(party)
    }

    /// 返回下一主体修订号。
    async fn next_party_revision_no(&self, party_id: &PartyId) -> Result<u32> {
        let history = self
            .db
            .party_revisions()
            .list_revision_history(party_id, &mut NoTransaction)
            .await?;
        Ok(next_revision_no(
            history.iter().map(|item| item.revision.revision_no),
        ))
    }

    /// 返回下一商务资料修订号。
    async fn next_profile_revision_no(&self, supplier_id: &SupplierAccountId) -> Result<u32> {
        let history = self
            .db
            .supplier_commercial_profile_revisions()
            .list_revision_history(supplier_id, &mut NoTransaction)
            .await?;
        Ok(next_revision_no(
            history.iter().map(|item| item.revision.revision_no),
        ))
    }

    /// 构造联系人、地址、税务与银行账户事实的追加/停用变更。
    async fn prepare_party_facts(
        &self,
        party_id: &PartyId,
        req: &SaveSupplierProfileRequest,
        actor_id: &str,
    ) -> Result<PartyFactChanges> {
        let party_filter = doc! { "party_id": party_id.to_string() };
        let mut changes = PartyFactChanges::default();
        if req.contact.is_some() || req.clear_contact {
            changes.contacts = self
                .db
                .party_contacts()
                .find_many(party_filter.clone(), &mut NoTransaction)
                .await?;
            disable_contacts(&mut changes.contacts, actor_id)?;
            changes.new_contact = self.create_contact(req, party_id, actor_id)?;
        }
        if req.address.is_some() || req.clear_address {
            changes.addresses = self
                .db
                .party_addresses()
                .find_many(party_filter.clone(), &mut NoTransaction)
                .await?;
            disable_addresses(&mut changes.addresses, actor_id)?;
            changes.new_address = self.create_address(req, party_id, actor_id)?;
        }
        if req.clear_tax_profile
            || req
                .tax_no
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty())
        {
            changes.tax_profiles = self
                .db
                .party_tax_profiles()
                .find_many(party_filter.clone(), &mut NoTransaction)
                .await?;
            disable_tax_profiles(&mut changes.tax_profiles, actor_id)?;
            changes.new_tax_profile = create_tax_profile(req, party_id, actor_id)?;
        }
        if req.bank_account.is_some() || req.clear_bank_account {
            changes.bank_accounts = self
                .db
                .party_bank_accounts()
                .find_many(party_filter, &mut NoTransaction)
                .await?;
            disable_bank_accounts(&mut changes.bank_accounts, actor_id)?;
            changes.new_bank_account = self.create_bank_account(req, party_id, actor_id)?;
        }
        Ok(changes)
    }

    /// 将能力代码集合解析为新增、启停与不可变快照。
    async fn prepare_capability_changes(
        &self,
        supplier_id: &SupplierAccountId,
        req: &SaveSupplierProfileRequest,
        actor_id: &str,
    ) -> Result<CapabilityChanges> {
        let existing = self
            .db
            .supplier_capabilities()
            .find_many(
                doc! { "supplier_id": supplier_id.to_string() },
                &mut NoTransaction,
            )
            .await?;
        let mut changes = CapabilityChanges::default();
        let requested: HashMap<String, _> = req
            .capability_codes
            .iter()
            .map(|code| (code.as_str().to_string(), *code))
            .collect();
        for mut capability in existing {
            let wanted = requested.contains_key(capability.capability_code.as_str());
            changes.ids.insert(
                capability.capability_code.as_str().to_string(),
                SupplierCapabilityId::new(&capability.base.id),
            );
            if wanted == capability.is_active() {
                continue;
            }
            let status = if wanted {
                CapabilityStatus::Active
            } else {
                CapabilityStatus::Disabled
            };
            capability.update(
                SupplierCapabilityUpdate {
                    service_region: FieldUpdate::Unchanged,
                    owner_user_id: None,
                    fulfillment_note: FieldUpdate::Unchanged,
                    valid_to: FieldUpdate::Unchanged,
                    status: Some(status),
                },
                actor_id,
            )?;
            let revision = self.capability_revision(&mut capability).await?;
            changes.updated.push(capability);
            changes.revisions.push(revision);
        }
        for code in requested.into_values() {
            if changes.ids.contains_key(code.as_str()) {
                continue;
            }
            let (capability, revision) = new_capability(supplier_id, code, req.effective_from, actor_id)?;
            changes.ids.insert(
                code.as_str().to_string(),
                SupplierCapabilityId::new(&capability.base.id),
            );
            changes.created.push(capability);
            changes.revisions.push(revision);
        }
        Ok(changes)
    }

    /// 为能力状态变更创建下一不可变快照。
    async fn capability_revision(
        &self,
        capability: &mut SupplierCapability,
    ) -> Result<SupplierCapabilityRevision> {
        let history: Vec<SupplierCapabilityRevision> = self
            .db
            .supplier_capability_revisions()
            .find_many(
                doc! {
                    "supplier_id": capability.supplier_id.to_string(),
                    "capability_code": capability.capability_code.as_str(),
                },
                &mut NoTransaction,
            )
            .await?;
        let revision_id = SupplierCapabilityRevisionId::new(next_id());
        capability.stable.current_revision_id = Some(revision_id.to_string());
        SupplierCapabilityRevision::new(
            revision_id,
            SupplierCapabilityRevisionData {
                supplier_id: capability.supplier_id.clone(),
                capability_code: capability.capability_code,
                service_region: capability.service_region.clone(),
                owner_user_id: capability.owner_user_id.clone(),
                fulfillment_note: capability.fulfillment_note.clone(),
                valid_from: capability.valid_from,
                valid_to: capability.valid_to,
                status: capability.stable.status,
                revision_no: next_revision_no(history.iter().map(|item| item.revision.revision_no)),
            },
        )
        .map_err(Into::into)
    }

    /// 将资质集合解析为新增、更新、停用、快照及能力关联替换。
    async fn prepare_qualification_changes(
        &self,
        supplier_id: &SupplierAccountId,
        req: &SaveSupplierProfileRequest,
        capability_ids: &HashMap<String, SupplierCapabilityId>,
        actor_id: &str,
    ) -> Result<QualificationChanges> {
        let existing = self
            .db
            .supplier_qualifications()
            .find_many(
                doc! { "supplier_id": supplier_id.to_string() },
                &mut NoTransaction,
            )
            .await?;
        let qualification_ids: Vec<SupplierQualificationId> = existing
            .iter()
            .map(|item| SupplierQualificationId::new(&item.base.id))
            .collect();
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
        let requested: HashMap<String, &SupplierProfileQualificationInput> = req
            .qualifications
            .iter()
            .map(|input| {
                (
                    qualification_key(input.qualification_type, &input.certificate_no),
                    input,
                )
            })
            .collect();
        let mut changes = QualificationChanges::default();
        for mut qualification in existing {
            let key = qualification_key(qualification.qualification_type, &qualification.certificate_no);
            if let Some(input) = requested.get(&key) {
                let desired_links: HashSet<String> = input
                    .capability_codes
                    .iter()
                    .map(|code| {
                        capability_ids
                            .get(code.as_str())
                            .map(ToString::to_string)
                            .ok_or_else(|| Error::ValidationError("资质适用能力不存在".to_string()))
                    })
                    .collect::<Result<_>>()?;
                let current_links = linked_capabilities
                    .get(&qualification.base.id)
                    .cloned()
                    .unwrap_or_default();
                if qualification_matches_input(&qualification, input) && current_links == desired_links {
                    continue;
                }
                apply_qualification_input(&mut qualification, input, actor_id)?;
                let revision = self.qualification_revision(&mut qualification).await?;
                let links = qualification_links(&qualification, input, capability_ids)?;
                changes
                    .replacements
                    .push((SupplierQualificationId::new(&qualification.base.id), links));
                changes.updated.push(qualification);
                changes.revisions.push(revision);
            } else if qualification.is_valid() {
                qualification.update(
                    SupplierQualificationUpdate {
                        issuer: FieldUpdate::Unchanged,
                        attachment_id: FieldUpdate::Unchanged,
                        valid_from: None,
                        valid_to: FieldUpdate::Unchanged,
                        status: Some(QualificationStatus::Disabled),
                    },
                    actor_id,
                )?;
                let revision = self.qualification_revision(&mut qualification).await?;
                changes.updated.push(qualification);
                changes.revisions.push(revision);
            }
        }
        for input in requested.into_values() {
            let exists = changes.updated.iter().any(|item| {
                qualification_key(item.qualification_type, &item.certificate_no)
                    == qualification_key(input.qualification_type, &input.certificate_no)
            });
            if !exists {
                let (qualification, revision, links) =
                    new_qualification(supplier_id, input, capability_ids, actor_id)?;
                changes
                    .replacements
                    .push((SupplierQualificationId::new(&qualification.base.id), links));
                changes.created.push(qualification);
                changes.revisions.push(revision);
            }
        }
        Ok(changes)
    }

    /// 为资质变更创建下一不可变快照。
    async fn qualification_revision(
        &self,
        qualification: &mut SupplierQualification,
    ) -> Result<SupplierQualificationRevision> {
        let history: Vec<SupplierQualificationRevision> = self
            .db
            .supplier_qualification_revisions()
            .find_many(
                doc! {
                    "supplier_id": qualification.supplier_id.to_string(),
                    "qualification_type": qualification.qualification_type.as_str(),
                    "certificate_no": &qualification.certificate_no,
                },
                &mut NoTransaction,
            )
            .await?;
        let revision_id = SupplierQualificationRevisionId::new(next_id());
        qualification.stable.current_revision_id = Some(revision_id.to_string());
        qualification_snapshot(
            revision_id,
            qualification,
            next_revision_no(history.iter().map(|item| item.revision.revision_no)),
        )
    }

    /// 构造评级开放区间关闭与下一评级版本。
    async fn prepare_rating_changes(
        &self,
        supplier_id: &SupplierAccountId,
        req: &SaveSupplierProfileRequest,
    ) -> Result<RatingChanges> {
        let Some(input) = &req.rating else {
            return Ok(RatingChanges::default());
        };
        let mut history: Vec<SupplierRatingRevision> = self
            .db
            .supplier_rating_revisions()
            .find_many(
                doc! { "supplier_id": supplier_id.to_string() },
                &mut NoTransaction,
            )
            .await?;
        history.sort_by_key(|item| item.revision.revision_no);
        let next_no = next_revision_no(history.iter().map(|item| item.revision.revision_no));
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
        Ok(RatingChanges {
            current,
            created: Some(created),
        })
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
    audit: entities::AuditLog,
    result: SupplierProfileMutationView,
    pending_assets: PendingFileAssets,
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
    audit: entities::AuditLog,
    result: SupplierProfileMutationView,
    pending_assets: PendingFileAssets,
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
        effective_from: entities::common::time::BusinessDate,
        change_reason: String,
        actor: &AuditActor,
        pending_assets: PendingFileAssets,
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
    async fn persist(mut self, db: &Database, session: &mut mongodb::ClientSession) -> Result<()> {
        self.pending_assets.persist(db, session).await?;
        self.persist_roots(db, session).await?;
        self.facts.persist(db, session).await?;
        self.capabilities.persist(db, session).await?;
        self.qualifications.persist(db, session).await?;
        self.ratings.persist(db, session).await?;
        db.supplier_profile_commands()
            .create(&self.command, session)
            .await?;
        db.audit_logs().create(&self.audit, session).await?;
        Ok(())
    }

    /// 写入 Party/Supplier 根及新修订。
    async fn persist_roots(&mut self, db: &Database, session: &mut mongodb::ClientSession) -> Result<()> {
        db.party_revisions().create(&self.party_revision, session).await?;
        db.parties().update(&mut self.party, session).await?;
        db.supplier_commercial_profile_revisions()
            .create(&self.commercial_profile, session)
            .await?;
        db.supplier_accounts().update(&mut self.supplier, session).await?;
        Ok(())
    }
}

impl PartyFactChanges {
    /// 写入从属事实的停用与新事实行。
    async fn persist(mut self, db: &Database, session: &mut mongodb::ClientSession) -> Result<()> {
        for item in &mut self.contacts {
            db.party_contacts().update(item, session).await?;
        }
        if let Some(item) = &self.new_contact {
            db.party_contacts().create(item, session).await?;
        }
        for item in &mut self.addresses {
            db.party_addresses().update(item, session).await?;
        }
        if let Some(item) = &self.new_address {
            db.party_addresses().create(item, session).await?;
        }
        for item in &mut self.tax_profiles {
            db.party_tax_profiles().update(item, session).await?;
        }
        if let Some(item) = &self.new_tax_profile {
            db.party_tax_profiles().create(item, session).await?;
        }
        for item in &mut self.bank_accounts {
            db.party_bank_accounts().update(item, session).await?;
        }
        if let Some(item) = &self.new_bank_account {
            db.party_bank_accounts().create(item, session).await?;
        }
        Ok(())
    }
}

impl CapabilityChanges {
    /// 写入能力快照及当前实体变更。
    async fn persist(mut self, db: &Database, session: &mut mongodb::ClientSession) -> Result<()> {
        for revision in &self.revisions {
            db.supplier_capability_revisions()
                .create(revision, session)
                .await?;
        }
        for capability in &self.created {
            db.supplier_capabilities().create(capability, session).await?;
        }
        for capability in &mut self.updated {
            db.supplier_capabilities().update(capability, session).await?;
        }
        Ok(())
    }
}

impl QualificationChanges {
    /// 写入资质快照、当前实体和整体替换后的能力关联。
    async fn persist(mut self, db: &Database, session: &mut mongodb::ClientSession) -> Result<()> {
        for revision in &self.revisions {
            db.supplier_qualification_revisions()
                .create(revision, session)
                .await?;
        }
        for qualification in &self.created {
            db.supplier_qualifications()
                .create(qualification, session)
                .await?;
        }
        for qualification in &mut self.updated {
            db.supplier_qualifications()
                .update(qualification, session)
                .await?;
        }
        for (qualification_id, links) in self.replacements {
            db.supplier()
                .replace_qualification_capabilities(&qualification_id, links, session)
                .await?;
        }
        Ok(())
    }
}

impl RatingChanges {
    /// 关闭上一开放区间并写入下一评级版本。
    async fn persist(mut self, db: &Database, session: &mut mongodb::ClientSession) -> Result<()> {
        if let Some(current) = self.current.as_mut() {
            db.supplier_rating_revisions().update(current, session).await?;
        }
        if let Some(created) = &self.created {
            db.supplier_rating_revisions().create(created, session).await?;
        }
        Ok(())
    }
}

/// 创建主体名称新修订并更新统一社会信用代码。
fn update_party(
    party: &mut Party,
    req: &SaveSupplierProfileRequest,
    revision_no: u32,
    actor_id: &str,
) -> Result<PartyRevision> {
    let revision_id = PartyRevisionId::new(next_id());
    party.update(
        PartyUpdate {
            unified_credit_code: option_as_authoritative_update(req.unified_credit_code.clone()),
            status: None,
        },
        actor_id,
    )?;
    party.stable.current_revision_id = Some(revision_id.to_string());
    PartyRevision::new(
        revision_id,
        PartyRevisionData {
            party_id: PartyId::new(&party.base.id),
            revision_no,
            legal_name: req.legal_name.clone(),
            short_name: req.short_name.clone(),
            change_reason: req.change_reason.clone(),
        },
    )
    .map_err(Into::into)
}

/// 创建商务资料新修订并推进供应商当前指针。
fn update_commercial_profile(
    supplier: &mut SupplierAccount,
    req: &SaveSupplierProfileRequest,
    revision_no: u32,
    actor_id: &str,
) -> Result<SupplierCommercialProfileRevision> {
    let revision_id = SupplierCommercialProfileRevisionId::new(next_id());
    let revision = SupplierCommercialProfileRevision::new(
        revision_id.clone(),
        SupplierCommercialProfileRevisionData {
            supplier_id: SupplierAccountId::new(&supplier.base.id),
            revision_no,
            settlement_mode: req.settlement_mode,
            reconciliation_cycle: req.reconciliation_cycle,
            payment_term_snapshot: req.payment_term_snapshot.clone(),
            invoice_type: req.invoice_type,
            invoice_tax_rate: req.invoice_tax_rate,
            signing_entity_party_id: req.signing_entity_party_id.clone(),
            payment_entity_party_id: req.payment_entity_party_id.clone(),
            change_reason: req.change_reason.clone(),
        },
    )?;
    supplier.update(
        SupplierAccountUpdate {
            default_payment_term_id: FieldUpdate::Unchanged,
            current_commercial_profile_revision_id: FieldUpdate::Set(revision_id),
            status: None,
        },
        actor_id,
    )?;
    Ok(revision)
}

/// 停用既有联系人事实行，供新默认事实行接替。
fn disable_contacts(items: &mut Vec<PartyContact>, actor_id: &str) -> Result<()> {
    items.retain(PartyContact::is_active);
    for item in items.iter_mut().filter(|item| item.is_active()) {
        item.update(
            PartyContactUpdate {
                status: Some(EffectiveRecordStatus::Disabled),
                valid_to: FieldUpdate::Unchanged,
                is_default: Some(false),
            },
            actor_id,
        )?;
    }
    Ok(())
}

/// 停用既有地址事实行，供新默认事实行接替。
fn disable_addresses(items: &mut Vec<PartyAddress>, actor_id: &str) -> Result<()> {
    items.retain(PartyAddress::is_active);
    for item in items.iter_mut().filter(|item| item.is_active()) {
        item.update(
            PartyAddressUpdate {
                status: Some(EffectiveRecordStatus::Disabled),
                valid_to: FieldUpdate::Unchanged,
                is_default: Some(false),
            },
            actor_id,
        )?;
    }
    Ok(())
}

/// 停用既有税务事实行，供新默认事实行接替。
fn disable_tax_profiles(items: &mut Vec<PartyTaxProfile>, actor_id: &str) -> Result<()> {
    items.retain(PartyTaxProfile::is_active);
    for item in items.iter_mut().filter(|item| item.is_active()) {
        item.update(
            PartyTaxProfileUpdate {
                status: Some(EffectiveRecordStatus::Disabled),
                valid_to: FieldUpdate::Unchanged,
                is_default: Some(false),
            },
            actor_id,
        )?;
    }
    Ok(())
}

/// 停用既有银行账户事实行，供新默认事实行接替。
fn disable_bank_accounts(items: &mut Vec<PartyBankAccount>, actor_id: &str) -> Result<()> {
    items.retain(PartyBankAccount::is_active);
    for item in items.iter_mut().filter(|item| item.is_active()) {
        item.update(
            PartyBankAccountUpdate {
                status: Some(EffectiveRecordStatus::Disabled),
                valid_to: FieldUpdate::Unchanged,
                is_default: Some(false),
            },
            actor_id,
        )?;
    }
    Ok(())
}

/// 创建一项新能力及首版快照。
fn new_capability(
    supplier_id: &SupplierAccountId,
    code: entities::supplier::CapabilityCode,
    valid_from: entities::common::time::BusinessDate,
    actor_id: &str,
) -> Result<(SupplierCapability, SupplierCapabilityRevision)> {
    let capability_id = SupplierCapabilityId::new(next_id());
    let revision_id = SupplierCapabilityRevisionId::new(next_id());
    let mut capability = SupplierCapability::new(
        capability_id,
        SupplierCapabilityData {
            supplier_id: supplier_id.clone(),
            capability_code: code,
            service_region: None,
            owner_user_id: actor_id.to_string(),
            fulfillment_note: None,
            valid_from,
            valid_to: None,
            status: CapabilityStatus::Active,
        },
        actor_id,
    )?;
    capability.stable.current_revision_id = Some(revision_id.to_string());
    let revision = SupplierCapabilityRevision::new(
        revision_id,
        SupplierCapabilityRevisionData {
            supplier_id: supplier_id.clone(),
            capability_code: code,
            service_region: None,
            owner_user_id: actor_id.to_string(),
            fulfillment_note: None,
            valid_from,
            valid_to: None,
            status: CapabilityStatus::Active,
            revision_no: 1,
        },
    )?;
    Ok((capability, revision))
}

/// 将根命令资质字段应用到同一稳定资质。
fn apply_qualification_input(
    qualification: &mut SupplierQualification,
    input: &SupplierProfileQualificationInput,
    actor_id: &str,
) -> Result<()> {
    let status = (!qualification.is_valid()).then_some(QualificationStatus::Active);
    qualification.update(
        SupplierQualificationUpdate {
            issuer: option_as_authoritative_update(input.issuer.clone()),
            attachment_id: option_as_authoritative_update(input.attachment_id.clone()),
            valid_from: Some(input.valid_from),
            valid_to: option_as_authoritative_update(input.valid_to),
            status,
        },
        actor_id,
    )?;
    Ok(())
}

/// 创建一份新资质、首版快照及适用能力关联。
fn new_qualification(
    supplier_id: &SupplierAccountId,
    input: &SupplierProfileQualificationInput,
    capability_ids: &HashMap<String, SupplierCapabilityId>,
    actor_id: &str,
) -> Result<(
    SupplierQualification,
    SupplierQualificationRevision,
    Vec<SupplierQualificationCapability>,
)> {
    let qualification_id = SupplierQualificationId::new(next_id());
    let revision_id = SupplierQualificationRevisionId::new(next_id());
    let mut qualification = SupplierQualification::new(
        qualification_id,
        SupplierQualificationData {
            supplier_id: supplier_id.clone(),
            qualification_type: input.qualification_type,
            certificate_no: input.certificate_no.clone(),
            issuer: input.issuer.clone(),
            valid_from: input.valid_from,
            valid_to: input.valid_to,
            attachment_id: input.attachment_id.clone(),
            status: QualificationStatus::Active,
        },
        actor_id,
    )?;
    qualification.stable.current_revision_id = Some(revision_id.to_string());
    let revision = qualification_snapshot(revision_id, &qualification, 1)?;
    let links = qualification_links(&qualification, input, capability_ids)?;
    Ok((qualification, revision, links))
}

/// 从当前资质构造不可变快照。
fn qualification_snapshot(
    revision_id: SupplierQualificationRevisionId,
    qualification: &SupplierQualification,
    revision_no: u32,
) -> Result<SupplierQualificationRevision> {
    SupplierQualificationRevision::new(
        revision_id,
        SupplierQualificationRevisionData {
            supplier_id: qualification.supplier_id.clone(),
            qualification_type: qualification.qualification_type,
            certificate_no: qualification.certificate_no.clone(),
            issuer: qualification.issuer.clone(),
            valid_from: qualification.valid_from,
            valid_to: qualification.valid_to,
            attachment_id: qualification.attachment_id.clone(),
            status: qualification.stable.status,
            revision_no,
        },
    )
    .map_err(Into::into)
}

/// 构造一份资质的完整能力关联集合。
fn qualification_links(
    qualification: &SupplierQualification,
    input: &SupplierProfileQualificationInput,
    capability_ids: &HashMap<String, SupplierCapabilityId>,
) -> Result<Vec<SupplierQualificationCapability>> {
    input
        .capability_codes
        .iter()
        .map(|code| {
            let capability_id = capability_ids
                .get(code.as_str())
                .ok_or_else(|| Error::ValidationError("资质适用能力不存在".to_string()))?;
            SupplierQualificationCapability::new(
                SupplierQualificationCapabilityId::new(next_id()),
                SupplierQualificationCapabilityData {
                    qualification_id: SupplierQualificationId::new(&qualification.base.id),
                    capability_id: capability_id.clone(),
                },
            )
            .map_err(Into::into)
        })
        .collect()
}

/// 形成稳定资质身份键。
fn qualification_key(
    qualification_type: entities::supplier::QualificationType,
    certificate_no: &str,
) -> String {
    format!("{}::{}", qualification_type.as_str(), certificate_no.trim())
}

/// 判断稳定资质当前字段是否已与根命令输入一致。
fn qualification_matches_input(
    qualification: &SupplierQualification,
    input: &SupplierProfileQualificationInput,
) -> bool {
    let expected_issuer = input
        .issuer
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    qualification.is_valid()
        && qualification.issuer.as_deref() == expected_issuer
        && qualification.valid_from == input.valid_from
        && qualification.valid_to == input.valid_to
        && qualification.attachment_id == input.attachment_id
}

/// 将当前集合中的可空字段映射为明确设置或清空意图。
fn option_as_authoritative_update<T>(value: Option<T>) -> FieldUpdate<T> {
    value.map_or(FieldUpdate::Clear, FieldUpdate::Set)
}

/// 返回最大修订号的下一号。
fn next_revision_no(values: impl Iterator<Item = u32>) -> u32 {
    values.max().unwrap_or(0) + 1
}

/// 计算根命令稳定指纹，保证同一幂等键只能重放完全相同的请求。
fn request_fingerprint(req: &SaveSupplierProfileRequest) -> Result<String> {
    let bytes =
        serde_json::to_vec(req).map_err(|error| Error::Internal(format!("供应商命令序列化失败: {error}")))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

/// 解析供应商根命令中的临时资质文件引用。
fn resolve_supplier_file_references(
    req: &mut SaveSupplierProfileRequest,
    pending_assets: &PendingFileAssets,
) -> Result<HashSet<String>> {
    let mut used = HashSet::new();
    for qualification in &mut req.qualifications {
        if let Some(attachment_id) = qualification.attachment_id.as_mut() {
            pending_assets.resolve_id(attachment_id, &mut used)?;
        }
    }
    Ok(used)
}

/// 校验幂等命令的操作、目标与请求指纹，并返回持久化结果。
fn replay_command(
    command: SupplierProfileCommand,
    operation: &str,
    supplier_id: Option<&str>,
    request_fingerprint: &str,
) -> Result<SupplierProfileMutationView> {
    if command.operation != operation || command.request_fingerprint != request_fingerprint {
        return Err(Error::ConflictError(
            "幂等键已用于不同的供应商资料请求".to_string(),
        ));
    }
    if supplier_id.is_some_and(|expected| command.supplier_id != expected) {
        return Err(Error::ConflictError("幂等键已用于其他供应商命令".to_string()));
    }
    Ok(command_view(command))
}

/// 读取修订场景必填版本号。
fn required_update_version(value: Option<u64>, object: &str) -> Result<u64> {
    value.ok_or_else(|| Error::ValidationError(format!("修订供应商时{object}版本不能为空")))
}

/// 校验乐观锁版本。
fn ensure_version(actual: u64, expected: u64) -> Result<()> {
    if actual != expected {
        return Err(Error::ConflictError(
            "数据已被其他请求修改，请刷新后重试".to_string(),
        ));
    }
    Ok(())
}

/// 校验敏感事实行归属于令牌限定供应商的 Party。
fn ensure_sensitive_party(actual: &PartyId, expected: &PartyId) -> Result<()> {
    if actual != expected {
        return Err(Error::ValidationError("敏感字段令牌与供应商不匹配".to_string()));
    }
    Ok(())
}

/// 校验供应商资质附件的最低敏感级别。
fn ensure_qualification_sensitivity(
    qualification: &SupplierProfileQualificationInput,
    actual: entities::file_asset::SensitivityClass,
) -> Result<()> {
    use entities::file_asset::SensitivityClass;

    let valid = if qualification.qualification_type.as_str() == "legal_person_id" {
        actual == SensitivityClass::HighlySensitive
    } else {
        matches!(
            actual,
            SensitivityClass::Sensitive | SensitivityClass::HighlySensitive
        )
    };
    if !valid {
        return Err(Error::ValidationError(
            "资质附件敏感级别不足，请按敏感资料重新上传".to_string(),
        ));
    }
    Ok(())
}

/// 创建 Party 与首版名称修订。
fn create_party_entities(
    req: &SaveSupplierProfileRequest,
    party_id: &PartyId,
    party_no: String,
    actor_id: &str,
) -> Result<(Party, PartyRevision)> {
    let revision_id = PartyRevisionId::new(next_id());
    let mut party = Party::new(
        party_id.clone(),
        PartyData {
            party_no,
            party_kind: PartyKind::Enterprise,
            unified_credit_code: req.unified_credit_code.clone(),
            status: PartyStatus::Active,
        },
        actor_id,
    )?;
    party.stable.current_revision_id = Some(revision_id.to_string());
    let revision = PartyRevision::new(
        revision_id,
        PartyRevisionData {
            party_id: party_id.clone(),
            revision_no: 1,
            legal_name: req.legal_name.clone(),
            short_name: req.short_name.clone(),
            change_reason: req.change_reason.clone(),
        },
    )?;
    Ok((party, revision))
}

/// 创建 Supplier 与首版商务资料。
fn create_supplier_entities(
    req: &SaveSupplierProfileRequest,
    supplier_id: &SupplierAccountId,
    party_id: &PartyId,
    profile_id: &SupplierCommercialProfileRevisionId,
    supplier_no: String,
    actor_id: &str,
) -> Result<(SupplierAccount, SupplierCommercialProfileRevision)> {
    let supplier = SupplierAccount::new(
        supplier_id.clone(),
        SupplierAccountData {
            party_id: party_id.clone(),
            supplier_no,
            default_payment_term_id: None,
            current_commercial_profile_revision_id: Some(profile_id.clone()),
            status: SupplierAccountStatus::Active,
        },
        actor_id,
    )?;
    let profile = SupplierCommercialProfileRevision::new(
        profile_id.clone(),
        SupplierCommercialProfileRevisionData {
            supplier_id: supplier_id.clone(),
            revision_no: 1,
            settlement_mode: req.settlement_mode,
            reconciliation_cycle: req.reconciliation_cycle,
            payment_term_snapshot: req.payment_term_snapshot.clone(),
            invoice_type: req.invoice_type,
            invoice_tax_rate: req.invoice_tax_rate,
            signing_entity_party_id: req.signing_entity_party_id.clone(),
            payment_entity_party_id: req.payment_entity_party_id.clone(),
            change_reason: req.change_reason.clone(),
        },
    )?;
    Ok((supplier, profile))
}

/// 创建可选税务事实。
fn create_tax_profile(
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

/// 创建场景中的能力实体、首版快照与代码索引。
struct CreatedCapabilities {
    items: Vec<SupplierCapability>,
    revisions: Vec<SupplierCapabilityRevision>,
    ids: HashMap<String, SupplierCapabilityId>,
}

/// 创建能力及首版快照，并返回代码到能力 ID 的映射。
fn create_capabilities(
    req: &SaveSupplierProfileRequest,
    supplier_id: &SupplierAccountId,
    actor_id: &str,
) -> Result<CreatedCapabilities> {
    let mut capabilities = Vec::with_capacity(req.capability_codes.len());
    let mut revisions = Vec::with_capacity(req.capability_codes.len());
    let mut ids = HashMap::new();
    for code in &req.capability_codes {
        let capability_id = SupplierCapabilityId::new(next_id());
        let revision_id = SupplierCapabilityRevisionId::new(next_id());
        let mut capability = SupplierCapability::new(
            capability_id.clone(),
            SupplierCapabilityData {
                supplier_id: supplier_id.clone(),
                capability_code: *code,
                service_region: None,
                owner_user_id: actor_id.to_string(),
                fulfillment_note: None,
                valid_from: req.effective_from,
                valid_to: None,
                status: CapabilityStatus::Active,
            },
            actor_id,
        )?;
        capability.stable.current_revision_id = Some(revision_id.to_string());
        let revision = SupplierCapabilityRevision::new(
            revision_id,
            SupplierCapabilityRevisionData {
                supplier_id: supplier_id.clone(),
                capability_code: *code,
                service_region: None,
                owner_user_id: actor_id.to_string(),
                fulfillment_note: None,
                valid_from: req.effective_from,
                valid_to: None,
                status: CapabilityStatus::Active,
                revision_no: 1,
            },
        )?;
        ids.insert(code.as_str().to_string(), capability_id);
        capabilities.push(capability);
        revisions.push(revision);
    }
    Ok(CreatedCapabilities {
        items: capabilities,
        revisions,
        ids,
    })
}

/// 创建资质、首版快照与适用能力关联。
fn create_qualifications(
    req: &SaveSupplierProfileRequest,
    supplier_id: &SupplierAccountId,
    capability_ids: &HashMap<String, SupplierCapabilityId>,
    actor_id: &str,
) -> Result<(
    Vec<SupplierQualification>,
    Vec<SupplierQualificationRevision>,
    Vec<SupplierQualificationCapability>,
)> {
    let mut qualifications = Vec::with_capacity(req.qualifications.len());
    let mut revisions = Vec::with_capacity(req.qualifications.len());
    let mut links = Vec::new();
    for input in &req.qualifications {
        let qualification_id = SupplierQualificationId::new(next_id());
        let revision_id = SupplierQualificationRevisionId::new(next_id());
        let mut qualification = SupplierQualification::new(
            qualification_id.clone(),
            SupplierQualificationData {
                supplier_id: supplier_id.clone(),
                qualification_type: input.qualification_type,
                certificate_no: input.certificate_no.clone(),
                issuer: input.issuer.clone(),
                valid_from: input.valid_from,
                valid_to: input.valid_to,
                attachment_id: input.attachment_id.clone(),
                status: QualificationStatus::Active,
            },
            actor_id,
        )?;
        qualification.stable.current_revision_id = Some(revision_id.to_string());
        revisions.push(SupplierQualificationRevision::new(
            revision_id,
            SupplierQualificationRevisionData {
                supplier_id: supplier_id.clone(),
                qualification_type: input.qualification_type,
                certificate_no: qualification.certificate_no.clone(),
                issuer: qualification.issuer.clone(),
                valid_from: qualification.valid_from,
                valid_to: qualification.valid_to,
                attachment_id: qualification.attachment_id.clone(),
                status: QualificationStatus::Active,
                revision_no: 1,
            },
        )?);
        for code in &input.capability_codes {
            let capability_id = capability_ids
                .get(code.as_str())
                .ok_or_else(|| Error::ValidationError("资质适用能力不存在".to_string()))?;
            links.push(SupplierQualificationCapability::new(
                SupplierQualificationCapabilityId::new(next_id()),
                SupplierQualificationCapabilityData {
                    qualification_id: qualification_id.clone(),
                    capability_id: capability_id.clone(),
                },
            )?);
        }
        qualifications.push(qualification);
    }
    Ok((qualifications, revisions, links))
}

/// 创建首版供应商评级。
fn create_rating(
    req: &SaveSupplierProfileRequest,
    supplier_id: &SupplierAccountId,
) -> Result<Option<SupplierRatingRevision>> {
    let Some(input) = &req.rating else {
        return Ok(None);
    };
    Ok(Some(SupplierRatingRevision::new(
        SupplierRatingRevisionId::new(next_id()),
        SupplierRatingRevisionData {
            supplier_id: supplier_id.clone(),
            revision_no: 1,
            initial_score: input.initial_score,
            rating: input.rating,
            current_score: input.current_score,
            valid_from: input.valid_from,
            valid_to: None,
            change_reason: req.change_reason.clone(),
        },
    )?))
}

/// 创建场景必填的稳定业务编号。
fn required_create_identity(value: Option<&str>, field: &str) -> Result<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| Error::ValidationError(format!("创建供应商时{field}不能为空")))
}

/// 将命令实体转换为稳定 HTTP 结果。
fn command_view(command: SupplierProfileCommand) -> SupplierProfileMutationView {
    SupplierProfileMutationView {
        supplier_id: command.supplier_id,
        supplier_no: command.supplier_no,
        revision_id: command.revision_id,
        revision_no: command.revision_no,
        supplier_version: command.supplier_version,
        effective_from: command.effective_from.to_string(),
        recorded_at: command.base.created_at,
        change_reason: command.change_reason,
    }
}

#[cfg(test)]
mod tests {
    use entities::{
        common::time::BusinessDate,
        file_asset::SensitivityClass,
        supplier::{QualificationType, SupplierProfileCommand, SupplierProfileCommandData},
    };

    use super::{
        ensure_qualification_sensitivity, ensure_version, replay_command, SupplierProfileQualificationInput,
    };

    fn qualification(qualification_type: QualificationType) -> SupplierProfileQualificationInput {
        SupplierProfileQualificationInput {
            qualification_type,
            certificate_no: "CERT-1".to_string(),
            issuer: None,
            valid_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
            valid_to: None,
            attachment_id: None,
            capability_codes: Vec::new(),
        }
    }

    #[test]
    fn legal_person_attachment_requires_highest_sensitivity() {
        let input = qualification(QualificationType::LegalPersonId);
        assert!(ensure_qualification_sensitivity(&input, SensitivityClass::Sensitive).is_err());
        assert!(ensure_qualification_sensitivity(&input, SensitivityClass::HighlySensitive).is_ok());
    }

    #[test]
    fn command_replay_is_bound_to_supplier() {
        let command = SupplierProfileCommand::new(
            "command-1",
            SupplierProfileCommandData {
                idempotency_key: "key-1".to_string(),
                operation: "update".to_string(),
                request_fingerprint: "fingerprint-1".to_string(),
                supplier_id: "supplier-1".to_string(),
                supplier_no: "SUP-1".to_string(),
                revision_id: "revision-1".to_string(),
                revision_no: 2,
                supplier_version: 3,
                effective_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
                change_reason: "修订".to_string(),
            },
        )
        .unwrap();
        let replayed =
            replay_command(command.clone(), "update", Some("supplier-1"), "fingerprint-1").unwrap();
        assert_eq!(replayed.effective_from, "2026-01-01");
        assert_eq!(replayed.recorded_at, command.base.created_at);
        assert_eq!(replayed.change_reason, "修订");
        assert!(replay_command(command.clone(), "update", Some("supplier-2"), "fingerprint-1").is_err());
        assert!(replay_command(command, "update", Some("supplier-1"), "different").is_err());
        assert!(ensure_version(3, 3).is_ok());
        assert!(ensure_version(3, 2).is_err());
    }
}
