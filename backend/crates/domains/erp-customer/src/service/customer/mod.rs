//! 域 D08 `customer` 服务编排。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 创建客户（customer_account + 首条 OWNER 归属 + 审计）→ 跨集合，必须事务；
//! - 更新/删除客户 → 单集合 + 审计；
//! - 归属变更（结束旧归属 + 建立新归属）→ 跨行跨集合，必须事务。
//!
//! 跨域事实只走消费方 Port：
//! - D07 `party`：创建客户前校验主体存在、详情补充主体编号/当前法定名称；
//!
//! 账号事实与姓名直接使用 identity 的公开账号目录。
//!
//! 组合层持有根事务时必须调用显式 `executor` 的事务内接口，不得再开事务。

use std::collections::HashMap;
use std::sync::Arc;

use application_core::AuditActor;
use erp_core::field_update::FieldUpdate;
use erp_identity::AccountDirectoryService;
use erp_permissions::customer as customer_permissions;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use crate::dto::customer::{
    CreateCustomerRequest, CustomerOwnerView, CustomerScope, CustomerView, UpdateCustomerRequest,
};
use crate::entity::customer::{
    AssignmentRole, CustomerAccount, CustomerAccountData, CustomerAccountId, CustomerAccountStatus,
    CustomerAccountUpdate, CustomerAssignment, CustomerAssignmentData, CustomerAssignmentId,
};
use crate::error::{Error, Result};
use crate::ports::{CustomerAuditPort, PartyFactPort, PartyIdentityFact};
use crate::repository::prelude::*;
use crate::repository::{CustomerAccountRow, CustomerExt};

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
#[derive(Clone)]
pub struct CustomerService {
    db: Database,
    audit: Arc<dyn CustomerAuditPort>,
    party: Arc<dyn PartyFactPort>,
    accounts: AccountDirectoryService,
    access: CustomerAccess,
}

impl CustomerService {
    /// 装配客户账户服务的仓储、授权以及审计，并接入主体事实端口，供客户建档和资料查询使用。
    ///
    /// # 参数
    /// * `audit` - 审计写入端口
    /// * `party` - 主体事实端口
    /// * `access` - 客户统一授权访问器
    ///
    /// 范围解析必须由 identity 完整授权，不得补公司范围。
    pub fn new(
        db: Database,
        audit: Arc<dyn CustomerAuditPort>,
        party: Arc<dyn PartyFactPort>,
        access: CustomerAccess,
    ) -> Self {
        Self { accounts: AccountDirectoryService::new(db.clone()), db, audit, party, access }
    }

