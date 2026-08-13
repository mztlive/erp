//! 客户资料根级命令与对象中心查询。
//!
//! 页面只通过本服务维护 Party 身份、客户角色、归属首行及当前从属事实；
//! 创建和修订把全部写入、审计与幂等结果放在同一 MongoDB 事务中。

use std::{collections::HashMap, sync::Arc};

use database::{AccessControlExt, CustomerExt, NoTransaction, PartyExt, Transactional};
use entities::{
    common::time::{BusinessDate, Instant},
    customer::{
        AssignmentRole, CustomerAccount, CustomerAccountData, CustomerAccountId, CustomerAccountStatus,
        CustomerAccountUpdate, CustomerAssignment, CustomerAssignmentData, CustomerAssignmentId,
        CustomerProfileCommand, CustomerProfileCommandData,
    },
    field_update::FieldUpdate,
    ids::{PartyAddressId, PartyBankAccountId, PartyContactId, PartyId, PartyRevisionId},
    party::{
        EffectiveRecordStatus, Party, PartyAddress, PartyAddressData, PartyAddressUpdate, PartyBankAccount,
        PartyBankAccountData, PartyBankAccountUpdate, PartyContact, PartyContactData, PartyContactUpdate,
        PartyData, PartyKind, PartyRevision, PartyRevisionData, PartyStatus, PartyUpdate,
    },
};
use id_generator::next_id;
use mongodb::{bson::doc, Database};
use sha2::{Digest, Sha256};
use validator::Validate;

use crate::{
    audit::AuditActor,
    errors::{Error, Result},
    party::{SensitiveDataCodec, SensitiveFieldKind},
};

use super::{
    CustomerActionBlockerView, CustomerAssignmentView, CustomerProfileAddressInput,
    CustomerProfileBankAccountInput, CustomerProfileContactInput, CustomerProfileDetailView,
    CustomerProfileMutationView, CustomerSensitiveFieldView, CustomerSensitiveRevealView, CustomerView,
    RevealCustomerSensitiveRequest, SaveCustomerProfileRequest,
};

/// 事务失败后核对幂等命令所需的请求上下文。
#[derive(Clone, Copy)]
struct TransactionResolutionContext<'a> {
    idempotency_key: &'a str,
    operation: &'a str,
    customer_id: Option<&'a str>,
    initiated_by: &'a str,
    fingerprint: &'a str,
}

/// 完整客户资料的根级服务。
pub struct CustomerProfileService {
    db: Database,
    sensitive_data: Arc<SensitiveDataCodec>,
}

impl CustomerProfileService {
    /// 创建客户资料根级服务。
    ///
    /// # 参数
    /// * `db` - MongoDB 数据库实例
    /// * `sensitive_data` - 启动期固定的敏感数据编解码器
    ///
    /// # 返回
    /// 返回可执行客户资料用例的服务实例。
    pub fn new(db: Database, sensitive_data: Arc<SensitiveDataCodec>) -> Self {
        Self { db, sensitive_data }
    }

    /// 原子创建 Party、客户角色、首条 OWNER 与首批资料事实。
    ///
    /// 首条 OWNER 固定为当前创建人；请求体中的 `owner_user_id` 即使提交也会被忽略。
    ///
    /// # 参数
    /// * `req` - 创建客户资料命令
    /// * `actor` - 已通过鉴权的审计操作人；其账号 ID 写入首条 OWNER 归属
    ///
    /// # 返回
    /// 返回创建命令的稳定结果视图。
    ///
    /// # 错误
    /// 输入非法、创建人账号不存在或已停用、身份重复、敏感值加密失败或事务失败时返回错误。
    pub async fn create(
        &self,
        req: SaveCustomerProfileRequest,
        actor: &AuditActor,
    ) -> Result<CustomerProfileMutationView> {
        self.validate_request(&req, true)?;
        let fingerprint = request_fingerprint(&req)?;
        if let Some(command) = self.command_record(&req.idempotency_key).await? {
            return replay_command(command, "create", None, actor.id(), &fingerprint);
        }
        let owner_user_id = actor.id().to_string();
        self.ensure_user_exists(&owner_user_id).await?;
        let prepared = self.prepare_create(req, owner_user_id, fingerprint.clone(), actor)?;
        let intended = prepared.result.clone();
        let idempotency_key = prepared.command.idempotency_key.clone();
        let db = self.db.clone();
        let client = db.client().clone();
        let transaction = client
            .with_transaction(move |session| Box::pin(async move { prepared.persist(&db, session).await }))
            .await;
        self.resolve_transaction(
            transaction,
            intended,
            TransactionResolutionContext {
                idempotency_key: &idempotency_key,
                operation: "create",
                customer_id: None,
                initiated_by: actor.id(),
                fingerprint: &fingerprint,
            },
        )
        .await
    }

    /// 原子修订 Party 身份、客户角色与显式提交的资料事实集合。
    ///
    /// # Errors
    /// 输入非法、乐观锁冲突、既有事实不属于当前客户、幂等键冲突或事务失败时返回错误。
    pub async fn update(
        &self,
        customer_id: &str,
        req: SaveCustomerProfileRequest,
        actor: &AuditActor,
    ) -> Result<CustomerProfileMutationView> {
        self.validate_request(&req, false)?;
        let fingerprint = request_fingerprint(&req)?;
        if let Some(command) = self.command_record(&req.idempotency_key).await? {
            return replay_command(command, "update", Some(customer_id), actor.id(), &fingerprint);
        }
        let prepared = self
            .prepare_update(customer_id, req, fingerprint.clone(), actor)
            .await?;
        let intended = prepared.result.clone();
        let idempotency_key = prepared.command.idempotency_key.clone();
        let db = self.db.clone();
        let client = db.client().clone();
        let transaction = client
            .with_transaction(move |session| Box::pin(async move { prepared.persist(&db, session).await }))
            .await;
        self.resolve_transaction(
            transaction,
            intended,
            TransactionResolutionContext {
                idempotency_key: &idempotency_key,
                operation: "update",
                customer_id: Some(customer_id),
                initiated_by: actor.id(),
                fingerprint: &fingerprint,
            },
        )
        .await
    }

    /// 查询客户资料对象中心的当前事实、历史版本与敏感字段揭示入口。
    ///
    /// # Errors
    /// 客户、Party 或当前名称修订不存在，或任一仓储查询失败时返回错误。
    pub async fn detail(&self, customer_id: &str) -> Result<CustomerProfileDetailView> {
        let account = self.load_customer(customer_id).await?;
        let party = self.load_party(&account.party_id).await?;
        let revisions = self
            .db
            .party_revisions()
            .list_revision_history(&account.party_id, &mut NoTransaction)
            .await?;
        let current_revision = current_revision(&party, &revisions)?;
        let assignments = self.assignments(customer_id).await?;
        let account_names = self.assignment_account_names(&assignments).await?;
        let (contacts, addresses, tax_profiles, bank_accounts) =
            self.current_party_facts(&account.party_id).await?;
        let sensitive_fields = self.sensitive_fields(customer_id, &contacts, &addresses, &bank_accounts)?;
        let mut detail = build_detail(ProfileDetailParts {
            account,
            party,
            current_revision,
            revisions,
            assignments,
            contacts,
            addresses,
            tax_profiles,
            bank_accounts,
            sensitive_fields,
        });
        for assignment in &mut detail.assignments {
            assignment.user_name = account_names
                .get(&assignment.user_id)
                .cloned()
                .unwrap_or_else(|| assignment.user_id.clone());
        }
        Ok(detail)
    }

    /// 按幂等键查询已成功客户资料命令的稳定结果。
    ///
    /// # Errors
    /// 查询失败时返回仓储错误。
    pub async fn command_result(&self, idempotency_key: &str) -> Result<Option<CustomerProfileMutationView>> {
        Ok(self.command_record(idempotency_key).await?.map(command_view))
    }

