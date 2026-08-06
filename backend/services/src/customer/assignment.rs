//! 域 D08 `customer_assignment` 服务编排。
//!
//! 归属变化按「结束旧归属并建立新归属」维护（W03），不原地修改人员/角色；
//! 归属是新单据参与权的来源（§6.2）。约束（§6.2 跨行，事务内校验）：
//! - 同一客户同一时点恰好一个 `OWNER`；
//! - 同一客户、用户、角色的有效期不得重叠。

use database::{AccessControlExt, CustomerExt, NoTransaction, Transactional};
use entities::common::time::BusinessDate;
use entities::customer::{
    AssignmentRole, CustomerAssignment, CustomerAssignmentData, CustomerAssignmentId,
    CustomerAssignmentUpdate,
};
use entities::ids::CustomerAccountId;
use id_generator::next_id;
use mongodb::bson::doc;
use mongodb::Database;
use validator::Validate;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};

use super::dto::{
    normalize_sort, AssignmentAction, CustomerAssignmentListParams, CustomerAssignmentRequest,
    CustomerAssignmentView, PageView, SortDir, CUSTOMER_ASSIGNMENT_SORT_FIELDS,
};
use super::{page_or_default, page_size_or_default};

/// 客户归属列表筛选条件类型（经 `CustomerExt` 关联类型跨 crate 可达）。
type CustomerAssignmentFilter = <mongodb::Database as CustomerExt>::CustomerAssignmentFilter;

/// 客户归属服务。
pub struct CustomerAssignmentService {
    db: Database,
}

impl CustomerAssignmentService {
    /// 创建客户归属服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(db: Database) -> Self {
        Self { db }
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
        match req.action {
            AssignmentAction::Assign => self.assign(customer_id, req, actor).await,
            AssignmentAction::End => self.end(customer_id, req, actor).await,
        }
    }

    /// 建立新归属（结束重叠旧归属）。
    ///
    /// # 参数
    /// * `customer_id` - 客户角色 ID
    /// * `req` - 归属变更请求（`Assign` 分支）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建立的归属行。
    ///
    /// # 错误
    /// * `NotFound` - 客户或销售人员账号不存在
    /// * `ConflictError` - 新窗口与剩余归属重叠
    async fn assign(
        &self,
        customer_id: &str,
        req: CustomerAssignmentRequest,
        actor: &AuditActor,
    ) -> Result<Vec<CustomerAssignmentView>> {
        let user_id = req
            .user_id
            .ok_or_else(|| Error::ValidationError("Assign 必须携带销售人员".to_string()))?;
        let assignment_role = req
            .assignment_role
            .ok_or_else(|| Error::ValidationError("Assign 必须携带归属角色".to_string()))?;
        let valid_from = req
            .valid_from
            .ok_or_else(|| Error::ValidationError("Assign 必须携带生效开始日期".to_string()))?;
        let valid_to = req.valid_to;
        ensure_window_valid(valid_from, valid_to)?;
        self.ensure_customer_exists(customer_id).await?;
        self.ensure_user_exists(&user_id).await?;

        let new_assignment = CustomerAssignment::new(
            CustomerAssignmentId::new(next_id()),
            CustomerAssignmentData {
                customer_id: CustomerAccountId::new(customer_id),
                user_id,
                assignment_role,
                valid_from,
                valid_to,
                change_reason: req.change_reason,
            },
        )?;
        let audit = actor.clone().resource_log(
            "customer_assignment.assign",
            "customer_assignment",
            new_assignment.base.id.clone(),
        )?;

        let db = self.db.clone();
        let client = db.client().clone();
        let customer_id_for_tx = CustomerAccountId::new(customer_id);
        let new_for_tx = new_assignment.clone();
        let changed = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let mut ended = end_overlapping(&db, &customer_id_for_tx, &new_for_tx, session).await?;
                    db.customer_assignments().create(&new_for_tx, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    ended.push(new_for_tx.clone());
                    Ok::<Vec<CustomerAssignment>, crate::errors::Error>(ended)
                })
            })
            .await?;

        Ok(changed.into_iter().map(Into::into).collect())
    }

    /// 提前结束既有归属的有效期。
    ///
    /// # 参数
    /// * `customer_id` - 客户角色 ID
    /// * `req` - 归属变更请求（`End` 分支）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回结束后的目标归属行。
    ///
    /// # 错误
    /// * `NotFound` - 目标归属不存在或不属于该客户
    /// * `ConflictError` - 期望版本与当前版本不一致
    async fn end(
        &self,
        customer_id: &str,
        req: CustomerAssignmentRequest,
        actor: &AuditActor,
    ) -> Result<Vec<CustomerAssignmentView>> {
        let assignment_id = req
            .assignment_id
            .ok_or_else(|| Error::ValidationError("End 必须携带目标归属 ID".to_string()))?;
        let valid_to = req
            .valid_to
            .ok_or_else(|| Error::ValidationError("End 必须携带生效结束日期".to_string()))?;
        let mut assignment = self
            .db
            .customer_assignments()
            .find_by_id(&assignment_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("归属不存在".to_string()))?;
        if assignment.customer_id.as_ref() != customer_id {
            return Err(Error::NotFound("归属不属于该客户".to_string()));
        }
        if assignment.base.version != req.version {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        assignment.update(CustomerAssignmentUpdate {
            valid_to: entities::field_update::FieldUpdate::Set(valid_to),
        })?;
        let audit = actor.clone().resource_log(
            "customer_assignment.end",
            "customer_assignment",
            assignment.base.id.clone(),
        )?;

        let db = self.db.clone();
        let client = db.client().clone();
        let mut assignment_for_tx = assignment.clone();
        let ended = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.customer_assignments()
                        .update(&mut assignment_for_tx, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<CustomerAssignment, crate::errors::Error>(assignment_for_tx)
                })
            })
            .await?;

        Ok(vec![ended.into()])
    }

    /// 校验客户角色存在。
    ///
    /// # 参数
    /// * `customer_id` - 客户角色 ID
    ///
    /// # 返回
    /// 客户存在返回 `Ok(())`。
    ///
    /// # 错误
    /// * `NotFound` - 客户不存在
    async fn ensure_customer_exists(&self, customer_id: &str) -> Result<()> {
        self.db
            .customer_accounts()
            .find_by_id(customer_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("客户不存在".to_string()))?;
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
            .ok_or_else(|| Error::NotFound("销售人员账号不存在".to_string()))?;
        Ok(())
    }
}

