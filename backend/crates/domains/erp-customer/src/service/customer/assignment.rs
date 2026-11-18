//! 域 D08 `customer_assignment` 服务编排。
//!
//! 归属变化按「分别新增或结束人员关系」维护（W03），不原地修改人员/角色；
//! 归属是新单据参与权的来源（§6.2）。Service 在事务内加载冲突候选并
//! 持久化，窗口、角色冲突与负责人结束限制由 `CustomerAssignment` 判定：
//! - 同一客户允许多名有效 `OWNER`；
//! - 同一客户、用户、角色的有效期不得重叠。

use std::sync::Arc;

use application_core::{AuditActor, normalize_sort, page_or_default, page_size_or_default};
use erp_core::ids::CustomerAccountId;
use erp_identity::{AccessTarget, AccountDirectoryService};
use erp_permissions::{customer as customer_permissions, customer};
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::access::CustomerAccess;
use crate::dto::customer::{
    CUSTOMER_ASSIGNMENT_SORT_FIELDS, CustomerAssignmentListParams, CustomerAssignmentRequest,
    CustomerAssignmentView, PageView, SortDir,
};
use crate::entity::customer::{
    AssignCustomerAssignment, AssignmentRole, CustomerAssignment, CustomerAssignmentCommand,
    CustomerAssignmentId, EndCustomerAssignment,
};
use crate::error::{Error, Result};
use crate::ports::CustomerAuditPort;
use crate::repository::prelude::*;
use crate::repository::{CustomerAssignmentFilter, CustomerAssignmentRow, CustomerExt};

/// 客户归属服务。
pub struct CustomerAssignmentService {
    db: Database,
    audit: Arc<dyn CustomerAuditPort>,
    accounts: AccountDirectoryService,
    access: CustomerAccess,
}

impl CustomerAssignmentService {
    /// 创建客户归属服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    /// * `audit` - 审计写入端口
    /// * `access` - 客户统一授权访问器
    ///
    /// # 返回
    /// 返回服务实例。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 归属变更必须通过注入的访问器 证明 customer:update 范围。
    pub fn new(db: Database, audit: Arc<dyn CustomerAuditPort>, access: CustomerAccess) -> Self {
        Self { accounts: AccountDirectoryService::new(db.clone()), db, audit, access }
    }

    /// 分页查询客户归属列表。
    ///
    /// # 参数
    /// * `customer_id` - 客户角色 ID
    /// * `params` - 查询参数
    ///
    /// # 返回
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    ///
    /// 归属查询失败时返回仓储错误。
    pub async fn customer_assignment_list(
        &self,
        actor: &AuditActor,
        customer_id: &str,
        params: &CustomerAssignmentListParams,
    ) -> Result<PageView<CustomerAssignmentView>> {
        let db = self.db.clone();
        let service = Self::new(db.clone(), self.audit.clone(), self.access.clone());
        let actor = actor.clone();
        let customer_id = customer_id.to_owned();
        let params = params.clone();
        db.client()
            .with_transaction(move |executor| {
                Box::pin(
                    async move { service.assignment_page(&actor, &customer_id, &params, executor).await },
                )
            })
            .await
    }

    /// 在同一快照内校验功能资格并读取完整归属分页与计数。
    async fn assignment_page(
        &self,
        actor: &AuditActor,
        customer_id: &str,
        params: &CustomerAssignmentListParams,
        executor: &mut dyn Executor,
    ) -> Result<PageView<CustomerAssignmentView>> {
        if !executor.boundary().is_some_and(|boundary| boundary.is_snapshot()) {
            return Err(Error::ValidationError("客户归属列表必须使用快照事务".into()));
        }
        self.access
            .service
            .require(actor, &customer::ASSIGNMENT_LIST, AccessTarget::Operation, executor)
            .await?;
        params.validate()?;
        let (sort_by, sort_dir) =
            normalize_sort(&params.sort_by, &params.sort_dir, CUSTOMER_ASSIGNMENT_SORT_FIELDS)?;
        let filter = CustomerAssignmentFilter {
            customer_id: Some(CustomerAccountId::new(customer_id)),
            user_id: params
                .user_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string),
            assignment_role: params.assignment_role,
            page: page_or_default(params.page),
            page_size: page_size_or_default(params.page_size),
            sort_by: Some(sort_by.to_string()),
            sort_ascending: matches!(sort_dir, SortDir::Asc),
        };
        let page = self.db.customer_assignments().search_customer_assignments(&filter, executor).await?;
        let items = page.items.into_iter().map(assignment_view).collect();

