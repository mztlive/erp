//! 域 D08 `customer` 服务编排。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 创建客户（customer_account + 首条 OWNER 归属 + 审计）→ 跨集合，必须事务；
//! - 更新/删除客户 → 单集合 + 审计（`&mut NoTransaction`，与 source_registry 同款）；
//! - 归属变更（结束旧归属 + 建立新归属）→ 跨行跨集合，必须事务。
//!
//! 跨域（只走对方 Repository，禁止 Service 依赖 Service）：
//! - D07 `party`：创建客户前校验主体存在、详情补充主体编号/当前法定名称；
//! - D06 `access_control`：负责销售账号存在性校验（`AccessControlExt::accounts`）。

use std::collections::HashMap;

use database::{AccessControlExt, CustomerExt, NoTransaction, PartyExt, Transactional};
use entities::common::time::BusinessDate;
use entities::customer::{
    AssignmentRole, CustomerAccount, CustomerAccountData, CustomerAccountId, CustomerAccountStatus,
    CustomerAccountUpdate, CustomerAssignment, CustomerAssignmentData, CustomerAssignmentId,
};
use entities::field_update::FieldUpdate;
use entities::ids::PartyId;
use id_generator::next_id;
use mongodb::bson::doc;
use mongodb::Database;
use validator::Validate;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};

pub mod assignment;
mod dto;
pub mod profile;

pub use self::dto::{
    AssignmentAction, CreateCustomerRequest, CustomerActionBlockerView, CustomerAssignmentListParams,
    CustomerAssignmentRequest, CustomerAssignmentView, CustomerDetailView, CustomerListParams,
    CustomerProfileAddressInput, CustomerProfileBankAccountInput, CustomerProfileContactInput,
    CustomerProfileDetailView, CustomerProfileMutationView, CustomerScope, CustomerSensitiveFieldView,
    CustomerSensitiveRevealView, CustomerView, PageView, RevealCustomerSensitiveRequest,
    SaveCustomerProfileRequest, UpdateCustomerRequest,
};

use self::dto::SortDir;

/// 客户角色列表筛选条件类型（经 `CustomerExt` 关联类型跨 crate 可达）。
type CustomerAccountFilter = <mongodb::Database as database::CustomerExt>::CustomerAccountFilter;

/// 客户服务。
///
/// 提供客户角色与归属的创建、查询与更新编排（§6.2：一个 party 最多一个
/// 有效客户角色）。
pub struct CustomerService {
    db: Database,
}