    /// 验证短时令牌和客户归属后解密单个敏感字段，并记录成功审计。
    ///
    /// HTTP 层必须先依据令牌字段类型执行对应 RBAC 校验。
    ///
    /// # Errors
    /// 令牌非法/过期、事实不属于令牌客户、密文不可用或审计失败时返回错误。
    pub async fn reveal_sensitive(
        &self,
        req: RevealCustomerSensitiveRequest,
        actor: &AuditActor,
    ) -> Result<CustomerSensitiveRevealView> {
        req.validate()?;
        let now = unix_now()?;
        let scope = self.sensitive_data.verify_reveal_token(&req.reveal_token, now)?;
        let account = self.load_customer(&scope.supplier_id).await?;
        let ciphertext = self
            .sensitive_ciphertext(scope.kind, &scope.record_id, &account.party_id)
            .await?;
        let value = self.sensitive_data.decrypt(&ciphertext)?;
        let audit =
            actor
                .clone()
                .resource_log("customer_sensitive.reveal", "customer_sensitive", scope.record_id)?;
        self.db.audit_logs().create(&audit, &mut NoTransaction).await?;
        Ok(CustomerSensitiveRevealView { value })
    }

    /// 验证根命令及其嵌套资料输入。
    fn validate_request(&self, req: &SaveCustomerProfileRequest, create: bool) -> Result<()> {
        req.validate()?;
        validate_default_count(req.contacts.as_deref(), |item| item.is_default, "联系人")?;
        validate_default_count(req.addresses.as_deref(), |item| item.is_default, "地址")?;
        validate_default_count(req.bank_accounts.as_deref(), |item| item.is_default, "银行账户")?;
        validate_unique_existing_ids(req)?;
        for contact in req.contacts.as_deref().unwrap_or_default() {
            validate_contact_input(contact, create)?;
        }
        for address in req.addresses.as_deref().unwrap_or_default() {
            validate_address_input(address, create)?;
        }
        for account in req.bank_accounts.as_deref().unwrap_or_default() {
            validate_bank_input(account, create)?;
        }
        validate_create_or_update_shape(req, create)
    }

    /// 加载已成功命令记录。
    async fn command_record(&self, idempotency_key: &str) -> Result<Option<CustomerProfileCommand>> {
        Ok(self
            .db
            .customer_profile_commands()
            .find_by_idempotency_key(idempotency_key, &mut NoTransaction)
            .await?)
    }

    /// 解析客户资料事务结果，并在失败后尝试重放同幂等键的已提交命令。
    ///
    /// `transaction` 是本次事务结果，`intended` 是成功时的稳定视图，`context` 提供幂等核对字段。
    /// 事务成功时直接返回预期视图；事务失败但并发请求已提交相同命令时返回已提交结果。
    /// 查询幂等记录失败、记录与请求上下文冲突或原事务失败且没有已提交记录时返回错误。
    async fn resolve_transaction(
        &self,
        transaction: Result<()>,
        intended: CustomerProfileMutationView,
        context: TransactionResolutionContext<'_>,
    ) -> Result<CustomerProfileMutationView> {
        match transaction {
            Ok(()) => Ok(intended),
            Err(error) => match self.command_record(context.idempotency_key).await? {
                Some(command) => replay_command(
                    command,
                    context.operation,
                    context.customer_id,
                    context.initiated_by,
                    context.fingerprint,
                ),
                None => Err(error),
            },
        }
    }

    /// 构造完整创建事务载荷。
    fn prepare_create(
        &self,
        req: SaveCustomerProfileRequest,
        owner_user_id: String,
        fingerprint: String,
        actor: &AuditActor,
    ) -> Result<PreparedCreate> {
        let party_id = PartyId::new(next_id());
        let customer_id = CustomerAccountId::new(next_id());
        let revision_id = PartyRevisionId::new(next_id());
        let (party, revision) = create_party(&req, &party_id, &revision_id, actor.id())?;
        let account = create_customer(&req, &customer_id, &party_id, actor.id())?;
        let assignment = create_owner(&req, &customer_id, owner_user_id)?;
        let facts = self.create_facts(&req, &party_id, actor.id())?;
        PreparedCreate::new(
            PreparedCreateParts {
                party,
                revision,
                account,
                assignment,
                facts,
            },
            req,
            fingerprint,
            actor,
        )
    }

    /// 加载当前聚合并构造完整修订事务载荷。
    async fn prepare_update(
        &self,
        customer_id: &str,
        req: SaveCustomerProfileRequest,
        fingerprint: String,
        actor: &AuditActor,
    ) -> Result<PreparedUpdate> {
        let mut account = self.load_customer(customer_id).await?;
        ensure_version(
            account.base.version,
            required_version(req.expected_customer_version, "客户")?,
        )?;
        let mut party = self.load_party(&account.party_id).await?;
        ensure_version(
            party.base.version,
            required_version(req.expected_party_version, "主体")?,
        )?;
        let revision_no = self.next_revision_no(&account.party_id).await?;
        let revision = update_roots(&mut party, &mut account, &req, revision_no, actor.id())?;
        let facts = self
            .prepare_fact_changes(&account.party_id, &req, actor.id())
            .await?;
        PreparedUpdate::new(party, revision, account, facts, req, fingerprint, actor)
    }