    /// 为往来主体建立客户账户，并以当前操作人作为首位负责人。客户账户、首条归属和审计一起提交，避免留下没有负责人的客户。
    ///
    /// 前置校验：主体必须存在、创建人账号必须存在且启用、该主体不得已有客户
    /// 角色（§6.2：一个 party 最多一个有效客户角色，唯一索引兜底）。同一事务
    /// 建立首条 `OWNER` 归属，负责销售固定为创建人。
    ///
    /// # 参数
    /// * `req` - 创建请求；`owner_user_id` 即使提交也会被忽略
    /// * `actor` - 已通过鉴权的审计操作人；其账号 ID 写入首条 OWNER 归属
    ///
    /// 返回新建客户角色的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 主体或创建人账号不存在
    /// * `BusinessLogicError` - 创建人账号不能登录
    /// * `ConflictError` - 客户编号重复或该主体已有客户角色（唯一索引透出）
    /// * `ValidationError` - 请求体校验失败
    ///
    /// 编号、窗口或归属字段非法时返回领域错误；范围校验、审计或事务写入失败时返回对应错误。
    pub async fn create_customer(
        &self,
        req: CreateCustomerRequest,
        actor: &AuditActor,
    ) -> Result<CustomerView> {
        req.validate()?;
        let (account, assignment) = prepare_new_customer(&req, actor.id())?;

        let service = self.clone();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { service.create_stage(account, assignment, &actor, executor).await })
            })
            .await
    }

    // 在建档事务里核对主体、操作账号与拟生效归属，资格成立后才一起保存账户、首位负责人和审计。
    async fn create_stage(
        &self,
        account: CustomerAccount,
        assignment: CustomerAssignment,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<CustomerView> {
        self.access.require_candidate(actor, &account.base.id, executor).await?;

        self.accounts
            .account(actor.id(), executor)
            .await?
            .ok_or_else(|| Error::NotFound("负责销售账号不存在".into()))?
            .ensure_can_login()
            .map_err(|error| Error::BusinessLogicError(error.to_string()))?;
        if !self
            .party
            .identities_by_ids(std::slice::from_ref(&account.party_id), executor)
            .await?
            .iter()
            .any(|identity| identity.party_id == account.party_id.as_ref())
        {
            return Err(Error::NotFound("主体不存在".into()));
        }

        let audit =
            self.audit.resource_log(actor.clone(), "customer.create", "customer", account.base.id.clone())?;
        self.persist_new_account(&account, &assignment, executor).await?;
        self.audit.persist(&audit, executor).await?;
        Ok(account.into())
    }

    /// 在上层用例的事务中写入客户账户及首位负责人，使账户和归属对后续读取同时可见。
    ///
    /// 组合层持有根事务时必须调用本方法，不得再开事务。
    ///
    /// # 参数
    /// * `account` - 已构造的客户角色
    /// * `assignment` - 已构造的首条 OWNER 归属
    ///
    /// 客户角色与首条 OWNER 归属都写入完成。
    ///
    /// 唯一索引冲突或底层写入失败时返回对应错误。
    pub async fn persist_new_account(
        &self,
        account: &CustomerAccount,
        assignment: &CustomerAssignment,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.db.customer_accounts().create(account, executor).await?;
        self.db.customer_assignments().create(assignment, executor).await?;
        Ok(())
    }

    /// 修改客户账户可编辑资料。用请求版本防止覆盖他人已提交的改动，并把审计与账户更新一并提交。
    ///
    /// # 参数
    /// * `id` - 客户角色 ID
    /// * `req` - 更新请求（含期望版本）
    ///
    /// 返回更新后客户角色的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 客户角色不存在
    /// * `ConflictError` - 期望版本与当前版本不一致
    ///
    /// 请求校验或状态迁移非法时返回校验或领域错误；范围校验、仓储更新或审计失败时返回对应错误。
    pub async fn update_customer(
        &self,
        id: &str,
        req: UpdateCustomerRequest,
        actor: &AuditActor,
    ) -> Result<CustomerView> {
        req.validate()?;
        let service = self.clone();
        let actor = actor.clone();
        let id = id.to_owned();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { service.update_stage(&id, req, &actor, executor).await })
            })
            .await
    }

    // 在更新事务里先校验目标客户的维护资格，再按版本更新账户并写审计。
    async fn update_stage(
        &self,
        id: &str,
        req: UpdateCustomerRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<CustomerView> {
        self.access.require_with(actor.clone(), &customer_permissions::UPDATE, id, executor).await?;

        let mut account = self
            .db
            .customer_accounts()
            .find_customer(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("客户不存在".into()))?;
        account.ensure_version(req.version).map_err(|error| Error::ConflictError(error.to_string()))?;

        account.update(
            CustomerAccountUpdate {
                default_payment_term_id: FieldUpdate::from_optional_text(req.default_payment_term_id),
                status: req.status,
            },
            actor.id(),
        )?;

        let audit = self.audit.resource_log(actor.clone(), "customer.update", "customer", id.to_owned())?;
        self.db.customer_accounts().update(&mut account, executor).await?;
        self.audit.persist(&audit, executor).await?;
        Ok(account.into())
    }
}

// 把建档数据和真实操作人转换成客户账户与首条负责人归属。负责人从服务端操作身份取得。
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

