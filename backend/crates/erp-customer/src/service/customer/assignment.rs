//! 域 D08 `customer_assignment` 服务编排。
//!
//! 归属变化按「结束旧归属并建立新归属」维护（W03），不原地修改人员/角色；
//! 归属是新单据参与权的来源（§6.2）。Service 在事务内加载冲突候选并
//! 持久化，窗口、角色冲突与负责人结束限制由 `CustomerAssignment` 判定：
//! - 同一客户同一时点恰好一个 `OWNER`；
//! - 同一客户、用户、角色的有效期不得重叠。

use std::sync::Arc;

use crate::dto::customer::{
    CustomerAssignmentListParams, CustomerAssignmentRequest, CustomerAssignmentView, PageView, SortDir,
    CUSTOMER_ASSIGNMENT_SORT_FIELDS,
};
use crate::entity::customer::{
    AssignCustomerAssignment, CustomerAssignment, CustomerAssignmentCommand, CustomerAssignmentId,
    EndCustomerAssignment,
};
use crate::error::{Error, Result};
use crate::ports::{AccountFactPort, CustomerAuditPort, CustomerDataScopePort};
use crate::repository::CustomerExt;
use application_core::{normalize_sort, page_or_default, page_size_or_default, AuditActor};
use erp_core::ids::CustomerAccountId;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction, Transactional};
use validator::Validate;

use super::access::CustomerAccess;

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
        Self {
            db,
            audit,
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
        let page = self
            .db
            .customer_assignments()
            .search_customer_assignments(&filter, &mut NoTransaction)
            .await?;
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

        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 应用归属变更（跨行事务：结束旧归属 + 建立新归属 + 审计原子写入）。
    ///
    /// - `Assign`：结束同一客户同一角色的重叠旧归属（OWNER 唯一，换负责人时
    ///   结束既有 OWNER 有效期并建立新 OWNER）；新窗口与剩余归属不得重叠。
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

    /// 在调用方 Executor 上建立新归属并结束重叠旧归属。
    ///
    /// # 参数
    /// * `customer_id` - 客户角色 ID
    /// * `new_assignment` - 已构造的新归属
    /// * `executor` - 调用方执行器，必须位于事务中
    ///
    /// # 返回
    /// 返回被结束的旧归属与新建归属。
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

    /// 在调用方 Executor 上结束既有协作归属。
    ///
    /// # 参数
    /// * `assignment` - 已应用结束规则的归属
    /// * `executor` - 调用方执行器
    ///
    /// # 错误
    /// 版本冲突或底层写入失败。
    pub async fn persist_end(
        &self,
        assignment: &mut CustomerAssignment,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.db
            .customer_assignments()
            .update(assignment, executor)
            .await?;
        Ok(())
    }

    /// 建立新归属（结束重叠旧归属）。
    async fn assign(
        &self,
        customer_id: &str,
        command: AssignCustomerAssignment,
        actor: &AuditActor,
    ) -> Result<Vec<CustomerAssignmentView>> {
        let new_assignment = command.into_assignment(
            CustomerAssignmentId::new(next_id()),
            CustomerAccountId::new(customer_id),
        )?;
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
        let new_for_tx = new_assignment.clone();
        let access = self.access();
        let actor_for_tx = actor.clone();
        let changed = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    access
                        .require_with(actor_for_tx, "update", &customer_id_for_tx, session)
                        .await?;
                    let changed = persist_assign(&db, &customer_id_for_tx, &new_for_tx, session).await?;
                    audit_port.persist(&audit, session).await?;
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
        let mut assignment = self
            .db
            .customer_assignments()
            .find_assignment(command.assignment_id(), &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("归属不存在".to_string()))?;
        assignment
            .ensure_customer(&CustomerAccountId::new(customer_id))
            .map_err(|error| Error::NotFound(error.to_string()))?;
        assignment
            .ensure_version(command.version())
            .map_err(|error| Error::ConflictError(error.to_string()))?;
        assignment
            .end_directly(command.valid_to())
            .map_err(|error| Error::ValidationError(error.to_string()))?;
        let audit = self.audit.resource_log(
            actor.clone(),
            "customer_assignment.end",
            "customer_assignment",
            assignment.base.id.clone(),
        )?;

        let db = self.db.clone();
        let client = db.client().clone();
        let audit_port = self.audit.clone();
        let mut assignment_for_tx = assignment.clone();
        let access = self.access();
        let actor_for_tx = actor.clone();
        let customer_id_for_tx = customer_id.to_string();
        let ended = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    access
                        .require_with(actor_for_tx, "update", &customer_id_for_tx, session)
                        .await?;
                    db.customer_assignments()
                        .update(&mut assignment_for_tx, session)
                        .await?;
                    audit_port.persist(&audit, session).await?;
                    Ok::<CustomerAssignment, crate::error::Error>(assignment_for_tx)
                })
            })
            .await?;

        Ok(vec![ended.into()])
    }
}

/// 结束重叠旧归属并写入新归属。**必须收到事务执行器**。
async fn persist_assign(
    db: &Database,
    customer_id: &str,
    new_assignment: &CustomerAssignment,
    executor: &mut dyn Executor,
) -> Result<Vec<CustomerAssignment>> {
    let customer_id = CustomerAccountId::new(customer_id);
    let mut ended = end_overlapping(db, &customer_id, new_assignment, executor).await?;
    db.customer_assignments().create(new_assignment, executor).await?;
    ended.push(new_assignment.clone());
    Ok(ended)
}

/// 结束与 `new_assignment` 重叠的旧归属（§6.2 跨行约束）。
///
/// - `OWNER`：同一客户任一其他 OWNER 与新区间重叠时结束其有效期
///   （换负责人：`new.valid_from` 必须晚于旧归属 `valid_from`）；
/// - `COLLABORATOR`：同一客户、同一用户、同一角色的重叠区间被结束。
///
/// 结束方式：把旧归属 `valid_to` 置为 `new.valid_from`（结束日为开区间，
/// 新旧归属无空档）。**必须收到事务执行器**。
async fn end_overlapping(
    db: &Database,
    customer_id: &CustomerAccountId,
    new_assignment: &CustomerAssignment,
    executor: &mut dyn Executor,
) -> Result<Vec<CustomerAssignment>> {
    let mut ended = Vec::new();
    let existing = db
        .customer_assignments()
        .list_for_customer(customer_id, executor)
        .await?;
    for mut old in existing {
        let changed = old
            .end_for_replacement(new_assignment)
            .map_err(|error| Error::ConflictError(error.to_string()))?;
        if !changed {
            continue;
        }
        db.customer_assignments().update(&mut old, executor).await?;
        ended.push(old);
    }
    Ok(ended)
}