    /// 构造创建场景的全部从属事实并完成敏感值加密。
    fn create_facts(
        &self,
        req: &SaveCustomerProfileRequest,
        party_id: &PartyId,
        actor: &str,
    ) -> Result<PartyFacts> {
        let contacts = req
            .contacts
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|input| self.new_contact(input, party_id, req.effective_from, actor))
            .collect::<Result<Vec<_>>>()?;
        let addresses = req
            .addresses
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|input| self.new_address(input, party_id, req.effective_from, actor))
            .collect::<Result<Vec<_>>>()?;
        let bank_accounts = req
            .bank_accounts
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|input| self.new_bank_account(input, party_id, req.effective_from, actor))
            .collect::<Result<Vec<_>>>()?;
        Ok(PartyFacts {
            contacts,
            addresses,
            bank_accounts,
        })
    }

    /// 为显式提交的事实集合计算保留、结束和新增差异。
    async fn prepare_fact_changes(
        &self,
        party_id: &PartyId,
        req: &SaveCustomerProfileRequest,
        actor: &str,
    ) -> Result<PartyFactChanges> {
        let mut changes = PartyFactChanges::default();
        if let Some(inputs) = &req.contacts {
            let existing = self.current_contacts(party_id, req.effective_from).await?;
            changes.contacts = self.diff_contacts(existing, inputs, party_id, req.effective_from, actor)?;
        }
        if let Some(inputs) = &req.addresses {
            let existing = self.current_addresses(party_id, req.effective_from).await?;
            changes.addresses = self.diff_addresses(existing, inputs, party_id, req.effective_from, actor)?;
        }
        if let Some(inputs) = &req.bank_accounts {
            let existing = self.current_bank_accounts(party_id, req.effective_from).await?;
            changes.bank_accounts =
                self.diff_bank_accounts(existing, inputs, party_id, req.effective_from, actor)?;
        }
        Ok(changes)
    }

    /// 构造并加密联系人事实。
    fn new_contact(
        &self,
        input: &CustomerProfileContactInput,
        party_id: &PartyId,
        valid_from: BusinessDate,
        actor: &str,
    ) -> Result<PartyContact> {
        let mobile = required_text(input.mobile.as_deref(), "手机号")?;
        let mut contact = PartyContact::new(
            PartyContactId::new(next_id()),
            PartyContactData {
                party_id: party_id.clone(),
                contact_name: input.contact_name.clone(),
                title: input.title.clone(),
                mobile: mobile.clone(),
                telephone: input.telephone.clone(),
                email: input.email.clone(),
                valid_from,
                valid_to: None,
                is_default: input.is_default,
                status: EffectiveRecordStatus::Active,
            },
            self.sensitive_data.fingerprint_key(),
            actor,
        )?;
        contact.mobile_ciphertext = self.sensitive_data.encrypt(&mobile)?;
        Ok(contact)
    }

    /// 构造并加密地址事实。
    fn new_address(
        &self,
        input: &CustomerProfileAddressInput,
        party_id: &PartyId,
        valid_from: BusinessDate,
        actor: &str,
    ) -> Result<PartyAddress> {
        let address = required_text(input.address.as_deref(), "地址")?;
        let mut entity = PartyAddress::new(
            PartyAddressId::new(next_id()),
            PartyAddressData {
                party_id: party_id.clone(),
                address_type: input.address_type,
                contact_name: input.contact_name.clone(),
                address: address.clone(),
                valid_from,
                valid_to: None,
                is_default: input.is_default,
                status: EffectiveRecordStatus::Active,
            },
            self.sensitive_data.fingerprint_key(),
            actor,
        )?;
        entity.address_ciphertext = self.sensitive_data.encrypt(&address)?;
        Ok(entity)
    }

    /// 构造并加密银行账户事实；内部账户编号由服务端生成。
    fn new_bank_account(
        &self,
        input: &CustomerProfileBankAccountInput,
        party_id: &PartyId,
        valid_from: BusinessDate,
        actor: &str,
    ) -> Result<PartyBankAccount> {
        let account_number = required_text(input.account_number.as_deref(), "银行账号")?;
        let mut entity = PartyBankAccount::new(
            PartyBankAccountId::new(next_id()),
            PartyBankAccountData {
                bank_account_no: business_no("BA"),
                party_id: party_id.clone(),
                account_name: input.account_name.clone(),
                bank_name: input.bank_name.clone(),
                bank_branch_name: input.bank_branch_name.clone(),
                account_number: account_number.clone(),
                valid_from,
                valid_to: None,
                is_default: input.is_default,
                status: EffectiveRecordStatus::Active,
            },
            self.sensitive_data.fingerprint_key(),
            actor,
        )?;
        entity.account_number_ciphertext = self.sensitive_data.encrypt(&account_number)?;
        Ok(entity)
    }

    /// 计算联系人集合差异；既有行未携带明文且元数据未变化时原样保留。
    fn diff_contacts(
        &self,
        existing: Vec<PartyContact>,
        inputs: &[CustomerProfileContactInput],
        party_id: &PartyId,
        effective_from: BusinessDate,
        actor: &str,
    ) -> Result<EntityChanges<PartyContact>> {
        let mut current = by_id(existing, |item| item.base.id.clone());
        let mut changes = EntityChanges::default();
        for input in inputs {
            let Some(existing_id) = input.existing_id.as_deref() else {
                changes
                    .created
                    .push(self.new_contact(input, party_id, effective_from, actor)?);
                continue;
            };
            let mut entity = take_existing(&mut current, existing_id, "联系人")?;
            if contact_matches(&entity, input, self.sensitive_data.fingerprint_key()) {
                update_contact_default(&mut entity, input.is_default, actor, &mut changes.updated)?;
                continue;
            }
            let mut replacement = input.clone();
            if replacement
                .mobile
                .as_deref()
                .is_none_or(|value| value.trim().is_empty())
            {
                replacement.mobile = Some(self.sensitive_data.decrypt(&entity.mobile_ciphertext)?);
            }
            close_contact(&mut entity, effective_from, actor)?;
            changes.updated.push(entity);
            changes
                .created
                .push(self.new_contact(&replacement, party_id, effective_from, actor)?);
        }
        close_remaining_contacts(current, effective_from, actor, &mut changes.updated)?;
        Ok(changes)
    }

    /// 计算地址集合差异；既有行未携带明文且元数据未变化时原样保留。
    fn diff_addresses(
        &self,
        existing: Vec<PartyAddress>,
        inputs: &[CustomerProfileAddressInput],
        party_id: &PartyId,
        effective_from: BusinessDate,
        actor: &str,
    ) -> Result<EntityChanges<PartyAddress>> {
        let mut current = by_id(existing, |item| item.base.id.clone());
        let mut changes = EntityChanges::default();
        for input in inputs {
            let Some(existing_id) = input.existing_id.as_deref() else {
                changes
                    .created
                    .push(self.new_address(input, party_id, effective_from, actor)?);
                continue;
            };
            let mut entity = take_existing(&mut current, existing_id, "地址")?;
            if address_matches(&entity, input, self.sensitive_data.fingerprint_key()) {
                update_address_default(&mut entity, input.is_default, actor, &mut changes.updated)?;
                continue;
            }
            let mut replacement = input.clone();
            if replacement
                .address
                .as_deref()
                .is_none_or(|value| value.trim().is_empty())
            {
                replacement.address = Some(self.sensitive_data.decrypt(&entity.address_ciphertext)?);
            }
            close_address(&mut entity, effective_from, actor)?;
            changes.updated.push(entity);
            changes
                .created
                .push(self.new_address(&replacement, party_id, effective_from, actor)?);
        }
        close_remaining_addresses(current, effective_from, actor, &mut changes.updated)?;
        Ok(changes)
    }

    /// 计算银行账户集合差异；既有稳定账户只允许调整默认标记或结束。
    fn diff_bank_accounts(
        &self,
        existing: Vec<PartyBankAccount>,
        inputs: &[CustomerProfileBankAccountInput],
        party_id: &PartyId,
        effective_from: BusinessDate,
        actor: &str,
    ) -> Result<EntityChanges<PartyBankAccount>> {
        let mut current = by_id(existing, |item| item.base.id.clone());
        let mut changes = EntityChanges::default();
        for input in inputs {
            let Some(existing_id) = input.existing_id.as_deref() else {
                changes
                    .created
                    .push(self.new_bank_account(input, party_id, effective_from, actor)?);
                continue;
            };
            let mut entity = take_existing(&mut current, existing_id, "银行账户")?;
            if !bank_account_matches(&entity, input, self.sensitive_data.fingerprint_key()) {
                return Err(Error::ValidationError(
                    "既有银行账户内容不可原地修改，请结束旧账户后新增账户".to_string(),
                ));
            }
            update_bank_default(&mut entity, input.is_default, actor, &mut changes.updated)?;
        }
        close_remaining_banks(current, effective_from, actor, &mut changes.updated)?;
        Ok(changes)
    }

    /// 查询指定日期当前有效联系人。
    async fn current_contacts(&self, party_id: &PartyId, as_of: BusinessDate) -> Result<Vec<PartyContact>> {
        self.db
            .party_contacts()
            .find_many(current_fact_filter(party_id, as_of), &mut NoTransaction)
            .await
            .map_err(Into::into)
    }

    /// 查询指定日期当前有效地址。
    async fn current_addresses(&self, party_id: &PartyId, as_of: BusinessDate) -> Result<Vec<PartyAddress>> {
        self.db
            .party_addresses()
            .find_many(current_fact_filter(party_id, as_of), &mut NoTransaction)
            .await
            .map_err(Into::into)
    }

    /// 查询指定日期当前有效银行账户。
    async fn current_bank_accounts(
        &self,
        party_id: &PartyId,
        as_of: BusinessDate,
    ) -> Result<Vec<PartyBankAccount>> {
        self.db
            .party_bank_accounts()
            .find_many(current_fact_filter(party_id, as_of), &mut NoTransaction)
            .await
            .map_err(Into::into)
    }

    /// 查询对象中心所需的当前有效 Party 从属事实。
    async fn current_party_facts(&self, party_id: &PartyId) -> Result<CurrentPartyFacts> {
        let filter = current_fact_filter(party_id, BusinessDate::today());
        let contacts = self
            .db
            .party_contacts()
            .find_many_sorted(
                filter.clone(),
                doc! { "is_default": -1, "created_at": -1 },
                &mut NoTransaction,
            )
            .await?;
        let addresses = self
            .db
            .party_addresses()
            .find_many_sorted(
                filter.clone(),
                doc! { "is_default": -1, "created_at": -1 },
                &mut NoTransaction,
            )
            .await?;
        let tax_profiles = self
            .db
            .party_tax_profiles()
            .find_many_sorted(
                filter.clone(),
                doc! { "is_default": -1, "created_at": -1 },
                &mut NoTransaction,
            )
            .await?;
        let bank_accounts = self
            .db
            .party_bank_accounts()
            .find_many_sorted(
                filter,
                doc! { "is_default": -1, "created_at": -1 },
                &mut NoTransaction,
            )
            .await?;
        Ok((contacts, addresses, tax_profiles, bank_accounts))
    }

    /// 查询客户全部归属历史。
    async fn assignments(&self, customer_id: &str) -> Result<Vec<CustomerAssignment>> {
        Ok(self
            .db
            .customer_assignments()
            .find_many_sorted(
                doc! { "customer_id": customer_id },
                doc! { "valid_from": -1, "created_at": -1 },
                &mut NoTransaction,
            )
            .await?)
    }

    /// 批量查询归属账号展示名。
    async fn assignment_account_names(
        &self,
        assignments: &[CustomerAssignment],
    ) -> Result<HashMap<String, String>> {
        let account_ids: Vec<String> = assignments
            .iter()
            .map(|assignment| assignment.user_id.clone())
            .collect();
        Ok(self
            .db
            .accounts()
            .find_many(doc! { "id": { "$in": account_ids } }, &mut NoTransaction)
            .await?
            .into_iter()
            .map(|account| (account.base.id, account.name))
            .collect())
    }

    /// 为每条当前敏感事实签发一分钟有效的字段级令牌。
    fn sensitive_fields(
        &self,
        customer_id: &str,
        contacts: &[PartyContact],
        addresses: &[PartyAddress],
        bank_accounts: &[PartyBankAccount],
    ) -> Result<Vec<CustomerSensitiveFieldView>> {
        let expires_at = unix_now()? + 60;
        let mut fields = Vec::with_capacity(contacts.len() + addresses.len() + bank_accounts.len());
        for contact in contacts {
            fields.push(self.sensitive_field(
                SensitiveFieldKind::ContactMobile,
                &contact.base.id,
                customer_id,
                masked_last4(&contact.mobile_last4),
                expires_at,
            )?);
        }
        for address in addresses {
            fields.push(self.sensitive_field(
                SensitiveFieldKind::Address,
                &address.base.id,
                customer_id,
                "********".to_string(),
                expires_at,
            )?);
        }
        for account in bank_accounts {
            fields.push(self.sensitive_field(
                SensitiveFieldKind::BankAccountNumber,
                &account.base.id,
                customer_id,
                masked_last4(&account.account_number_last4),
                expires_at,
            )?);
        }
        Ok(fields)
    }

    /// 签发一个受客户与事实行约束的敏感字段令牌。
    fn sensitive_field(
        &self,
        kind: SensitiveFieldKind,
        record_id: &str,
        customer_id: &str,
        masked_value: String,
        expires_at: u64,
    ) -> Result<CustomerSensitiveFieldView> {
        let reveal_token =
            self.sensitive_data
                .issue_reveal_token(kind, record_id, customer_id, expires_at)?;
        Ok(CustomerSensitiveFieldView {
            kind,
            record_id: record_id.to_string(),
            masked_value,
            reveal_token,
            expires_at,
        })
    }

    /// 读取令牌指定事实的密文并校验其 Party 归属。
    async fn sensitive_ciphertext(
        &self,
        kind: SensitiveFieldKind,
        record_id: &str,
        party_id: &PartyId,
    ) -> Result<String> {
        match kind {
            SensitiveFieldKind::ContactMobile => {
                let record = self
                    .db
                    .party_contacts()
                    .find_by_id(record_id, &mut NoTransaction)
                    .await?
                    .ok_or_else(|| Error::NotFound("联系人不存在".to_string()))?;
                ensure_party(&record.party_id, party_id)?;
                Ok(record.mobile_ciphertext)
            }
            SensitiveFieldKind::Address => {
                let record = self
                    .db
                    .party_addresses()
                    .find_by_id(record_id, &mut NoTransaction)
                    .await?
                    .ok_or_else(|| Error::NotFound("地址不存在".to_string()))?;
                ensure_party(&record.party_id, party_id)?;
                Ok(record.address_ciphertext)
            }
            SensitiveFieldKind::BankAccountNumber => {
                let record = self
                    .db
                    .party_bank_accounts()
                    .find_by_id(record_id, &mut NoTransaction)
                    .await?
                    .ok_or_else(|| Error::NotFound("银行账户不存在".to_string()))?;
                ensure_party(&record.party_id, party_id)?;
                Ok(record.account_number_ciphertext)
            }
        }
    }

    /// 加载客户角色。
    async fn load_customer(&self, id: &str) -> Result<CustomerAccount> {
        self.db
            .customer_accounts()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("客户不存在".to_string()))
    }

    /// 加载客户关联 Party。
    async fn load_party(&self, party_id: &PartyId) -> Result<Party> {
        self.db
            .parties()
            .find_by_id(party_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("客户关联主体不存在".to_string()))
    }

    /// 校验负责人账号存在。
    async fn ensure_user_exists(&self, user_id: &str) -> Result<()> {
        let account = self
            .db
            .accounts()
            .find_by_id(user_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("负责销售账号不存在".to_string()))?;
        if !account.status.is_active() {
            return Err(Error::BusinessLogicError(
                "负责销售账号已停用，不能建立客户归属".to_string(),
            ));
        }
        Ok(())
    }

    /// 返回 Party 的下一修订号。
    async fn next_revision_no(&self, party_id: &PartyId) -> Result<u32> {
        let revisions = self
            .db
            .party_revisions()
            .list_revision_history(party_id, &mut NoTransaction)
            .await?;
        Ok(revisions
            .iter()
            .map(|item| item.revision.revision_no)
            .max()
            .unwrap_or(0)
            + 1)
    }
}