        Ok(PageView { items, total: page.total, page: filter.page, page_size: filter.page_size })
    }

    /// 应用归属变更（跨行事务：维护独立人员关系 + 审计原子写入）。
    ///
    /// - `Assign`：增加负责销售，不结束其他人员关系；同人同角色不得重叠。
    /// - `End`：提前结束既有归属的有效期（版本 CAS）。
    ///
    /// # 参数
    /// * `customer_id` - 客户角色 ID
    /// * `req` - 归属变更请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回本次变更涉及的归属行（新归属或结束后的目标归属）。
    ///
    /// # 错误
    /// * `NotFound` - 客户、销售人员账号或目标归属不存在
    /// * `ConflictError` - 版本冲突或新归属窗口与剩余归属重叠
    /// * `ValidationError` - 请求体校验失败
    ///
    /// 命令字段组合或归属窗口非法时返回校验或领域错误；授权、仓储或审计失败时返回对应错误。
    pub async fn apply_assignment(
        &self,
        customer_id: &str,
        req: CustomerAssignmentRequest,
        actor: &AuditActor,
    ) -> Result<Vec<CustomerAssignmentView>> {
        req.validate()?;
        match req.into_command()? {
            CustomerAssignmentCommand::Assign(command) => self.assign(customer_id, command, actor).await,
            CustomerAssignmentCommand::End(command) => self.end(customer_id, command, actor).await,
        }
    }

    /// 在调用方 Executor 上新增人员关系并校验重复区间。
    ///
    /// # 参数
    /// * `customer_id` - 客户角色 ID
    /// * `new_assignment` - 已构造的新归属
    /// * `executor` - 调用方执行器，必须位于事务中
    ///
    /// # 返回
    /// 返回新建归属。
    ///
    /// # 错误
    /// 窗口冲突或底层写入失败。
    pub async fn persist_assign(
        &self,
        customer_id: &str,
        new_assignment: &CustomerAssignment,
        executor: &mut dyn Executor,
    ) -> Result<Vec<CustomerAssignment>> {
        persist_assign(&self.db, customer_id, new_assignment, executor).await?;
        Ok(vec![new_assignment.clone()])
    }

    /// 建立新归属（拒绝同人重叠区间）。
    async fn assign(
        &self,
        customer_id: &str,
        command: AssignCustomerAssignment,
        actor: &AuditActor,
    ) -> Result<Vec<CustomerAssignmentView>> {
        let new_assignment = command
            .into_assignment(CustomerAssignmentId::new(next_id()), CustomerAccountId::new(customer_id))?;
        let audit = self.audit.resource_log(
            actor.clone(),
            "customer_assignment.assign",
            "customer_assignment",
            new_assignment.base.id.clone(),
        )?;

        let db = self.db.clone();
        let client = db.client().clone();
        let audit_port = self.audit.clone();
        let customer_id_for_tx = customer_id.to_string();
        let access = self.access.clone();
        let accounts = self.accounts.clone();
        let actor_for_tx = actor.clone();
        let created = client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    require_assignment(
                        &accounts,
                        &access,
                        &actor_for_tx,
                        &customer_id_for_tx,
                        &new_assignment,
                        executor,
                    )
                    .await?;
                    persist_assign(&db, &customer_id_for_tx, &new_assignment, executor).await?;
                    audit_port.persist(&audit, executor).await?;
                    Ok::<CustomerAssignment, crate::error::Error>(new_assignment)
                })
            })
            .await?;

        Ok(vec![created.into()])
    }

    /// 提前结束既有归属的有效期。
    async fn end(
        &self,
        customer_id: &str,
        command: EndCustomerAssignment,
        actor: &AuditActor,
    ) -> Result<Vec<CustomerAssignmentView>> {
        let audit = self.audit.resource_log(
            actor.clone(),
            "customer_assignment.end",
            "customer_assignment",
            command.assignment_id().to_string(),
        )?;

        let db = self.db.clone();
        let client = db.client().clone();
        let audit_port = self.audit.clone();
        let access = self.access.clone();
        let actor_for_tx = actor.clone();
        let customer_id_for_tx = customer_id.to_string();
        let ended = client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    access
                        .service
                        .require(
                            &actor_for_tx,
                            &customer::ASSIGNMENT_APPLY,
                            AccessTarget::Operation,
                            executor,
                        )
                        .await?;
                    let ended =
                        persist_end(&db, &access, &actor_for_tx, &customer_id_for_tx, &command, executor)
                            .await?;
                    audit_port.persist(&audit, executor).await?;
                    Ok::<CustomerAssignment, crate::error::Error>(ended)
                })
            })
            .await?;

        Ok(vec![ended.into()])
    }
}