impl CustomerService {
    /// 创建客户服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 创建客户（跨集合事务：customer_account + 首条 OWNER 归属 + 审计原子写入）。
    ///
    /// 前置校验：主体必须存在（D07 仓储读）、负责销售账号必须存在（D06 仓储读）、
    /// 该主体不得已有客户角色（§6.2：一个 party 最多一个有效客户角色，唯一
    /// 索引兜底）。同一事务建立首条 `OWNER` 归属（W03：新建客户必带负责销售）。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建客户角色的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 主体或负责销售账号不存在
    /// * `ConflictError` - 客户编号重复或该主体已有客户角色（唯一索引透出）
    /// * `ValidationError` - 请求体校验失败
    pub async fn create_customer(
        &self,
        req: CreateCustomerRequest,
        actor: &AuditActor,
    ) -> Result<CustomerView> {
        req.validate()?;
        self.ensure_party_exists(&req.party_id).await?;
        self.ensure_user_exists(&req.owner_user_id).await?;

        let account = CustomerAccount::new(
            CustomerAccountId::new(next_id()),
            CustomerAccountData {
                party_id: req.party_id.clone(),
                customer_no: req.customer_no,
                default_payment_term_id: req.default_payment_term_id,
                status: req.status.unwrap_or(CustomerAccountStatus::Active),
            },
            actor.id(),
        )?;
        let assignment = CustomerAssignment::new(
            CustomerAssignmentId::new(next_id()),
            CustomerAssignmentData {
                customer_id: CustomerAccountId::new(account.base.id.clone()),
                user_id: req.owner_user_id,
                assignment_role: AssignmentRole::Owner,
                valid_from: req.valid_from,
                valid_to: req.valid_to,
                change_reason: req.change_reason,
            },
        )?;
        let audit = actor
            .clone()
            .resource_log("customer.create", "customer", account.base.id.clone())?;

        let db = self.db.clone();
        let client = db.client().clone();
        let account_for_tx = account.clone();
        let assignment_for_tx = assignment.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.customer_accounts().create(&account_for_tx, session).await?;
                    db.customer_assignments()
                        .create(&assignment_for_tx, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(account.into())
    }

    /// 分页查询客户角色列表。
    ///
    /// 排序字段白名单在 Service 层校验（api-contract §4），禁止任意字段透传。
    ///
    /// # 参数
    /// * `params` - 查询参数
    /// * `actor_user_id` - 当前登录用户 ID，用于执行客户归属范围过滤
    ///
    /// # 返回
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    pub async fn customer_list(
        &self,
        params: &CustomerListParams,
        actor_user_id: &str,
    ) -> Result<PageView<CustomerView>> {
        params.validate()?;
        let query = params.normalized()?;
        let customer_ids = self.customer_ids_for_scope(query.scope, actor_user_id).await?;
        let keyword_party_ids = match query.keyword.as_deref() {
            Some(keyword) => Some(self.matching_party_ids(keyword).await?),
            None => None,
        };
        let filter = CustomerAccountFilter {
            keyword: query.keyword,
            keyword_party_ids,
            party_id: query.party_id,
            party_ids: None,
            customer_ids,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .customer_accounts()
            .search_customer_accounts(&filter, &mut NoTransaction)
            .await?;
        let items = self
            .hydrate_customer_rows(page.items, actor_user_id, query.scope)
            .await?;

        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 判断当前用户是否在指定客户的当前 OWNER 或 COLLABORATOR 归属中。
    ///
    /// # 参数
    /// * `customer_id` - 客户角色 ID
    /// * `user_id` - 当前登录用户 ID
    ///
    /// # 返回
    /// 命中当前有效归属返回 `true`，否则返回 `false`。
    pub async fn customer_is_assigned_to(&self, customer_id: &str, user_id: &str) -> Result<bool> {
        let assignments = self
            .db
            .customer_assignments()
            .find_active_assignments_for_user(user_id, BusinessDate::today(), &mut NoTransaction)
            .await?;
        Ok(assignments
            .iter()
            .any(|assignment| assignment.customer_id.as_ref() == customer_id))
    }

    /// 按服务端数据范围解析允许返回的客户 ID。
    async fn customer_ids_for_scope(
        &self,
        scope: CustomerScope,
        actor_user_id: &str,
    ) -> Result<Option<Vec<String>>> {
        if scope == CustomerScope::AllAuthorized {
            return Ok(None);
        }
        let expected_role = match scope {
            CustomerScope::Mine => Some(AssignmentRole::Owner),
            CustomerScope::Collaborating => Some(AssignmentRole::Collaborator),
            CustomerScope::Assigned => None,
            CustomerScope::AllAuthorized => unreachable!(),
        };
        let assignments = self
            .db
            .customer_assignments()
            .find_active_assignments_for_user(actor_user_id, BusinessDate::today(), &mut NoTransaction)
            .await?;
        Ok(Some(
            assignments
                .into_iter()
                .filter(|assignment| expected_role.is_none_or(|role| assignment.assignment_role == role))
                .map(|assignment| assignment.customer_id.to_string())
                .collect(),
        ))
    }

    /// 查找法定名称或简称命中的主体 ID，供客户编号与名称统一关键词搜索。
    async fn matching_party_ids(&self, keyword: &str) -> Result<Vec<String>> {
        let escaped = regex::escape(keyword);
        let revisions = self
            .db
            .party_revisions()
            .find_many(
                doc! {
                    "$or": [
                        { "legal_name": { "$regex": &escaped, "$options": "i" } },
                        { "short_name": { "$regex": &escaped, "$options": "i" } },
                    ]
                },
                &mut NoTransaction,
            )
            .await?;
        let revision_ids: Vec<String> = revisions.into_iter().map(|revision| revision.base.id).collect();
        let mut ids: Vec<String> = self
            .db
            .parties()
            .find_many(
                doc! { "current_revision_id": { "$in": revision_ids } },
                &mut NoTransaction,
            )
            .await?
            .into_iter()
            .map(|party| party.base.id)
            .collect();
        ids.sort();
        ids.dedup();
        Ok(ids)
    }

    /// 批量补齐客户当前主体身份与归属，避免列表逐行查询。
    async fn hydrate_customer_rows(
        &self,
        rows: Vec<database::repository::CustomerAccountRow>,
        actor_user_id: &str,
        requested_scope: CustomerScope,
    ) -> Result<Vec<CustomerView>> {
        if rows.is_empty() {
            return Ok(Vec::new());
        }
        let party_ids: Vec<String> = rows.iter().map(|row| row.party_id.clone()).collect();
        let parties = self
            .db
            .parties()
            .find_many(doc! { "id": { "$in": party_ids } }, &mut NoTransaction)
            .await?;
        let revision_ids: Vec<String> = parties
            .iter()
            .filter_map(|party| party.stable.current_revision_id.clone())
            .collect();
        let revisions = self
            .db
            .party_revisions()
            .find_many(doc! { "id": { "$in": revision_ids } }, &mut NoTransaction)
            .await?;
        let customer_ids: Vec<String> = rows.iter().map(|row| row.id.clone()).collect();
        let today = BusinessDate::today().to_string();
        let assignments = self
            .db
            .customer_assignments()
            .find_many(
                doc! {
                    "customer_id": { "$in": customer_ids },
                    "valid_from": { "$lte": &today },
                    "$or": [
                        { "valid_to": null },
                        { "valid_to": { "$gt": &today } },
                    ],
                },
                &mut NoTransaction,
            )
            .await?;
        let account_ids: Vec<String> = assignments
            .iter()
            .map(|assignment| assignment.user_id.clone())
            .collect();
        let account_names: HashMap<String, String> = self
            .db
            .accounts()
            .find_many(doc! { "id": { "$in": account_ids } }, &mut NoTransaction)
            .await?
            .into_iter()
            .map(|account| (account.base.id, account.name))
            .collect();
        Ok(assemble_customer_views(
            rows,
            parties,
            revisions,
            assignments,
            actor_user_id,
            requested_scope,
            account_names,
        ))
    }

    /// 查询客户角色详情（客户 + 主体身份 + 当前生效 OWNER）。
    ///
    /// # 参数
    /// * `id` - 客户角色 ID
    ///
    /// # 返回
    /// 返回客户详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 客户角色不存在
    pub async fn customer_detail(&self, id: &str) -> Result<CustomerDetailView> {
        let account = self.load_customer(id).await?;
        let (party_no, legal_name) = self.party_identity(&account.party_id).await;
        let owner_user_id = self.current_owner_user_id(&account.base.id).await?;
        Ok(CustomerDetailView {
            account: account.into(),
            party_no,
            legal_name,
            owner_user_id,
        })
    }

    /// 更新客户角色（乐观锁；单集合 + 审计）。
    ///
    /// # 参数
    /// * `id` - 客户角色 ID
    /// * `req` - 更新请求（含期望版本）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回更新后客户角色的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 客户角色不存在
    /// * `ConflictError` - 期望版本与当前版本不一致
    pub async fn update_customer(
        &self,
        id: &str,
        req: UpdateCustomerRequest,
        actor: &AuditActor,
    ) -> Result<CustomerView> {
        req.validate()?;
        let mut account = self.load_customer(id).await?;
        if account.base.version != req.version {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        account.update(
            CustomerAccountUpdate {
                default_payment_term_id: payment_term_update(req.default_payment_term_id),
                status: req.status,
            },
            actor.id(),
        )?;
        let audit = actor
            .clone()
            .resource_log("customer.update", "customer", account.base.id.clone())?;

        let db = self.db.clone();
        let client = db.client().clone();
        let mut account_for_tx = account.clone();
        let updated = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.customer_accounts()
                        .update(&mut account_for_tx, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<CustomerAccount, crate::errors::Error>(account_for_tx)
                })
            })
            .await?;

        Ok(updated.into())
    }

    /// 软删除客户角色（单集合操作，无事务）。
    ///
    /// 停用/删除角色仍可被历史单据引用（§6.2），不删除主体与归属历史。
    ///
    /// # 参数
    /// * `id` - 客户角色 ID
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 删除成功返回 `Ok(())`。
    ///
    /// # 错误
    /// * `NotFound` - 客户角色不存在
    pub async fn delete_customer(&self, id: &str, actor: &AuditActor) -> Result<()> {
        let mut account = self.load_customer(id).await?;
        let audit = actor
            .clone()
            .resource_log("customer.delete", "customer", account.base.id.clone())?;
        self.db
            .customer_accounts()
            .soft_delete(&mut account, &mut NoTransaction)
            .await?;
        self.db.audit_logs().create(&audit, &mut NoTransaction).await?;
        Ok(())
    }

    /// 按 ID 加载未删除客户角色。
    ///
    /// # 参数
    /// * `id` - 客户角色 ID
    ///
    /// # 返回
    /// 返回客户角色实体。
    ///
    /// # 错误
    /// * `NotFound` - 客户角色不存在
    async fn load_customer(&self, id: &str) -> Result<CustomerAccount> {
        self.db
            .customer_accounts()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("客户不存在".to_string()))
    }