/// 结束与 `new_assignment` 重叠的旧归属（§6.2 跨行约束）。
///
/// - `OWNER`：同一客户任一其他 OWNER 与新区间重叠时结束其有效期
///   （换负责人：`new.valid_from` 必须晚于旧归属 `valid_from`）；
/// - `COLLABORATOR`：同一客户、同一用户、同一角色的重叠区间被结束。
///
/// 结束方式：把旧归属 `valid_to` 置为 `new.valid_from` 的前一天（含端点语义，
/// 满足实体「结束晚于开始」校验）。**必须收到事务执行器**。
///
/// # 参数
/// * `db` - 数据库实例
/// * `customer_id` - 客户角色 ID
/// * `new_assignment` - 新归属
/// * `executor` - 数据访问执行器，必须位于事务中
///
/// # 返回
/// 返回被结束的旧归属行。
///
/// # 错误
/// 当新窗口起点不晚于旧归属起点（无法结束）或 CAS 更新失败时返回错误。
async fn end_overlapping(
    db: &Database,
    customer_id: &CustomerAccountId,
    new_assignment: &CustomerAssignment,
    executor: &mut dyn database::Executor,
) -> Result<Vec<CustomerAssignment>> {
    let mut ended = Vec::new();
    let existing = db
        .customer_assignments()
        .find_many(doc! { "customer_id": customer_id.to_string() }, executor)
        .await?;
    for mut old in existing {
        if !conflicts(&old, new_assignment) {
            continue;
        }
        let end_date = previous_day(new_assignment.valid_from);
        if end_date < old.valid_from {
            return Err(Error::ConflictError(
                "新归属开始日期必须晚于旧归属开始日期，请调整生效日期".to_string(),
            ));
        }
        old.update(CustomerAssignmentUpdate {
            valid_to: entities::field_update::FieldUpdate::Set(end_date),
        })?;
        db.customer_assignments().update(&mut old, executor).await?;
        ended.push(old);
    }
    Ok(ended)
}

/// 判断旧归属是否需要被新归属结束（角色/用户与区间重叠判定）。
///
/// # 参数
/// * `old` - 既有归属
/// * `new_assignment` - 新归属
///
/// # 返回
/// 需要结束时返回 `true`。
fn conflicts(old: &CustomerAssignment, new_assignment: &CustomerAssignment) -> bool {
    let same_role = old.assignment_role == new_assignment.assignment_role;
    let owner_conflict = new_assignment.assignment_role == AssignmentRole::Owner;
    let same_user = old.user_id == new_assignment.user_id;
    if !(same_role && (owner_conflict || same_user)) {
        return false;
    }
    windows_overlap(
        old.valid_from,
        old.valid_to,
        new_assignment.valid_from,
        new_assignment.valid_to,
    )
}

