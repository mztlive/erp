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

pub use self::dto::{
    AssignmentAction, CreateCustomerRequest, CustomerAssignmentListParams, CustomerAssignmentRequest,
    CustomerAssignmentView, CustomerDetailView, CustomerListParams, CustomerView, PageView,
    UpdateCustomerRequest,
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
    ///
    /// # 返回
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    pub async fn customer_list(&self, params: &CustomerListParams) -> Result<PageView<CustomerView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = CustomerAccountFilter {
            keyword: query.keyword,
            party_id: query.party_id,
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
        let items = page
            .items
            .into_iter()
            .map(|row| CustomerView {
                id: row.id,
                party_id: row.party_id,
                customer_no: row.customer_no,
                default_payment_term_id: row.default_payment_term_id,
                status: row.status,
                version: row.version,
                created_at: row.created_at,
            })
            .collect();

        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
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
    async fn ensure_user_exists(&self, user_id: &str) -> Result<()> {
        self.db
            .accounts()
            .find_by_id(user_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("负责销售账号不存在".to_string()))?;
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