    /// 校验主体存在（D07 跨域读）。
    ///
    /// # 参数
    /// * `party_id` - 主体 ID
    ///
    /// # 返回
    /// 主体存在返回 `Ok(())`。
    ///
    /// # 错误
    /// * `NotFound` - 主体不存在
    async fn ensure_party_exists(&self, party_id: &PartyId) -> Result<()> {
        self.db
            .parties()
            .find_by_id(party_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("企业主体不存在，请先创建主体".to_string()))?;
        Ok(())
    }

    /// 校验销售人员账号存在（D06 跨域读）。
    ///
    /// # 参数
    /// * `user_id` - 账号 ID
    ///
    /// # 返回
    /// 账号存在返回 `Ok(())`。
    ///
    /// # 错误
    /// * `NotFound` - 账号不存在
    /// * `BusinessLogicError` - 账号已停用
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

    /// 查询主体编号与当前法定名称（D07 跨域读；缺失时静默降级为 `None`）。
    ///
    /// # 参数
    /// * `party_id` - 主体 ID
    ///
    /// # 返回
    /// 返回 `(party_no, legal_name)` 元组。
    async fn party_identity(&self, party_id: &PartyId) -> (Option<String>, Option<String>) {
        let Ok(Some(party)) = self.db.parties().find_by_id(party_id, &mut NoTransaction).await else {
            return (None, None);
        };
        let legal_name = match &party.stable.current_revision_id {
            Some(revision_id) => self
                .db
                .party_revisions()
                .find_by_id(revision_id, &mut NoTransaction)
                .await
                .ok()
                .flatten()
                .map(|revision| revision.legal_name),
            None => None,
        };
        (Some(party.party_no), legal_name)
    }