/// 将已授权快照中的单行归属投影为原响应形状。
fn assignment_view(row: CustomerAssignmentRow) -> CustomerAssignmentView {
    CustomerAssignmentView {
        id: row.id,
        customer_id: row.customer_id,
        user_name: row.user_id.clone(),
        user_id: row.user_id,
        assignment_role: row.assignment_role,
        valid_from: row.valid_from,
        valid_to: row.valid_to,
        change_reason: row.change_reason,
        version: row.version,
        created_at: row.created_at,
    }
}

/// 在原事务中校验分配入口、原客户以及真实目标负责人。
async fn require_assignment(
    accounts: &AccountDirectoryService,
    access: &CustomerAccess,
    actor: &AuditActor,
    customer_id: &str,
    assignment: &CustomerAssignment,
    executor: &mut dyn Executor,
) -> Result<()> {
    access.service.require(actor, &customer::ASSIGNMENT_APPLY, AccessTarget::Operation, executor).await?;
    access.require_with(actor.clone(), &customer_permissions::UPDATE, customer_id, executor).await?;
    accounts
        .account(&assignment.user_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("负责销售账号不存在".into()))?
        .ensure_can_login()
        .map_err(|error| Error::BusinessLogicError(error.to_string()))?;
    if assignment.assignment_role == AssignmentRole::Owner {
        access.require_owner_change(actor, customer_id, &assignment.user_id, executor).await?;
    }
    Ok(())
}

/// 锁定客户根记录后校验并写入人员关系；必须收到事务执行器。
async fn persist_assign(
    db: &Database,
    customer_id: &str,
    new_assignment: &CustomerAssignment,
    executor: &mut dyn Executor,
) -> Result<()> {
    let customer_id = CustomerAccountId::new(customer_id);
    let mut customer = db
        .customer_accounts()
        .find_customer(customer_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::NotFound("客户不存在".into()))?;
    let existing = db.customer_assignments().list_for_customer(&customer_id, executor).await?;
    new_assignment.ensure_no_overlap(&existing).map_err(|error| Error::ConflictError(error.to_string()))?;
    db.customer_accounts().update(&mut customer, executor).await?;
    db.customer_assignments().create(new_assignment, executor).await?;
    Ok(())
}