// 按客户和主体的稳定 ID 对齐批量查询结果，为目录行补全主体身份及负责人。
//
// 主体身份、负责人与目录标签按 ID 对齐；缺失的主体或归属不补造记录。
//
// # 参数
// * `rows` - 客户角色投影行，决定结果顺序
// * `identities` - 主体身份事实；同一 `party_id` 后者覆盖前者
// * `assignments` - 归属行，按客户分组
// * `actor_user_id` - 当前账号，用于目录标签
// * `requested_scope` - 页面请求的目录范围
// * `account_names` - 账号显示名
//
// 按 `rows` 顺序返回已填充主体、负责人和范围标签的客户视图。
pub(super) fn assemble_customer_views(
    rows: Vec<CustomerAccountRow>,
    identities: Vec<PartyIdentityFact>,
    assignments: Vec<CustomerAssignment>,
    actor_user_id: &str,
    requested_scope: CustomerScope,
    account_names: HashMap<String, String>,
) -> Vec<CustomerView> {
    let identities: HashMap<String, PartyIdentityFact> =
        identities.into_iter().map(|fact| (fact.party_id.clone(), fact)).collect();
    let mut assignments_by_customer: HashMap<String, Vec<CustomerAssignment>> = HashMap::new();
    for assignment in assignments {
        assignments_by_customer.entry(assignment.customer_id.to_string()).or_default().push(assignment);
    }

    rows.into_iter()
        .map(|row| {
            let identity = identities.get(&row.party_id);
            let assignments = assignments_by_customer.get(&row.id).map(Vec::as_slice).unwrap_or_default();
            let (owner_user_id, owner_user_name, collaborator_count) =
                CustomerAssignment::owner_summary(assignments, &account_names);
            let scope_tags = CustomerScope::tags_for(
                assignments,
                owner_user_id.as_deref(),
                actor_user_id,
                requested_scope,
            );

            let mut view = CustomerView::from_account_parts(
                row.id,
                row.party_id,
                row.customer_no,
                row.default_payment_term_id,
                row.status,
                row.version,
                row.created_at,
                row.updated_at,
            );
            view.party_no = identity.map(|fact| fact.party_no.clone());
            view.legal_name = identity.and_then(|fact| fact.legal_name.clone());
            view.short_name = identity.and_then(|fact| fact.short_name.clone());

            view.owners = CustomerAssignment::owner_ids(assignments)
                .into_iter()
                .map(|user_id| CustomerOwnerView { user_name: account_names.get(&user_id).cloned(), user_id })
                .collect();
            view.owner_user_id = owner_user_id;
            view.owner_user_name = owner_user_name;
            view.collaborator_count = collaborator_count;
            view.scope_tags = scope_tags;
            view
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use application_core::AuditActor;
    use erp_core::AccountKind;
    use erp_core::common::time::BusinessDate;
    use erp_core::ids::PartyId;
    use erp_identity::testing::unavailable_authorization;
    use persistence_core::testing::SnapshotExecutor;
    use serde_json::json;

    use super::{CustomerService, assemble_customer_views, prepare_new_customer};
    use crate::CustomerAccess;
    use crate::dto::customer::{CreateCustomerRequest, CustomerScope, customer_status_blockers};
    use crate::entity::customer::{AssignmentRole, CustomerAccountStatus, CustomerAssignment};
    use crate::error::Error;
    use crate::ports::{FailClosedAuditPort, FailClosedPartyFactPort, PartyIdentityFact};
    use crate::repository::CustomerAccountRow;

    #[tokio::test]
    async fn update_authorizes_before_customer_existence_and_version_reads() {
        let client =
            mongodb::Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1").await.unwrap();
        let db = client.database("customer_update_authorization");
        let service = CustomerService::new(
            db.clone(),
            Arc::new(FailClosedAuditPort),
            Arc::new(FailClosedPartyFactPort),
            CustomerAccess::new(db, unavailable_authorization()),
        );
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        for version in [1, 999] {
            let request = serde_json::from_value(json!({"version": version})).unwrap();
            let error = service
                .update_stage("unread-customer", request, &actor, &mut SnapshotExecutor::default())
                .await
                .expect_err("授权失败不得探测客户存在性或版本");
            assert!(matches!(error, Error::Internal(message) if message == "测试授权设施不可用"));
        }
    }

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
        let mut row = CustomerAccountRow::new("customer-1", "party-1", "C-1");
        row.version = 1;
        row.created_at = 1;
        row.updated_at = 1;
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
            [("actor-1".to_string(), "张三".to_string())].into_iter().collect(),
        );

        assert_eq!(views[0].party_no.as_deref(), Some("P-1"));
        assert_eq!(views[0].legal_name.as_deref(), Some("示例"));
        assert_eq!(views[0].owner_user_name.as_deref(), Some("张三"));
        assert!(views[0].scope_tags.contains(&CustomerScope::Mine));
    }
}
