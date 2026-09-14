//! 域 D08 `customer` 服务编排。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 创建客户（customer_account + 首条 OWNER 归属 + 审计）→ 跨集合，必须事务；
//! - 更新/删除客户 → 单集合 + 审计；
//! - 归属变更（结束旧归属 + 建立新归属）→ 跨行跨集合，必须事务。
//!
//! 跨域事实只走消费方 Port：
//! - D07 `party`：创建客户前校验主体存在、详情补充主体编号/当前法定名称；
//! - D06 `access_control`：负责销售账号存在性校验。
//!
//! 组合层持有根事务时必须调用显式 `executor` 的事务内接口，不得再开事务。

use std::collections::HashMap;
use std::sync::Arc;

use crate::dto::customer::{CreateCustomerRequest, CustomerScope, CustomerView, UpdateCustomerRequest};
use crate::entity::customer::{
    AssignmentRole, CustomerAccount, CustomerAccountData, CustomerAccountId, CustomerAccountStatus,
    CustomerAccountUpdate, CustomerAssignment, CustomerAssignmentData, CustomerAssignmentId,
};
use crate::error::{Error, Result};
use crate::ports::{
    AccountFactPort, CustomerAuditPort, CustomerDataScopePort, PartyFactPort, PartyIdentityFact,
};
use crate::repository::{CustomerAccountRow, CustomerExt};
use erp_core::common::time::BusinessDate;
use erp_core::field_update::FieldUpdate;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction, Transactional};
use validator::Validate;

use application_core::AuditActor;

pub mod access;
pub mod assignment;
pub mod profile;
mod query;
mod scope;

pub use access::CustomerAccess;
pub use assignment::CustomerAssignmentService;
pub use scope::CustomerListView;

/// 客户服务。
///
/// 提供客户角色与归属的创建、查询与更新编排（§6.2：一个 party 最多一个
/// 有效客户角色）。
pub struct CustomerService {
    db: Database,
    audit: Arc<dyn CustomerAuditPort>,
    party: Arc<dyn PartyFactPort>,
    accounts: Arc<dyn AccountFactPort>,
    data_scope: Arc<dyn CustomerDataScopePort>,
}

impl CustomerService {
    /// 创建客户服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    /// * `audit` - 审计写入端口
    /// * `party` - 主体事实端口
    /// * `accounts` - 账号事实端口
    /// * `data_scope` - 客户范围授权端口
    ///
    /// # 返回
    /// 返回服务实例。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 范围解析必须走注入的 Port；未接线端口失败关闭，不得补公司范围。
    pub fn new(
        db: Database,
        audit: Arc<dyn CustomerAuditPort>,
        party: Arc<dyn PartyFactPort>,
        accounts: Arc<dyn AccountFactPort>,
        data_scope: Arc<dyn CustomerDataScopePort>,
    ) -> Self {
        Self {
            db,
            audit,
            party,
            accounts,
            data_scope,
        }
    }

    /// 构造复用本服务授权 Port 的客户访问器。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回绑定当前数据库与范围 Port 的访问器。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 不得在此回退构造身份域 Service。
    fn access(&self) -> access::CustomerAccess {
        access::CustomerAccess::new(self.db.clone(), Arc::clone(&self.data_scope))
    }