/// 创建事务载荷。
struct PreparedCreateParts {
    party: Party,
    revision: PartyRevision,
    account: CustomerAccount,
    assignment: CustomerAssignment,
    facts: PartyFacts,
}

/// 创建事务载荷及其幂等、审计事实。
struct PreparedCreate {
    party: Party,
    revision: PartyRevision,
    account: CustomerAccount,
    assignment: CustomerAssignment,
    facts: PartyFacts,
    command: CustomerProfileCommand,
    audit: entities::AuditLog,
    result: CustomerProfileMutationView,
}

impl PreparedCreate {
    /// 构造命令、审计及稳定结果。
    fn new(
        parts: PreparedCreateParts,
        req: SaveCustomerProfileRequest,
        fingerprint: String,
        actor: &AuditActor,
    ) -> Result<Self> {
        let PreparedCreateParts {
            party,
            revision,
            account,
            assignment,
            facts,
        } = parts;
        let command = profile_command(
            &party,
            &revision,
            &account,
            ProfileCommandInput {
                operation: "create",
                req,
                fingerprint,
                initiated_by: actor.id(),
                customer_version: account.base.version,
                party_version: party.base.version,
            },
        )?;
        let result = command_view(command.clone());
        let audit = actor.clone().resource_log(
            "customer_profile.create",
            "customer_profile",
            account.base.id.clone(),
        )?;
        Ok(Self {
            party,
            revision,
            account,
            assignment,
            facts,
            command,
            audit,
            result,
        })
    }