/// 判断两个生效区间是否重叠（含端点语义；`None` 视为长期有效）。
///
/// # 参数
/// * `a_from` - A 区间开始
/// * `a_to` - A 区间结束（`None` 表示无穷远）
/// * `b_from` - B 区间开始
/// * `b_to` - B 区间结束（`None` 表示无穷远）
///
/// # 返回
/// 重叠返回 `true`。
fn windows_overlap(
    a_from: BusinessDate,
    a_to: Option<BusinessDate>,
    b_from: BusinessDate,
    b_to: Option<BusinessDate>,
) -> bool {
    let a_covers = |day: BusinessDate| a_to.is_none_or(|end| day <= end);
    let b_covers = |day: BusinessDate| b_to.is_none_or(|end| day <= end);
    a_covers(b_from) && b_covers(a_from)
}

/// 返回指定日期的前一天（用于「结束旧归属」的端点计算）。
///
/// # 参数
/// * `date` - 基准日期
///
/// # 返回
/// 返回前一天的自然日。
fn previous_day(date: BusinessDate) -> BusinessDate {
    use std::str::FromStr;
    let previous = date.as_naive_date().pred_opt().unwrap_or(date.as_naive_date());
    BusinessDate::from_str(&previous.to_string()).unwrap_or(date)
}

/// 校验生效区间：`valid_to` 必须晚于 `valid_from`。
///
/// # 参数
/// * `valid_from` - 生效开始日期
/// * `valid_to` - 生效结束日期（可空）
///
/// # 返回
/// 区间合法返回 `Ok(())`。
///
/// # 错误
/// 结束日期不晚于开始日期时返回 `ValidationError`。
fn ensure_window_valid(valid_from: BusinessDate, valid_to: Option<BusinessDate>) -> Result<()> {
    if let Some(valid_to) = valid_to {
        if valid_to <= valid_from {
            return Err(Error::ValidationError(
                "生效结束日期必须晚于生效开始日期".to_string(),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use entities::common::time::BusinessDate;
    use entities::customer::{AssignmentRole, CustomerAssignment, CustomerAssignmentData};
    use entities::ids::{CustomerAccountId, CustomerAssignmentId};

    use super::{conflicts, previous_day, windows_overlap};

    fn assignment(from: &str, to: Option<&str>, role: AssignmentRole, user: &str) -> CustomerAssignment {
        let from = BusinessDate::from_str(from).unwrap();
        CustomerAssignment::new(
            CustomerAssignmentId::new("a-1"),
            CustomerAssignmentData {
                customer_id: CustomerAccountId::new("c-1"),
                user_id: user.to_string(),
                assignment_role: role,
                valid_from: from,
                valid_to: to.map(|value| BusinessDate::from_str(value).unwrap()),
                change_reason: "测试".to_string(),
            },
        )
        .unwrap()
    }

    #[test]
    fn windows_overlap_uses_inclusive_endpoints() {
        let from = |value: &str| BusinessDate::from_str(value).unwrap();
        assert!(windows_overlap(
            from("2026-01-01"),
            Some(from("2026-03-31")),
            from("2026-03-31"),
            None,
        ));
        assert!(!windows_overlap(
            from("2026-01-01"),
            Some(from("2026-03-30")),
            from("2026-03-31"),
            None,
        ));
    }

    #[test]
    fn owner_conflicts_regardless_of_user() {
        let old = assignment("2026-01-01", Some("2026-12-31"), AssignmentRole::Owner, "sales-a");
        let new_owner = assignment("2026-06-01", None, AssignmentRole::Owner, "sales-b");
        let new_collab = assignment("2026-06-01", None, AssignmentRole::Collaborator, "sales-b");
        assert!(conflicts(&old, &new_owner));
        assert!(!conflicts(&old, &new_collab));
    }

    #[test]
    fn collaborator_conflicts_only_same_user() {
        let old = assignment(
            "2026-01-01",
            Some("2026-12-31"),
            AssignmentRole::Collaborator,
            "sales-a",
        );
        let same_user = assignment("2026-06-01", None, AssignmentRole::Collaborator, "sales-a");
        let other_user = assignment("2026-06-01", None, AssignmentRole::Collaborator, "sales-b");
        assert!(conflicts(&old, &same_user));
        assert!(!conflicts(&old, &other_user));
    }

    #[test]
    fn previous_day_handles_month_boundary() {
        assert_eq!(
            previous_day(BusinessDate::from_ymd(2026, 3, 1).unwrap()),
            BusinessDate::from_ymd(2026, 2, 28).unwrap()
        );
    }
}