    /// 创建客户（跨集合事务：customer_account + 首条 OWNER 归属 + 审计原子写入）。
    ///
    /// 前置校验：主体必须存在、创建人账号必须存在且启用、该主体不得已有客户
    /// 角色（§6.2：一个 party 最多一个有效客户角色，唯一索引兜底）。同一事务
    /// 建立首条 `OWNER` 归属，负责销售固定为创建人。
    ///
    /// # 参数
    /// * `req` - 创建请求；`owner_user_id` 即使提交也会被忽略
    /// * `actor` - 已通过鉴权的审计操作人；其账号 ID 写入首条 OWNER 归属
    ///
    /// # 返回
    /// 返回新建客户角色的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 主体或创建人账号不存在
    /// * `ConflictError` - 客户编号重复或该主体已有客户角色（唯一索引透出）
    /// * `ValidationError` - 请求体校验失败
    pub async fn create_customer(
        &self,
        req: CreateCustomerRequest,
        actor: &AuditActor,
    ) -> Result<CustomerView> {
        req.validate()?;
        self.party.ensure_exists(&req.party_id).await?;
        let owner_user_id = actor.id().to_string();
        self.accounts.ensure_can_login(&owner_user_id).await?;
        let (account, assignment) = prepare_new_customer(&req, actor.id())?;
        let audit = self.audit.resource_log(
            actor.clone(),
            "customer.create",
            "customer",
            account.base.id.clone(),
        )?;

        let db = self.db.clone();
        let client = db.client().clone();
        let audit_port = self.audit.clone();
        let account_for_tx = account.clone();
        let assignment_for_tx = assignment.clone();
        let access = self.access();
        let actor_for_tx = actor.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    access.require_create(&actor_for_tx, session).await?;
                    persist_new_account(&db, &account_for_tx, &assignment_for_tx, session).await?;
                    audit_port.persist(&audit, session).await?;
                    Ok::<(), crate::error::Error>(())
                })
            })
            .await?;

        Ok(account.into())
    }

    /// 在调用方 Executor 上写入客户角色与首条 OWNER 归属。
    ///
    /// 组合层持有根事务时必须调用本方法，不得再开事务。
    ///
    /// # 参数
    /// * `account` - 已构造的客户角色
    /// * `assignment` - 已构造的首条 OWNER 归属
    /// * `executor` - 调用方执行器
    ///
    /// # 错误
    /// 唯一索引冲突或底层写入失败。
    pub async fn persist_new_account(
        &self,
        account: &CustomerAccount,
        assignment: &CustomerAssignment,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        persist_new_account(&self.db, account, assignment, executor).await
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
        Ok(self
            .db
            .customer_assignments()
            .has_active_assignment_for_customer_user(
                customer_id,
                user_id,
                BusinessDate::today(),
                &mut NoTransaction,
            )
            .await?)
    }

    /// 按服务端数据范围解析允许返回的客户 ID。
    ///
    /// # 参数
    /// `scope` 必须由入口授权，`actor_user_id` 为当前登录用户。
    /// # 返回
    /// 全量权限返回 None；受限范围返回当前有效归属集合（可为空）。
    /// # 错误
    /// 查询失败时返回仓储错误。
    pub async fn customer_ids_for_scope(
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
        account
            .ensure_version(req.version)
            .map_err(|error| Error::ConflictError(error.to_string()))?;
        account.update(
            CustomerAccountUpdate {
                default_payment_term_id: FieldUpdate::from_optional_text(req.default_payment_term_id),
                status: req.status,
            },
            actor.id(),
        )?;
        let audit = self.audit.resource_log(
            actor.clone(),
            "customer.update",
            "customer",
            account.base.id.clone(),
        )?;

        let db = self.db.clone();
        let client = db.client().clone();
        let audit_port = self.audit.clone();
        let mut account_for_tx = account.clone();
        let access = self.access();
        let actor_for_tx = actor.clone();
        let customer_id = id.to_string();
        let updated = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    access
                        .require_with(actor_for_tx, "update", &customer_id, session)
                        .await?;
                    persist_account_update(&db, &mut account_for_tx, session).await?;
                    audit_port.persist(&audit, session).await?;
                    Ok::<CustomerAccount, crate::error::Error>(account_for_tx)
                })
            })
            .await?;

        Ok(updated.into())
    }

    /// 在调用方 Executor 上更新客户角色。
    ///
    /// # 参数
    /// * `account` - 已应用领域更新的客户角色
    /// * `executor` - 调用方执行器
    ///
    /// # 错误
    /// 版本冲突或底层写入失败。
    pub async fn persist_account_update(
        &self,
        account: &mut CustomerAccount,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        persist_account_update(&self.db, account, executor).await
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
    pub async fn load_customer(&self, id: &str) -> Result<CustomerAccount> {
        self.db
            .customer_accounts()
            .find_customer(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("客户不存在".to_string()))
    }

    /// 查询客户当前生效的负责销售（§6.2：同一时点恰好一个 OWNER）。
    ///
    /// # 参数
    /// * `customer_id` - 客户角色 ID
    ///
    /// # 返回
    /// 返回当前生效 OWNER 的销售人员；无生效归属时返回 `None`。
    async fn current_owner_user_id(&self, customer_id: &str) -> Result<Option<String>> {
        Ok(self
            .db
            .customer_assignments()
            .find_current_owner(
                &CustomerAccountId::new(customer_id),
                BusinessDate::today(),
                &mut NoTransaction,
            )
            .await?
            .map(|assignment| assignment.user_id))
    }
}

/// 构造客户角色与首条 OWNER 归属；忽略请求中的 `owner_user_id`。
fn prepare_new_customer(
    req: &CreateCustomerRequest,
    actor_id: &str,
) -> Result<(CustomerAccount, CustomerAssignment)> {
    let account = CustomerAccount::new(
        CustomerAccountId::new(next_id()),
        CustomerAccountData {
            party_id: req.party_id.clone(),
            customer_no: req.customer_no.clone(),
            default_payment_term_id: req.default_payment_term_id.clone(),
            status: req.status.unwrap_or(CustomerAccountStatus::Active),
        },
        actor_id,
    )?;
    let assignment = CustomerAssignment::new(
        CustomerAssignmentId::new(next_id()),
        CustomerAssignmentData {
            customer_id: CustomerAccountId::new(account.base.id.clone()),
            user_id: actor_id.to_string(),
            assignment_role: AssignmentRole::Owner,
            valid_from: req.valid_from,
            valid_to: req.valid_to,
            change_reason: req.change_reason.clone(),
        },
    )?;
    Ok((account, assignment))
}