    /// 将完整客户资料与幂等结果写入同一事务。
    async fn persist(self, db: &Database, session: &mut mongodb::ClientSession) -> Result<()> {
        db.party_revisions().create(&self.revision, session).await?;
        db.parties().create(&self.party, session).await?;
        db.customer_accounts().create(&self.account, session).await?;
        db.customer_assignments()
            .create(&self.assignment, session)
            .await?;
        self.facts.persist(db, session).await?;
        db.customer_profile_commands()
            .create(&self.command, session)
            .await?;
        db.audit_logs().create(&self.audit, session).await?;
        Ok(())
    }
}

/// 修订事务载荷。
struct PreparedUpdate {
    party: Party,
    revision: PartyRevision,
    account: CustomerAccount,
    facts: PartyFactChanges,
    command: CustomerProfileCommand,
    audit: entities::AuditLog,
    result: CustomerProfileMutationView,
}

impl PreparedUpdate {
    /// 构造命令、审计及保存后的稳定版本结果。
    fn new(
        party: Party,
        revision: PartyRevision,
        account: CustomerAccount,
        facts: PartyFactChanges,
        req: SaveCustomerProfileRequest,
        fingerprint: String,
        actor: &AuditActor,
    ) -> Result<Self> {
        let command = profile_command(
            &party,
            &revision,
            &account,
            ProfileCommandInput {
                operation: "update",
                req,
                fingerprint,
                initiated_by: actor.id(),
                customer_version: account.base.version + 1,
                party_version: party.base.version + 1,
            },
        )?;
        let result = command_view(command.clone());
        let audit = actor.clone().resource_log(
            "customer_profile.update",
            "customer_profile",
            account.base.id.clone(),
        )?;
        Ok(Self {
            party,
            revision,
            account,
            facts,
            command,
            audit,
            result,
        })
    }

    /// 将根修订、事实差异、幂等结果与审计写入同一事务。
    async fn persist(mut self, db: &Database, session: &mut mongodb::ClientSession) -> Result<()> {
        db.party_revisions().create(&self.revision, session).await?;
        db.parties().update(&mut self.party, session).await?;
        db.customer_accounts().update(&mut self.account, session).await?;
        self.facts.persist(db, session).await?;
        db.customer_profile_commands()
            .create(&self.command, session)
            .await?;
        db.audit_logs().create(&self.audit, session).await?;
        Ok(())
    }
}

/// 创建场景的从属事实。
#[derive(Default)]
struct PartyFacts {
    contacts: Vec<PartyContact>,
    addresses: Vec<PartyAddress>,
    bank_accounts: Vec<PartyBankAccount>,
}

impl PartyFacts {
    /// 写入全部新事实。
    async fn persist(self, db: &Database, session: &mut mongodb::ClientSession) -> Result<()> {
        for item in &self.contacts {
            db.party_contacts().create(item, session).await?;
        }
        for item in &self.addresses {
            db.party_addresses().create(item, session).await?;
        }
        for item in &self.bank_accounts {
            db.party_bank_accounts().create(item, session).await?;
        }
        Ok(())
    }
}

/// 一类有效期事实的更新与新增差异。
struct EntityChanges<T> {
    updated: Vec<T>,
    created: Vec<T>,
}

impl<T> Default for EntityChanges<T> {
    fn default() -> Self {
        Self {
            updated: Vec::new(),
            created: Vec::new(),
        }
    }
}

/// 修订场景的 Party 事实差异。
#[derive(Default)]
struct PartyFactChanges {
    contacts: EntityChanges<PartyContact>,
    addresses: EntityChanges<PartyAddress>,
    bank_accounts: EntityChanges<PartyBankAccount>,
}

impl PartyFactChanges {
    /// 按先结束旧事实、后写新事实的顺序持久化差异。
    async fn persist(mut self, db: &Database, session: &mut mongodb::ClientSession) -> Result<()> {
        for item in &mut self.contacts.updated {
            db.party_contacts().update(item, session).await?;
        }
        for item in &self.contacts.created {
            db.party_contacts().create(item, session).await?;
        }
        for item in &mut self.addresses.updated {
            db.party_addresses().update(item, session).await?;
        }
        for item in &self.addresses.created {
            db.party_addresses().create(item, session).await?;
        }
        for item in &mut self.bank_accounts.updated {
            db.party_bank_accounts().update(item, session).await?;
        }
        for item in &self.bank_accounts.created {
            db.party_bank_accounts().create(item, session).await?;
        }
        Ok(())
    }
}

/// 当前 Party 事实查询结果。
type CurrentPartyFacts = (
    Vec<PartyContact>,
    Vec<PartyAddress>,
    Vec<entities::party::PartyTaxProfile>,
    Vec<PartyBankAccount>,
);