    /// 查询客户当前生效的负责销售（§6.2：同一时点恰好一个 OWNER）。
    ///
    /// # 参数
    /// * `customer_id` - 客户角色 ID
    ///
    /// # 返回
    /// 返回当前生效 OWNER 的销售人员；无生效归属时返回 `None`。
    async fn current_owner_user_id(&self, customer_id: &str) -> Result<Option<String>> {
        let today = BusinessDate::today();
        let today_str = today.to_string();
        let assignments = self
            .db
            .customer_assignments()
            .find_many(
                doc! {
                    "customer_id": customer_id,
                    "assignment_role": AssignmentRole::Owner.as_str(),
                    "valid_from": { "$lte": &today_str },
                    "$or": [
                        { "valid_to": null },
                        { "valid_to": { "$gt": &today_str } },
                    ],
                },
                &mut NoTransaction,
            )
            .await?;
        Ok(assignments.first().map(|assignment| assignment.user_id.clone()))
    }
}

/// 将批量读取结果按稳定 ID 装配为客户列表视图。
fn assemble_customer_views(
    rows: Vec<database::repository::CustomerAccountRow>,
    parties: Vec<entities::party::Party>,
    revisions: Vec<entities::party::PartyRevision>,
    assignments: Vec<CustomerAssignment>,
    actor_user_id: &str,
    requested_scope: CustomerScope,
    account_names: HashMap<String, String>,
) -> Vec<CustomerView> {
    let parties: HashMap<String, entities::party::Party> = parties
        .into_iter()
        .map(|party| (party.base.id.clone(), party))
        .collect();
    let revisions: HashMap<String, entities::party::PartyRevision> = revisions
        .into_iter()
        .map(|revision| (revision.base.id.clone(), revision))
        .collect();
    let mut assignments_by_customer: HashMap<String, Vec<CustomerAssignment>> = HashMap::new();
    for assignment in assignments {
        assignments_by_customer
            .entry(assignment.customer_id.to_string())
            .or_default()
            .push(assignment);
    }

    rows.into_iter()
        .map(|row| {
            let party = parties.get(&row.party_id);
            let revision = party
                .and_then(|party| party.stable.current_revision_id.as_ref())
                .and_then(|id| revisions.get(id));
            let assignments = assignments_by_customer
                .get(&row.id)
                .map(Vec::as_slice)
                .unwrap_or_default();
            let owner_user_id = assignments
                .iter()
                .find(|assignment| assignment.assignment_role == AssignmentRole::Owner)
                .map(|assignment| assignment.user_id.clone());
            let owner_user_name = owner_user_id
                .as_ref()
                .and_then(|id| account_names.get(id))
                .cloned();
            let collaborator_count = assignments
                .iter()
                .filter(|assignment| assignment.assignment_role == AssignmentRole::Collaborator)
                .count() as u32;
            let mut scope_tags = Vec::new();
            if owner_user_id.as_deref() == Some(actor_user_id) {
                scope_tags.push(CustomerScope::Mine);
            }
            if assignments.iter().any(|assignment| {
                assignment.assignment_role == AssignmentRole::Collaborator
                    && assignment.user_id == actor_user_id
            }) {
                scope_tags.push(CustomerScope::Collaborating);
            }
            if !scope_tags.contains(&requested_scope) {
                scope_tags.push(requested_scope);
            }
            CustomerView {
                id: row.id,
                party_id: row.party_id,
                party_no: party.map(|party| party.party_no.clone()),
                legal_name: revision.map(|revision| revision.legal_name.clone()),
                short_name: revision.and_then(|revision| revision.short_name.clone()),
                customer_no: row.customer_no,
                default_payment_term_id: row.default_payment_term_id,
                status: row.status,
                owner_user_id,
                owner_user_name,
                collaborator_count,
                scope_tags,
                version: row.version,
                created_at: row.created_at,
                updated_at: row.updated_at,
            }
        })
        .collect()
}

/// 分页默认值辅助（与 `crate::query` 对齐，供子模块复用）。
fn page_or_default(page: Option<u64>) -> u64 {
    page.unwrap_or(1)
}

/// 分页大小默认值辅助（与 `crate::query` 对齐，供子模块复用）。
fn page_size_or_default(page_size: Option<u32>) -> u32 {
    page_size.unwrap_or(20).clamp(1, 100)
}

/// 将可选付款条件入参映射为 `FieldUpdate`：`None` 表示不修改，空字符串表示清除。
///
/// # 参数
/// * `value` - 请求携带的付款条件引用
///
/// # 返回
/// 返回实体更新意图。
fn payment_term_update(value: Option<String>) -> FieldUpdate<String> {
    match value {
        Some(term) if term.trim().is_empty() => FieldUpdate::Clear,
        Some(term) => FieldUpdate::Set(term),
        None => FieldUpdate::Unchanged,
    }
}