/// 在同一 Executor 上写入客户角色与首条 OWNER 归属。
async fn persist_new_account(
    db: &Database,
    account: &CustomerAccount,
    assignment: &CustomerAssignment,
    executor: &mut dyn Executor,
) -> Result<()> {
    db.customer_accounts().create(account, executor).await?;
    db.customer_assignments().create(assignment, executor).await?;
    Ok(())
}

/// 在同一 Executor 上更新客户角色。
async fn persist_account_update(
    db: &Database,
    account: &mut CustomerAccount,
    executor: &mut dyn Executor,
) -> Result<()> {
    db.customer_accounts().update(account, executor).await?;
    Ok(())
}

/// 将批量读取结果按稳定 ID 装配为客户列表视图。
pub(super) fn assemble_customer_views(
    rows: Vec<CustomerAccountRow>,
    identities: Vec<PartyIdentityFact>,
    assignments: Vec<CustomerAssignment>,
    actor_user_id: &str,
    requested_scope: CustomerScope,
    account_names: HashMap<String, String>,
) -> Vec<CustomerView> {
    let identities: HashMap<String, PartyIdentityFact> = identities
        .into_iter()
        .map(|fact| (fact.party_id.clone(), fact))
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
            let identity = identities.get(&row.party_id);
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
                party_no: identity.map(|fact| fact.party_no.clone()),
                legal_name: identity.and_then(|fact| fact.legal_name.clone()),
                short_name: identity.and_then(|fact| fact.short_name.clone()),
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

#[cfg(test)]
mod tests {
    use super::{assemble_customer_views, prepare_new_customer};
    use crate::dto::customer::{customer_status_blockers, CreateCustomerRequest, CustomerScope};
    use crate::entity::customer::{AssignmentRole, CustomerAccountStatus, CustomerAssignment};
    use crate::ports::PartyIdentityFact;
    use crate::repository::CustomerAccountRow;
    use erp_core::common::time::BusinessDate;
    use erp_core::ids::PartyId;
    use serde_json::json;

    #[test]
    fn create_customer_ignores_submitted_owner_and_uses_actor() {
        let req: CreateCustomerRequest = serde_json::from_value(json!({
            "party_id": "party-1",
            "customer_no": "C-2026-001",
            "owner_user_id": "someone-else",
            "valid_from": "2026-01-01",
            "change_reason": "首次建档",
        }))
        .unwrap();
        let (account, assignment) = prepare_new_customer(&req, "actor-1").unwrap();
        assert_eq!(account.party_id, PartyId::new("party-1"));
        assert_eq!(assignment.user_id, "actor-1");
        assert_eq!(assignment.assignment_role, AssignmentRole::Owner);
        assert_eq!(assignment.customer_id.to_string(), account.base.id);
    }

    #[test]
    fn disabled_customer_blocks_new_business_actions() {
        assert!(customer_status_blockers(CustomerAccountStatus::Active).is_empty());
        let blockers = customer_status_blockers(CustomerAccountStatus::Disabled);
        assert_eq!(blockers.len(), 2);
        assert!(blockers.iter().all(|item| item.code == "CUSTOMER_DISABLED"));
    }

    #[test]
    fn assemble_views_hydrates_party_and_scope_tags() {
        let row = CustomerAccountRow {
            id: "customer-1".to_string(),
            party_id: "party-1".to_string(),
            customer_no: "C-1".to_string(),
            default_payment_term_id: None,
            status: CustomerAccountStatus::Active,
            version: 1,
            created_at: 1,
            updated_at: 1,
        };
        let identity = PartyIdentityFact {
            party_id: "party-1".to_string(),
            party_no: "P-1".to_string(),
            legal_name: Some("示例".to_string()),
            short_name: Some("示".to_string()),
        };
        let assignment = CustomerAssignment::new(
            crate::entity::customer::CustomerAssignmentId::new("asg-1"),
            crate::entity::customer::CustomerAssignmentData {
                customer_id: crate::entity::customer::CustomerAccountId::new("customer-1"),
                user_id: "actor-1".to_string(),
                assignment_role: AssignmentRole::Owner,
                valid_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
                valid_to: None,
                change_reason: "首次指派".to_string(),
            },
        )
        .unwrap();
        let views = assemble_customer_views(
            vec![row],
            vec![identity],
            vec![assignment],
            "actor-1",
            CustomerScope::Mine,
            [("actor-1".to_string(), "张三".to_string())]
                .into_iter()
                .collect(),
        );
        assert_eq!(views[0].party_no.as_deref(), Some("P-1"));
        assert_eq!(views[0].legal_name.as_deref(), Some("示例"));
        assert_eq!(views[0].owner_user_name.as_deref(), Some("张三"));
        assert!(views[0].scope_tags.contains(&CustomerScope::Mine));
    }
}