/// 创建 Party 与首个名称修订。
fn create_party(
    req: &SaveCustomerProfileRequest,
    party_id: &PartyId,
    revision_id: &PartyRevisionId,
    actor: &str,
) -> Result<(Party, PartyRevision)> {
    let mut party = Party::new(
        party_id.clone(),
        PartyData {
            party_no: business_no("P"),
            party_kind: PartyKind::Enterprise,
            unified_credit_code: req.unified_credit_code.clone(),
            status: PartyStatus::Active,
        },
        actor,
    )?;
    party.stable.current_revision_id = Some(revision_id.to_string());
    let revision = PartyRevision::new(
        revision_id.clone(),
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

/// 创建客户角色。
fn create_customer(
    req: &SaveCustomerProfileRequest,
    customer_id: &CustomerAccountId,
    party_id: &PartyId,
    actor: &str,
) -> Result<CustomerAccount> {
    Ok(CustomerAccount::new(
        customer_id.clone(),
        CustomerAccountData {
            party_id: party_id.clone(),
            customer_no: business_no("KH"),
            default_payment_term_id: req.default_payment_term_id.clone(),
            status: req.status.unwrap_or(CustomerAccountStatus::Active),
        },
        actor,
    )?)
}

/// 创建首条 OWNER 归属。
fn create_owner(
    req: &SaveCustomerProfileRequest,
    customer_id: &CustomerAccountId,
    owner_user_id: String,
) -> Result<CustomerAssignment> {
    Ok(CustomerAssignment::new(
        CustomerAssignmentId::new(next_id()),
        CustomerAssignmentData {
            customer_id: customer_id.clone(),
            user_id: owner_user_id,
            assignment_role: AssignmentRole::Owner,
            valid_from: req.effective_from,
            valid_to: None,
            change_reason: req.change_reason.clone(),
        },
    )?)
}

/// 形成新的 Party 名称修订并更新 Party/客户稳定字段。
fn update_roots(
    party: &mut Party,
    account: &mut CustomerAccount,
    req: &SaveCustomerProfileRequest,
    revision_no: u32,
    actor: &str,
) -> Result<PartyRevision> {
    let revision_id = PartyRevisionId::new(next_id());
    party.update(
        PartyUpdate {
            unified_credit_code: string_update(req.unified_credit_code.clone()),
            status: None,
        },
        actor,
    )?;
    party.stable.current_revision_id = Some(revision_id.to_string());
    let status = req.status.filter(|status| *status != account.stable.status);
    account.update(
        CustomerAccountUpdate {
            default_payment_term_id: string_update(req.default_payment_term_id.clone()),
            status,
        },
        actor,
    )?;
    Ok(PartyRevision::new(
        revision_id,
        PartyRevisionData {
            party_id: party_id(party),
            revision_no,
            legal_name: req.legal_name.clone(),
            short_name: req.short_name.clone(),
            change_reason: req.change_reason.clone(),
        },
    )?)
}

/// 构造幂等命令所需的请求与结果版本上下文。
struct ProfileCommandInput<'a> {
    operation: &'a str,
    req: SaveCustomerProfileRequest,
    fingerprint: String,
    initiated_by: &'a str,
    customer_version: u64,
    party_version: u64,
}

/// 构造幂等命令实体。
fn profile_command(
    party: &Party,
    revision: &PartyRevision,
    account: &CustomerAccount,
    input: ProfileCommandInput<'_>,
) -> Result<CustomerProfileCommand> {
    let ProfileCommandInput {
        operation,
        req,
        fingerprint,
        initiated_by,
        customer_version,
        party_version,
    } = input;
    Ok(CustomerProfileCommand::new(
        next_id(),
        CustomerProfileCommandData {
            idempotency_key: req.idempotency_key,
            operation: operation.to_string(),
            initiated_by: initiated_by.to_string(),
            request_fingerprint: fingerprint,
            customer_id: account.base.id.clone(),
            customer_no: account.customer_no.clone(),
            party_id: party.base.id.clone(),
            revision_id: revision.base.id.clone(),
            revision_no: revision.revision.revision_no,
            customer_version,
            party_version,
            effective_from: req.effective_from,
            change_reason: req.change_reason,
        },
    )?)
}

/// 客户资料完整详情视图的已加载事实。
struct ProfileDetailParts {
    account: CustomerAccount,
    party: Party,
    current_revision: PartyRevision,
    revisions: Vec<PartyRevision>,
    assignments: Vec<CustomerAssignment>,
    contacts: Vec<PartyContact>,
    addresses: Vec<PartyAddress>,
    tax_profiles: Vec<entities::party::PartyTaxProfile>,
    bank_accounts: Vec<PartyBankAccount>,
    sensitive_fields: Vec<CustomerSensitiveFieldView>,
}

/// 构造客户资料完整详情视图。
fn build_detail(parts: ProfileDetailParts) -> CustomerProfileDetailView {
    let ProfileDetailParts {
        account,
        party,
        current_revision,
        mut revisions,
        assignments,
        contacts,
        addresses,
        tax_profiles,
        bank_accounts,
        sensitive_fields,
    } = parts;
    let today = BusinessDate::today();
    let owner_user_id = assignments
        .iter()
        .find(|item| item.assignment_role == AssignmentRole::Owner && item.is_active_on(today))
        .map(|item| item.user_id.clone());
    let collaborator_count = assignments
        .iter()
        .filter(|item| item.assignment_role == AssignmentRole::Collaborator && item.is_active_on(today))
        .count() as u32;
    let mut customer = CustomerView::from(account);
    customer.party_no = Some(party.party_no.clone());
    customer.legal_name = Some(current_revision.legal_name.clone());
    customer.short_name = current_revision.short_name.clone();
    customer.owner_user_id = owner_user_id;
    customer.collaborator_count = collaborator_count;
    revisions.sort_by_key(|item| std::cmp::Reverse(item.revision.revision_no));
    let action_blockers = customer_status_blockers(customer.status);
    CustomerProfileDetailView {
        account: customer,
        party_status: party.stable.status,
        party_version: party.base.version,
        unified_credit_code: party.unified_credit_code,
        current_revision: current_revision.into(),
        revisions: revisions.into_iter().map(Into::into).collect(),
        assignments: assignments
            .into_iter()
            .map(CustomerAssignmentView::from)
            .collect(),
        contacts: contacts.into_iter().map(Into::into).collect(),
        addresses: addresses.into_iter().map(Into::into).collect(),
        tax_profiles: tax_profiles.into_iter().map(Into::into).collect(),
        bank_accounts: bank_accounts.into_iter().map(Into::into).collect(),
        sensitive_fields,
        allowed_actions: Vec::new(),
        action_blockers,
    }
}

/// 返回客户状态产生的业务动作阻断原因。
fn customer_status_blockers(status: CustomerAccountStatus) -> Vec<CustomerActionBlockerView> {
    if status.is_active() {
        return Vec::new();
    }
    ["UPLOAD_CONTRACT_PDF", "CREATE_SALES_ORDER"]
        .into_iter()
        .map(|action| CustomerActionBlockerView {
            action: action.to_string(),
            code: "CUSTOMER_DISABLED".to_string(),
            message: "客户已停用，请先恢复客户后再发起新业务".to_string(),
        })
        .collect()
}

/// 查找 Party 当前修订；缺失视为聚合损坏。
fn current_revision(party: &Party, revisions: &[PartyRevision]) -> Result<PartyRevision> {
    let revision_id = party
        .stable
        .current_revision_id
        .as_deref()
        .ok_or_else(|| Error::Internal("客户主体缺少当前名称修订".to_string()))?;
    revisions
        .iter()
        .find(|item| item.base.id == revision_id)
        .cloned()
        .ok_or_else(|| Error::Internal("客户主体当前名称修订不存在".to_string()))
}

/// 把实体集合转成按 ID 索引的当前集合。
fn by_id<T>(items: Vec<T>, id: impl Fn(&T) -> String) -> HashMap<String, T> {
    items.into_iter().map(|item| (id(&item), item)).collect()
}

/// 从当前集合取出客户端引用的既有事实。
fn take_existing<T>(current: &mut HashMap<String, T>, id: &str, label: &str) -> Result<T> {
    current
        .remove(id)
        .ok_or_else(|| Error::ConflictError(format!("{label}已变化，请刷新后重试")))
}

/// 判断联系人输入是否与当前事实内容一致。
fn contact_matches(contact: &PartyContact, input: &CustomerProfileContactInput, key: &[u8]) -> bool {
    let mobile_matches = input.mobile.as_deref().is_none_or(|mobile| {
        mobile.trim().is_empty() || PartyContact::mobile_fingerprint(mobile, key) == contact.mobile_query_hmac
    });
    mobile_matches
        && contact.contact_name == input.contact_name.trim()
        && normalized_optional(&contact.title) == normalized_optional(&input.title)
        && normalized_optional(&contact.telephone) == normalized_optional(&input.telephone)
        && normalized_optional(&contact.email) == normalized_optional(&input.email)
}

/// 判断地址输入是否与当前事实内容一致。
fn address_matches(address: &PartyAddress, input: &CustomerProfileAddressInput, key: &[u8]) -> bool {
    let content_matches = input.address.as_deref().is_none_or(|value| {
        value.trim().is_empty() || PartyAddress::address_fingerprint(value, key) == address.address_query_hmac
    });
    content_matches
        && address.address_type == input.address_type
        && normalized_optional(&address.contact_name) == normalized_optional(&input.contact_name)
}

/// 判断既有银行账户稳定内容是否未被修改。
fn bank_account_matches(
    account: &PartyBankAccount,
    input: &CustomerProfileBankAccountInput,
    key: &[u8],
) -> bool {
    let number_matches = input.account_number.as_deref().is_none_or(|value| {
        value.trim().is_empty()
            || PartyBankAccount::account_number_fingerprint(value, key) == account.account_number_query_hmac
    });
    number_matches
        && account.account_name == input.account_name.trim()
        && account.bank_name == input.bank_name.trim()
        && normalized_optional(&account.bank_branch_name) == normalized_optional(&input.bank_branch_name)
}

/// 仅在默认标记变化时更新联系人事实。
fn update_contact_default(
    contact: &mut PartyContact,
    is_default: bool,
    actor: &str,
    updated: &mut Vec<PartyContact>,
) -> Result<()> {
    if contact.is_default == is_default {
        return Ok(());
    }
    contact.update(
        PartyContactUpdate {
            is_default: Some(is_default),
            ..Default::default()
        },
        actor,
    )?;
    updated.push(contact.clone());
    Ok(())
}

/// 仅在默认标记变化时更新地址事实。
fn update_address_default(
    address: &mut PartyAddress,
    is_default: bool,
    actor: &str,
    updated: &mut Vec<PartyAddress>,
) -> Result<()> {
    if address.is_default == is_default {
        return Ok(());
    }
    address.update(
        PartyAddressUpdate {
            is_default: Some(is_default),
            ..Default::default()
        },
        actor,
    )?;
    updated.push(address.clone());
    Ok(())
}

/// 仅在默认标记变化时更新银行账户事实。
fn update_bank_default(
    account: &mut PartyBankAccount,
    is_default: bool,
    actor: &str,
    updated: &mut Vec<PartyBankAccount>,
) -> Result<()> {
    if account.is_default == is_default {
        return Ok(());
    }
    account.update(
        PartyBankAccountUpdate {
            is_default: Some(is_default),
            ..Default::default()
        },
        actor,
    )?;
    updated.push(account.clone());
    Ok(())
}

/// 结束联系人当前事实。
fn close_contact(contact: &mut PartyContact, effective_from: BusinessDate, actor: &str) -> Result<()> {
    Ok(contact.update(
        PartyContactUpdate {
            status: Some(EffectiveRecordStatus::Disabled),
            valid_to: close_date(contact.valid_from, effective_from),
            is_default: Some(false),
        },
        actor,
    )?)
}

/// 结束地址当前事实。
fn close_address(address: &mut PartyAddress, effective_from: BusinessDate, actor: &str) -> Result<()> {
    Ok(address.update(
        PartyAddressUpdate {
            status: Some(EffectiveRecordStatus::Disabled),
            valid_to: close_date(address.valid_from, effective_from),
            is_default: Some(false),
        },
        actor,
    )?)
}

/// 结束银行账户当前事实。
fn close_bank(account: &mut PartyBankAccount, effective_from: BusinessDate, actor: &str) -> Result<()> {
    Ok(account.update(
        PartyBankAccountUpdate {
            status: Some(EffectiveRecordStatus::Disabled),
            valid_to: close_date(account.valid_from, effective_from),
            is_default: Some(false),
        },
        actor,
    )?)
}

/// 结束未在目标集合中保留的联系人。
fn close_remaining_contacts(
    current: HashMap<String, PartyContact>,
    effective_from: BusinessDate,
    actor: &str,
    updated: &mut Vec<PartyContact>,
) -> Result<()> {
    for mut item in current.into_values() {
        close_contact(&mut item, effective_from, actor)?;
        updated.push(item);
    }
    Ok(())
}

/// 结束未在目标集合中保留的地址。
fn close_remaining_addresses(
    current: HashMap<String, PartyAddress>,
    effective_from: BusinessDate,
    actor: &str,
    updated: &mut Vec<PartyAddress>,
) -> Result<()> {
    for mut item in current.into_values() {
        close_address(&mut item, effective_from, actor)?;
        updated.push(item);
    }
    Ok(())
}

/// 结束未在目标集合中保留的银行账户。
fn close_remaining_banks(
    current: HashMap<String, PartyBankAccount>,
    effective_from: BusinessDate,
    actor: &str,
    updated: &mut Vec<PartyBankAccount>,
) -> Result<()> {
    for mut item in current.into_values() {
        close_bank(&mut item, effective_from, actor)?;
        updated.push(item);
    }
    Ok(())
}

/// 只有新生效日晚于旧事实开始日时才写结束日期；同日创建后修订仅停用。
fn close_date(valid_from: BusinessDate, effective_from: BusinessDate) -> FieldUpdate<BusinessDate> {
    if effective_from > valid_from {
        FieldUpdate::Set(effective_from)
    } else {
        FieldUpdate::Unchanged
    }
}

/// 构造当前有效事实过滤条件。
fn current_fact_filter(party_id: &PartyId, as_of: BusinessDate) -> mongodb::bson::Document {
    let as_of = as_of.to_string();
    doc! {
        "party_id": party_id.to_string(),
        "status": EffectiveRecordStatus::Active.as_str(),
        "valid_from": { "$lte": &as_of },
        "$or": [
            { "valid_to": null },
            { "valid_to": { "$gt": &as_of } },
        ],
    }
}

/// 生成服务端业务编号。
fn business_no(prefix: &str) -> String {
    format!("{prefix}-{}", next_id())
}

/// 将可选字符串映射为保留、清空或设置意图。
fn string_update(value: Option<String>) -> FieldUpdate<String> {
    match value {
        Some(value) if value.trim().is_empty() => FieldUpdate::Clear,
        Some(value) => FieldUpdate::Set(value),
        None => FieldUpdate::Unchanged,
    }
}

/// 返回 Party ID newtype。
fn party_id(party: &Party) -> PartyId {
    PartyId::new(party.base.id.clone())
}

/// 校验请求版本存在且大于零。
fn required_version(value: Option<u64>, label: &str) -> Result<u64> {
    value
        .filter(|version| *version > 0)
        .ok_or_else(|| Error::ValidationError(format!("修订时必须提供{label}版本")))
}

/// 校验乐观锁版本。
fn ensure_version(actual: u64, expected: u64) -> Result<()> {
    if actual == expected {
        return Ok(());
    }
    Err(Error::ConflictError(
        "数据已被其他请求修改，请刷新后重试".to_string(),
    ))
}

/// 校验必填文本并返回去空白值。
fn required_text(value: Option<&str>, label: &str) -> Result<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| Error::ValidationError(format!("{label}不能为空")))
}

