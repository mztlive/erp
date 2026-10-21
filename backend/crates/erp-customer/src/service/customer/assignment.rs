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
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction, Transactional};
use validator::Validate;

use super::access::CustomerAccess;
use crate::dto::customer::{
    CUSTOMER_ASSIGNMENT_SORT_FIELDS, CustomerAssignmentListParams, CustomerAssignmentRequest,
    CustomerAssignmentView, PageView, SortDir,
};
use crate::entity::customer::{
    AssignCustomerAssignment, CustomerAssignment, CustomerAssignmentCommand, CustomerAssignmentId,
    EndCustomerAssignment,
};
use crate::error::{Error, Result};
use crate::ports::{AccountFactPort, CustomerAuditPort, CustomerDataScopePort};
use crate::repository::CustomerExt;
use crate::repository::prelude::*;

/// 客户归属列表筛选条件类型（经 `CustomerExt` 关联类型跨 crate 可达）。
type CustomerAssignmentFilter = <mongodb::Database as CustomerExt>::CustomerAssignmentFilter;

/// 客户归属服务。
pub struct CustomerAssignmentService {
    db: Database,
    audit: Arc<dyn CustomerAuditPort>,
    accounts: Arc<dyn AccountFactPort>,
    data_scope: Arc<dyn CustomerDataScopePort>,
}

impl CustomerAssignmentService {
    /// 创建客户归属服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    /// * `audit` - 审计写入端口
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
    /// 归属变更必须通过注入的 Port 证明 customer:update 范围。
    pub fn new(
        db: Database,
        audit: Arc<dyn CustomerAuditPort>,
        accounts: Arc<dyn AccountFactPort>,
        data_scope: Arc<dyn CustomerDataScopePort>,
    ) -> Self {
        Self { db, audit, accounts, data_scope }
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
    fn access(&self) -> CustomerAccess {
        CustomerAccess::new(self.db.clone(), Arc::clone(&self.data_scope))
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
        customer_id: &str,
        params: &CustomerAssignmentListParams,
    ) -> Result<PageView<CustomerAssignmentView>> {
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
        let page =
            self.db.customer_assignments().search_customer_assignments(&filter, &mut NoTransaction).await?;
        let items = page
            .items
            .into_iter()
            .map(|row| CustomerAssignmentView {
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
            })
            .collect();

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
        persist_assign(&self.db, customer_id, new_assignment, executor).await
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
        self.db
            .customer_accounts()
            .find_customer(customer_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("客户不存在".to_string()))?;
        self.accounts.ensure_can_login(&new_assignment.user_id).await?;
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
        let new_for_tx = new_assignment;
        let access = self.access();
        let actor_for_tx = actor.clone();
        let changed = client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    access.require_with(actor_for_tx, "update", &customer_id_for_tx, executor).await?;
                    let changed = persist_assign(&db, &customer_id_for_tx, &new_for_tx, executor).await?;
                    audit_port.persist(&audit, executor).await?;
                    Ok::<Vec<CustomerAssignment>, crate::error::Error>(changed)
                })
            })
            .await?;

        Ok(changed.into_iter().map(Into::into).collect())
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
        let access = self.access();
        let actor_for_tx = actor.clone();
        let customer_id_for_tx = customer_id.to_string();
        let ended = client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    access.require_with(actor_for_tx, "update", &customer_id_for_tx, executor).await?;
                    let ended = persist_end(&db, &customer_id_for_tx, &command, executor).await?;
                    audit_port.persist(&audit, executor).await?;
                    Ok::<CustomerAssignment, crate::error::Error>(ended)
                })
            })
            .await?;

        Ok(vec![ended.into()])
    }
}

/// 锁定客户根记录后校验并写入人员关系；必须收到事务执行器。
async fn persist_assign(
    db: &Database,
    customer_id: &str,
    new_assignment: &CustomerAssignment,
    executor: &mut dyn Executor,
) -> Result<Vec<CustomerAssignment>> {
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
    Ok(vec![new_assignment.clone()])
}

/// 在原事务校验目标版本与区间覆盖，串行化同一客户的全部人员调整。
async fn persist_end(
    db: &Database,
    customer_id: &str,
    command: &EndCustomerAssignment,
    executor: &mut dyn Executor,
) -> Result<CustomerAssignment> {
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
    assignment
        .ensure_owner_retained(&assignments, command.valid_to())
        .map_err(|error| Error::ValidationError(error.to_string()))?;
    assignment.end_directly(command.valid_to()).map_err(|error| Error::ValidationError(error.to_string()))?;
    assignment.change_reason = command.change_reason().to_string();
    db.customer_accounts().update(&mut customer, executor).await?;
    db.customer_assignments().update(&mut assignment, executor).await?;
    Ok(assignment)
}
