//! 供应商资料根级命令。
//!
//! 页面只调用本服务维护 Party、Supplier、当前事实与独立资质；服务在一个
//! MongoDB 事务中提交全部写入，并把幂等结果与业务数据一并落库。

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use database::{AccessControlExt, NoTransaction, PartyExt, SupplierExt, Transactional};
use entities::{
    common::time::Instant,
    field_update::FieldUpdate,
    file_asset::SensitivityClass,
    ids::{
        PartyAddressId, PartyBankAccountId, PartyContactId, PartyId, PartyRevisionId, PartyTaxProfileId,
        SupplierAccountId, SupplierCapabilityId, SupplierCapabilityRevisionId,
        SupplierCommercialProfileRevisionId, SupplierQualificationCapabilityId, SupplierQualificationId,
        SupplierQualificationRevisionId, SupplierRatingRevisionId,
    },
    party::{
        AddressType, EffectiveRecordStatus, Party, PartyAddress, PartyAddressData, PartyBankAccount,
        PartyBankAccountData, PartyContact, PartyContactData, PartyData, PartyKind, PartyRevision,
        PartyRevisionData, PartyStatus, PartyTaxProfile, PartyTaxProfileData,
    },
    supplier::{
        next_supplier_revision_no, profile_change, qualification_identity_key, validate_profile_selection,
        QualificationAttachmentSensitivity, QualificationStatus, SupplierAccount, SupplierAccountData,
        SupplierAccountStatus, SupplierCapability, SupplierCapabilityRevision, SupplierCapabilityUpdate,
        SupplierCommercialProfileRevision, SupplierCommercialProfileRevisionData, SupplierProfileCommand,
        SupplierProfileCommandData, SupplierProfileUpdateViolation, SupplierQualification,
        SupplierQualificationCapability, SupplierQualificationRevision, SupplierQualificationSelection,
        SupplierQualificationUpdate, SupplierRatingRevision, SupplierRatingRevisionData,
    },
};
use id_generator::next_id;
use mongodb::Database;
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
        let pending_assets = PendingFileAssets::prepare(asset_requests, actor)?;
        let used = resolve_supplier_file_references(&mut req, &pending_assets)?;
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
    ///
    /// # 参数
    /// * `idempotency_key` - 客户端根资料命令幂等键
    ///
    /// # 返回
    /// 返回已成功命令；不存在时返回 `None`。
    ///
    /// # 错误
    /// 仓储查询或反序列化失败时返回错误。
    async fn command_record(&self, idempotency_key: &str) -> Result<Option<SupplierProfileCommand>> {
        Ok(self
            .db
            .supplier()
            .profile_command(idempotency_key, &mut NoTransaction)
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
                    Some(command) => {
                        command
                            .ensure_replayable(operation, supplier_id, request_fingerprint)
                            .map_err(|e| Error::ConflictError(e.to_string()))?;
                        Ok(SupplierProfileWithAssetsResult {
                            view: command_view(command),
                            assets_committed: assets_may_be_committed,
                        })
                    }
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
            .supplier()
            .account(&SupplierAccountId::new(&scope.supplier_id), &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商不存在".to_string()))?;
        let ciphertext = match scope.kind {
            SensitiveFieldKind::ContactMobile => {
                let record = self
                    .db
                    .party()
                    .contact(&scope.record_id, &mut NoTransaction)
                    .await?
                    .ok_or_else(|| Error::NotFound("联系人不存在".to_string()))?;
                ensure_sensitive_party(&record.party_id, &supplier.party_id)?;
                record.mobile_ciphertext
            }
            SensitiveFieldKind::Address => {
                let record = self
                    .db
                    .party()
                    .address(&scope.record_id, &mut NoTransaction)
                    .await?
                    .ok_or_else(|| Error::NotFound("地址不存在".to_string()))?;
                ensure_sensitive_party(&record.party_id, &supplier.party_id)?;
                record.address_ciphertext
            }
            SensitiveFieldKind::BankAccountNumber => {
                let record = self
                    .db
                    .party()
                    .bank_account(&scope.record_id, &mut NoTransaction)
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

    /// 校验签约或付款主体存在且启用。
    ///
    /// # 参数
    /// * `party_id` - 待引用的企业主体 ID
    ///
    /// # 返回
    /// 主体存在且启用时返回 `Ok(())`。
    ///
    /// # 错误
    /// 主体不存在、已停用或仓储查询失败时返回错误。
    async fn ensure_party_active(&self, party_id: &PartyId) -> Result<()> {
        let party = self
            .db
            .party()
            .party(party_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("签约或付款主体不存在".to_string()))?;
        if !party.is_active() {
            return Err(Error::BusinessLogicError("签约或付款主体已停用".to_string()));
        }
        Ok(())
    }

    /// 校验资质附件存在且敏感级别符合附件用途。
    ///
    /// # 参数
    /// * `qualifications` - 根资料命令提交的资质集合
    /// * `pending_assets` - 同命令待登记的文件资产
    ///
    /// # 返回
    /// 全部附件存在且满足资质类型最低敏感级别时返回 `Ok(())`。
    ///
    /// # 错误
    /// 附件不存在、敏感级别不足或仓储查询失败时返回错误。
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
                        .supplier()
                        .qualification_attachment(attachment_id, &mut NoTransaction)
                        .await?
                        .ok_or_else(|| Error::NotFound("资质附件不存在，请先上传文件".to_string()))?
                        .sensitivity_class
                }
            };
            let sensitivity = match sensitivity {
                SensitivityClass::General => QualificationAttachmentSensitivity::General,
                SensitivityClass::Sensitive => QualificationAttachmentSensitivity::Sensitive,
                SensitivityClass::HighlySensitive => QualificationAttachmentSensitivity::HighlySensitive,
            };
            if !qualification
                .qualification_type
                .accepts_attachment_sensitivity(sensitivity)
            {
                return Err(Error::ValidationError(
                    "资质附件敏感级别不足，请按敏感资料重新上传".to_string(),
                ));
            }
        }
        Ok(())
    }

    /// 校验根资料能力与资质选择关系。
    ///
    /// # 参数
    /// * `req` - 已通过 DTO 格式校验的根资料命令
    ///
    /// # 返回
    /// 能力、资质身份唯一且资质仅引用已勾选能力时返回 `Ok(())`。
    ///
    /// # 错误
    /// 任一纯领域选择规则不满足时返回校验错误。
    fn ensure_unique_inputs(&self, req: &SaveSupplierProfileRequest) -> Result<()> {
        let qualifications: Vec<SupplierQualificationSelection<'_>> = req
            .qualifications
            .iter()
            .map(|qualification| SupplierQualificationSelection {
                qualification_type: qualification.qualification_type,
                certificate_no: &qualification.certificate_no,
                capability_codes: &qualification.capability_codes,
            })
            .collect();
        validate_profile_selection(&req.capability_codes, &qualifications)
            .map_err(|error| Error::ValidationError(error.to_string()))
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
        let party_revision_id = PartyRevisionId::new(next_id());
        let commercial_profile_id = SupplierCommercialProfileRevisionId::new(next_id());
        let party_revision = profile_change::plan_party_revision(
            &mut party,
            req.unified_credit_code.clone(),
            req.legal_name.clone(),
            req.short_name.clone(),
            req.change_reason.clone(),
            party_revision_id,
            party_revision_no,
            actor.id(),
        )
        .map_err(|e| Error::BusinessLogicError(e.to_string()))?;
        let commercial_profile = profile_change::plan_commercial_profile_revision(
            &mut supplier,
            req.settlement_mode,
            req.reconciliation_cycle,
            req.payment_term_snapshot.clone(),
            req.business_category.clone(),
            req.invoice_type,
            req.invoice_tax_rate,
            req.signing_entity_party_id.clone(),
            req.payment_entity_party_id.clone(),
            req.change_reason.clone(),
            commercial_profile_id,
            profile_revision_no,
            actor.id(),
        )
        .map_err(|e| Error::BusinessLogicError(e.to_string()))?;
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
            None => Ok(supplier),
            Some(SupplierProfileUpdateViolation::VersionMismatch) => Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            )),
            Some(SupplierProfileUpdateViolation::SupplierDisabled) => Err(Error::BusinessLogicError(
                "供应商已停用，不能修订资料".to_string(),
            )),
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
        Ok(self
            .db
            .party_revisions()
            .next_revision_no(party_id, &mut NoTransaction)
            .await?)
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
        Ok(self
            .db
            .supplier()
            .next_commercial_profile_revision_no(supplier_id, &mut NoTransaction)
            .await?)
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
            changes.contacts = self
                .db
                .party_contacts()
                .list_by_party(party_id, &mut NoTransaction)
                .await?;
            profile_change::disable_contacts(&mut changes.contacts, actor_id)
                .map_err(|e| Error::BusinessLogicError(e.to_string()))?;
            changes.new_contact = self.create_contact(req, party_id, actor_id)?;
        }
        if req.address.is_some() || req.clear_address {
            changes.addresses = self
                .db
                .party_addresses()
                .list_by_party(party_id, &mut NoTransaction)
                .await?;
            profile_change::disable_addresses(&mut changes.addresses, actor_id)
                .map_err(|e| Error::BusinessLogicError(e.to_string()))?;
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
                .list_by_party(party_id, &mut NoTransaction)
                .await?;
            profile_change::disable_tax_profiles(&mut changes.tax_profiles, actor_id)
                .map_err(|e| Error::BusinessLogicError(e.to_string()))?;
            changes.new_tax_profile = create_tax_profile(req, party_id, actor_id)?;
        }
        if req.bank_account.is_some() || req.clear_bank_account {
            changes.bank_accounts = self
                .db
                .party_bank_accounts()
                .list_by_party(party_id, &mut NoTransaction)
                .await?;
            profile_change::disable_bank_accounts(&mut changes.bank_accounts, actor_id)
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
        let existing = self
            .db
            .supplier()
            .list_capabilities(supplier_id, &mut NoTransaction)
            .await?;
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
        let mut existing_by_code: HashMap<String, SupplierCapability> = existing
            .into_iter()
            .map(|cap| (cap.capability_code.as_str().to_string(), cap))
            .collect();
        for cap in existing_by_code.values() {
            changes.ids.insert(
                cap.capability_code.as_str().to_string(),
                SupplierCapabilityId::new(&cap.base.id),
            );
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
            let (capability, revision) = profile_change::new_capability(
                supplier_id,
                code,
                req.effective_from,
                actor_id,
                capability_id.clone(),
                revision_id,
            )
            .map_err(|e| Error::BusinessLogicError(e.to_string()))?;
            changes.ids.insert(
                code.as_str().to_string(),
                SupplierCapabilityId::new(&capability.base.id),
            );
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
        let existing = self
            .db
            .supplier()
            .list_qualifications(supplier_id, &mut NoTransaction)
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
            .map(|input| {
                (
                    qualification_identity_key(input.qualification_type, &input.certificate_no),
                    input,
                )
            })
            .collect();
        let mut existing_by_key: HashMap<String, SupplierQualification> = existing
            .into_iter()
            .map(|qual| (qual.identity_key(), qual))
            .collect();
        let mut changes = QualificationChanges::default();
        for key in plan.qualification_updates {
            let mut qualification = existing_by_key
                .remove(&key)
                .ok_or_else(|| Error::Internal("资质计划与已加载事实不一致".to_string()))?;
            let input = requested_map
                .get(&key)
                .ok_or_else(|| Error::Internal("资质请求与计划不一致".to_string()))?;
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
            changes
                .replacements
                .push((SupplierQualificationId::new(&qualification.base.id), links));
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
                        valid_from: None,
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
            let (qualification, revision, links) = profile_change::new_qualification(
                supplier_id,
                input.qualification_type,
                input.certificate_no.clone(),
                input.issuer.clone(),
                input.valid_from,
                input.valid_to,
                input.attachment_id.clone(),
                &input.capability_codes,
                capability_ids,
                actor_id,
                qualification_id.clone(),
                revision_id,
                link_ids,
            )
            .map_err(|e| Error::BusinessLogicError(e.to_string()))?;
            changes
                .replacements
                .push((SupplierQualificationId::new(&qualification.base.id), links));
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
        let history = self
            .db
            .supplier()
            .list_rating_history(supplier_id, &mut NoTransaction)
            .await?;
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

/// 校验敏感事实行归属于令牌限定供应商的 Party。
fn ensure_sensitive_party(actual: &PartyId, expected: &PartyId) -> Result<()> {
    if actual != expected {
        return Err(Error::ValidationError("敏感字段令牌与供应商不匹配".to_string()));
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
            business_category: req.business_category.clone(),
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
        let (capability, revision) = profile_change::new_capability(
            supplier_id,
            *code,
            req.effective_from,
            actor_id,
            capability_id.clone(),
            revision_id,
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

/// 创建资质、首版快照与适用能力关联；委托领域工厂保证修订快照与实体一致。
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
        let link_ids = input
            .capability_codes
            .iter()
            .map(|_| SupplierQualificationCapabilityId::new(next_id()))
            .collect();
        let (qualification, revision, new_links) = profile_change::new_qualification(
            supplier_id,
            input.qualification_type,
            input.certificate_no.clone(),
            input.issuer.clone(),
            input.valid_from,
            input.valid_to,
            input.attachment_id.clone(),
            &input.capability_codes,
            capability_ids,
            actor_id,
            qualification_id,
            revision_id,
            link_ids,
        )?;
        qualifications.push(qualification);
        revisions.push(revision);
        links.extend(new_links);
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
        supplier::{SupplierProfileCommand, SupplierProfileCommandData},
    };

    use super::{command_view, SaveSupplierProfileRequest};

    #[test]
    fn command_replay_is_bound_to_supplier_and_fingerprint_stable() {
        const FP1: &str = "0000000000000000000000000000000000000000000000000000000000000000";
        const FP1_V1: &str = "sha256-v1:0000000000000000000000000000000000000000000000000000000000000000";
        const FP2: &str = "1111111111111111111111111111111111111111111111111111111111111111";
        let command = SupplierProfileCommand::new(
            "command-1",
            SupplierProfileCommandData {
                idempotency_key: "key-1".to_string(),
                operation: "update".to_string(),
                request_fingerprint: FP1.to_string(),
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
        assert!(command
            .ensure_replayable("update", Some("supplier-1"), FP1)
            .is_ok());
        assert!(command
            .ensure_replayable("update", Some("supplier-1"), FP1_V1)
            .is_ok());
        let replayed = command_view(command.clone());
        assert_eq!(replayed.effective_from, "2026-01-01");
        assert_eq!(replayed.recorded_at, command.base.created_at);
        assert_eq!(replayed.change_reason, "修订");
        assert!(command
            .ensure_replayable("update", Some("supplier-2"), FP1)
            .is_err());
        assert!(command
            .ensure_replayable("update", Some("supplier-1"), FP2)
            .is_err());
        assert!(command
            .ensure_replayable("create", Some("supplier-1"), FP1)
            .is_err());
        assert!(SupplierProfileCommand::ensure_version(3, 3).is_ok());
        assert!(SupplierProfileCommand::ensure_version(3, 2).is_err());
        assert!(matches!(
            SupplierProfileCommand::required_update_version(None, "主体"),
            Err(e) if e.to_string().contains("版本不能为空")
        ));
        assert_eq!(
            SupplierProfileCommand::required_create_identity(Some(" SUP-001 "), "供应商编号").unwrap(),
            "SUP-001"
        );
        let req1 = SaveSupplierProfileRequest {
            idempotency_key: "key-1".to_string(),
            party_no: Some("PARTY-1".to_string()),
            supplier_no: Some("SUP-1".to_string()),
            expected_party_version: None,
            expected_supplier_version: None,
            legal_name: "示例".to_string(),
            short_name: None,
            unified_credit_code: None,
            contact: None,
            clear_contact: false,
            address: None,
            clear_address: false,
            tax_no: None,
            clear_tax_profile: false,
            bank_account: None,
            clear_bank_account: false,
            settlement_mode: entities::supplier::SettlementMode::Prepayment,
            reconciliation_cycle: entities::supplier::ReconciliationCycle::Monthly,
            payment_term_snapshot: "PREPAY_30".to_string(),
            business_category: None,
            invoice_type: entities::supplier::InvoiceType::VatSpecial,
            invoice_tax_rate: entities::money::Rate::from_str("0.13").unwrap(),
            signing_entity_party_id: entities::ids::PartyId::new("party-1"),
            payment_entity_party_id: entities::ids::PartyId::new("party-2"),
            capability_codes: vec![],
            qualifications: vec![],
            rating: None,
            effective_from: BusinessDate::from_ymd(2026, 8, 31).unwrap(),
            change_reason: "首次".to_string(),
        };
        let mut req2 = req1.clone();
        let fp1 = req1.fingerprint().unwrap();
        let fp2 = req2.fingerprint().unwrap();
        assert_eq!(fp1, fp2);
        assert!(fp1.starts_with("sha256-v1:"));
        assert_eq!(fp1.len(), "sha256-v1:".len() + 64);
        req2.legal_name = "不同".to_string();
        let fp3 = req2.fingerprint().unwrap();
        assert_ne!(fp1, fp3);
        assert!(SaveSupplierProfileRequest::required_create_identity(None, "主体编号").is_err());
        assert!(SaveSupplierProfileRequest::ensure_version(1, 2).is_err());
        let digest = fp1.strip_prefix("sha256-v1:").unwrap();
        assert!(digest.chars().all(|c| c.is_ascii_hexdigit()));
        // 兼容旧裸 hex：存储为裸 hex 的命令仍可与新版指纹回放
        let bare = fp1.strip_prefix("sha256-v1:").unwrap().to_string();
        let cmd_bare = SupplierProfileCommand::new(
            "cmd-bare-fp",
            SupplierProfileCommandData {
                idempotency_key: "key-1".to_string(),
                operation: "update".to_string(),
                request_fingerprint: bare.clone(),
                supplier_id: "supplier-1".to_string(),
                supplier_no: "SUP-1".to_string(),
                revision_id: "revision-1".to_string(),
                revision_no: 2,
                supplier_version: 2,
                effective_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
                change_reason: "修订".to_string(),
            },
        )
        .unwrap();
        assert!(cmd_bare
            .ensure_replayable("update", Some("supplier-1"), &fp1)
            .is_ok());
        assert_eq!(cmd_bare.request_fingerprint, bare);
    }

    use std::str::FromStr;
}