/// 校验创建/修订专属字段。
///
/// # 参数
/// * `req` - 客户资料根命令
/// * `create` - `true` 表示创建，`false` 表示修订
///
/// # 返回
/// 形状合法时返回 `Ok(())`。
///
/// # 错误
/// 创建时携带既有版本，或修订时提交负责人 / 缺少乐观锁版本时返回校验错误。
///
/// # 约束
/// 创建时负责人由服务端按创建人写入，不要求也不使用请求体中的 `owner_user_id`。
fn validate_create_or_update_shape(req: &SaveCustomerProfileRequest, create: bool) -> Result<()> {
    if create {
        if req.expected_party_version.is_some() || req.expected_customer_version.is_some() {
            return Err(Error::ValidationError("创建客户时不能提交既有版本".to_string()));
        }
        return Ok(());
    }
    if req.owner_user_id.is_some() {
        return Err(Error::ValidationError(
            "负责人变更必须通过客户归属操作提交".to_string(),
        ));
    }
    required_version(req.expected_party_version, "主体")?;
    required_version(req.expected_customer_version, "客户")?;
    Ok(())
}

/// 校验联系人输入；新事实必须携带手机号明文。
fn validate_contact_input(input: &CustomerProfileContactInput, create: bool) -> Result<()> {
    input.validate()?;
    if create && input.existing_id.is_some() {
        return Err(Error::ValidationError("创建客户时不能引用既有联系人".to_string()));
    }
    if input.existing_id.is_none() {
        required_text(input.mobile.as_deref(), "手机号")?;
    }
    Ok(())
}

/// 校验地址输入；新事实必须携带地址明文。
fn validate_address_input(input: &CustomerProfileAddressInput, create: bool) -> Result<()> {
    input.validate()?;
    if create && input.existing_id.is_some() {
        return Err(Error::ValidationError("创建客户时不能引用既有地址".to_string()));
    }
    if input.existing_id.is_none() {
        required_text(input.address.as_deref(), "地址")?;
    }
    Ok(())
}