/// 在原事务校验目标版本与区间覆盖，串行化同一客户的全部人员调整。
async fn persist_end(
    db: &Database,
    access: &CustomerAccess,
    actor: &AuditActor,
    customer_id: &str,
    command: &EndCustomerAssignment,
    executor: &mut dyn Executor,
) -> Result<CustomerAssignment> {
    access.require_with(actor.clone(), &customer_permissions::UPDATE, customer_id, executor).await?;
    let mut customer = db
        .customer_accounts()
        .find_customer(customer_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("客户不存在".into()))?;
    let mut assignment = db
        .customer_assignments()
        .find_assignment(command.assignment_id(), executor)
        .await?
        .ok_or_else(|| Error::NotFound("归属不存在".into()))?;
    assignment
        .ensure_customer(&CustomerAccountId::new(customer_id))
        .map_err(|error| Error::NotFound(error.to_string()))?;
    assignment.ensure_version(command.version()).map_err(|error| Error::ConflictError(error.to_string()))?;
    let assignments =
        db.customer_assignments().list_for_customer(&CustomerAccountId::new(customer_id), executor).await?;
    let owners = assignment
        .owners_after_end(&assignments, command.valid_to())
        .map_err(|error| Error::ValidationError(error.to_string()))?;
    for owners in owners {
        access.require_owner_set_change(actor, customer_id, &owners, executor).await?;
    }
    assignment.end_directly(command.valid_to()).map_err(|error| Error::ValidationError(error.to_string()))?;
    assignment.change_reason = command.change_reason().to_string();
    db.customer_accounts().update(&mut customer, executor).await?;
    db.customer_assignments().update(&mut assignment, executor).await?;
    Ok(assignment)
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use erp_core::common::time::BusinessDate;
    use erp_identity::testing::unavailable_authorization;
    use persistence_core::ReadExecutor;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;
    use crate::ports::FailClosedAuditPort;

    #[tokio::test]
    async fn assigning_checks_entry_before_customer_or_target_account_reads() {
        let client =
            mongodb::Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1").await.unwrap();
        let db = client.database("customer_assignment_create_authorization");
        let access = CustomerAccess::new(db.clone(), unavailable_authorization());
        let accounts = AccountDirectoryService::new(db);
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        let assignment = AssignCustomerAssignment::new(
            "unread-owner".into(),
            AssignmentRole::Owner,
            BusinessDate::from_ymd(2026, 11, 13).unwrap(),
            None,
            "新增负责人".into(),
        )
        .unwrap()
        .into_assignment(
            CustomerAssignmentId::new("new-assignment"),
            CustomerAccountId::new("unread-customer"),
        )
        .unwrap();
        let error = require_assignment(
            &accounts,
            &access,
            &actor,
            "unread-customer",
            &assignment,
            &mut SnapshotExecutor::default(),
        )
        .await
        .expect_err("授权失败不得探测客户或目标账号");
        assert!(matches!(error, Error::Internal(message) if message == "测试授权设施不可用"));
    }

    #[tokio::test]
    async fn ending_assignment_authorizes_original_before_detail_or_version_reads() {
        let client =
            mongodb::Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1").await.unwrap();
        let db = client.database("customer_assignment_end_authorization");
        let access = CustomerAccess::new(db.clone(), unavailable_authorization());
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        for version in [1, 999] {
            let command = EndCustomerAssignment::new(
                "unread-assignment".into(),
                BusinessDate::from_ymd(2026, 11, 13).unwrap(),
                version,
                "结束归属".into(),
            )
            .unwrap();
            let error = persist_end(
                &db,
                &access,
                &actor,
                "unread-customer",
                &command,
                &mut SnapshotExecutor::default(),
            )
            .await
            .expect_err("原客户授权失败不得探测归属存在性或版本");
            assert!(matches!(error, Error::Internal(message) if message == "测试授权设施不可用"));
        }
    }

    #[tokio::test]
    async fn assignment_list_checks_qualification_before_reading_any_rows() {
        let client =
            mongodb::Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1").await.unwrap();
        let db = client.database("customer_assignment_authorization");
        let access = CustomerAccess::new(db.clone(), unavailable_authorization());
        let service = CustomerAssignmentService::new(db, Arc::new(FailClosedAuditPort), access);
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        let params: CustomerAssignmentListParams = serde_json::from_value(serde_json::json!({})).unwrap();
        let error = service
            .assignment_page(&actor, "customer", &params, &mut SnapshotExecutor::default())
            .await
            .expect_err("功能授权失败必须停止");
        assert!(matches!(error, Error::Internal(message) if message == "测试授权设施不可用"));
        let error = service
            .assignment_page(&actor, "customer", &params, &mut ReadExecutor::default())
            .await
            .expect_err("非快照执行器必须拒绝");
        assert!(
            matches!(error, Error::ValidationError(message) if message == "客户归属列表必须使用快照事务")
        );
    }
}