/// 校验银行账户输入；新稳定账户必须携带账号明文。
fn validate_bank_input(input: &CustomerProfileBankAccountInput, create: bool) -> Result<()> {
    input.validate()?;
    if create && input.existing_id.is_some() {
        return Err(Error::ValidationError(
            "创建客户时不能引用既有银行账户".to_string(),
        ));
    }
    if input.existing_id.is_none() {
        required_text(input.account_number.as_deref(), "银行账号")?;
    }
    Ok(())
}

/// 校验一类事实最多一个默认项。
fn validate_default_count<T>(
    items: Option<&[T]>,
    is_default: impl Fn(&T) -> bool,
    label: &str,
) -> Result<()> {
    let count = items
        .unwrap_or_default()
        .iter()
        .filter(|item| is_default(item))
        .count();
    if count <= 1 {
        return Ok(());
    }
    Err(Error::ValidationError(format!("同一时间只能有一个默认{label}")))
}

/// 校验每类事实中既有 ID 不重复。
fn validate_unique_existing_ids(req: &SaveCustomerProfileRequest) -> Result<()> {
    let groups = [
        req.contacts.as_ref().map(|items| {
            items
                .iter()
                .filter_map(|item| item.existing_id.as_deref())
                .collect::<Vec<_>>()
        }),
        req.addresses.as_ref().map(|items| {
            items
                .iter()
                .filter_map(|item| item.existing_id.as_deref())
                .collect::<Vec<_>>()
        }),
        req.bank_accounts.as_ref().map(|items| {
            items
                .iter()
                .filter_map(|item| item.existing_id.as_deref())
                .collect::<Vec<_>>()
        }),
    ];
    for ids in groups.into_iter().flatten() {
        let unique = ids.iter().collect::<std::collections::HashSet<_>>();
        if unique.len() != ids.len() {
            return Err(Error::ValidationError("同一资料事实不能重复提交".to_string()));
        }
    }
    Ok(())
}

/// 规范化可选文本供内容比较。
fn normalized_optional(value: &Option<String>) -> Option<&str> {
    value.as_deref().map(str::trim).filter(|value| !value.is_empty())
}

/// 校验敏感事实属于客户 Party。
fn ensure_party(actual: &PartyId, expected: &PartyId) -> Result<()> {
    if actual == expected {
        return Ok(());
    }
    Err(Error::Forbidden("敏感字段不属于当前客户".to_string()))
}

/// 返回当前 Unix 秒。
fn unix_now() -> Result<u64> {
    u64::try_from(Instant::now().unix_secs()).map_err(|_| Error::Internal("系统时间非法".to_string()))
}

/// 生成不可逆末四位掩码。
fn masked_last4(last4: &str) -> String {
    if last4.is_empty() {
        "****".to_string()
    } else {
        format!("****{last4}")
    }
}

/// 计算请求指纹；只落摘要，不持久化敏感请求正文。
fn request_fingerprint(req: &SaveCustomerProfileRequest) -> Result<String> {
    let payload =
        serde_json::to_vec(req).map_err(|_| Error::Internal("客户资料请求指纹计算失败".to_string()))?;
    Ok(format!("{:x}", Sha256::digest(payload)))
}

/// 重放已成功命令，并确保幂等键未被复用于其他请求。
fn replay_command(
    command: CustomerProfileCommand,
    operation: &str,
    customer_id: Option<&str>,
    initiated_by: &str,
    fingerprint: &str,
) -> Result<CustomerProfileMutationView> {
    let same_customer = customer_id.is_none_or(|id| id == command.customer_id);
    if command.operation != operation
        || !same_customer
        || command.initiated_by != initiated_by
        || command.request_fingerprint != fingerprint
    {
        return Err(Error::ConflictError("幂等键已用于另一项客户资料请求".to_string()));
    }
    Ok(command_view(command))
}

/// 将命令实体转换为稳定返回视图。
fn command_view(command: CustomerProfileCommand) -> CustomerProfileMutationView {
    CustomerProfileMutationView {
        initiated_by: command.initiated_by,
        customer_id: command.customer_id,
        customer_no: command.customer_no,
        party_id: command.party_id,
        revision_id: command.revision_id,
        revision_no: command.revision_no,
        customer_version: command.customer_version,
        party_version: command.party_version,
        effective_from: command.effective_from.to_string(),
        recorded_at: command.base.created_at,
        change_reason: command.change_reason,
    }
}

#[cfg(test)]
mod tests {
    use entities::{
        common::time::BusinessDate,
        customer::{CustomerProfileCommand, CustomerProfileCommandData},
    };

    use super::{
        customer_status_blockers, replay_command, validate_create_or_update_shape, validate_default_count,
        CustomerAccountStatus, SaveCustomerProfileRequest,
    };

    /// 构造最小合法的客户资料请求，供创建/修订形状校验用例复用。
    ///
    /// # 返回
    /// 返回不含负责销售与乐观锁版本的创建形态请求。
    fn sample_profile_request() -> SaveCustomerProfileRequest {
        SaveCustomerProfileRequest {
            idempotency_key: "key-1".to_string(),
            expected_party_version: None,
            expected_customer_version: None,
            legal_name: "主太帅".to_string(),
            short_name: None,
            unified_credit_code: None,
            default_payment_term_id: None,
            status: None,
            owner_user_id: None,
            contacts: None,
            addresses: None,
            bank_accounts: None,
            effective_from: BusinessDate::from_ymd(2026, 8, 8).unwrap(),
            change_reason: "首版建档".to_string(),
        }
    }

    #[test]
    fn default_fact_validation_accepts_zero_or_one_and_rejects_multiple() {
        assert!(validate_default_count(Some(&[false, true]), |value| *value, "联系人").is_ok());
        assert!(validate_default_count(Some(&[true, true]), |value| *value, "联系人").is_err());
    }

    #[test]
    fn disabled_customer_blocks_new_business_actions() {
        assert!(customer_status_blockers(CustomerAccountStatus::Active).is_empty());
        let blockers = customer_status_blockers(CustomerAccountStatus::Disabled);
        assert_eq!(blockers.len(), 2);
        assert!(blockers.iter().all(|item| item.code == "CUSTOMER_DISABLED"));
    }

    #[test]
    fn command_replay_requires_same_operation_customer_and_fingerprint() {
        let command = CustomerProfileCommand::new(
            "command-1",
            CustomerProfileCommandData {
                idempotency_key: "key-1".to_string(),
                operation: "update".to_string(),
                initiated_by: "admin-1".to_string(),
                request_fingerprint: "fingerprint-1".to_string(),
                customer_id: "customer-1".to_string(),
                customer_no: "KH-1".to_string(),
                party_id: "party-1".to_string(),
                revision_id: "revision-2".to_string(),
                revision_no: 2,
                customer_version: 2,
                party_version: 2,
                effective_from: BusinessDate::from_ymd(2026, 8, 8).unwrap(),
                change_reason: "资料修订".to_string(),
            },
        )
        .unwrap();
        assert!(replay_command(
            command.clone(),
            "update",
            Some("customer-1"),
            "admin-1",
            "fingerprint-1"
        )
        .is_ok());
        assert!(replay_command(command, "update", Some("customer-2"), "admin-1", "fingerprint-1").is_err());
    }

    #[test]
    fn create_shape_does_not_require_client_owner() {
        let request = sample_profile_request();
        assert!(validate_create_or_update_shape(&request, true).is_ok());

        let mut with_legacy_owner = sample_profile_request();
        with_legacy_owner.owner_user_id = Some("legacy-owner".to_string());
        assert!(validate_create_or_update_shape(&with_legacy_owner, true).is_ok());
    }

    #[test]
    fn update_shape_still_rejects_owner_field() {
        let mut request = sample_profile_request();
        request.expected_party_version = Some(1);
        request.expected_customer_version = Some(1);
        request.owner_user_id = Some("someone-else".to_string());
        assert!(validate_create_or_update_shape(&request, false).is_err());
    }
}
