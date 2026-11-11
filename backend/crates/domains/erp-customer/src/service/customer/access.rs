//! 客户用例仅执行统一授权查询，不解释角色范围或记录授权。
use std::collections::BTreeSet;
use std::sync::Arc;

use application_core::AuditActor;
use erp_core::common::time::{BusinessDate, Instant};
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use crate::entity::customer::AssignmentRole;
use crate::ports::{CustomerDataScopePort, CustomerResolvedScope};
use crate::repository::CustomerExt;
use crate::repository::prelude::*;
use crate::repository::scope::{CustomerReadScope, authorized_ids};
use crate::{Error, Result};

/// 客户查询与命令共用的统一授权接入。
#[derive(Clone)]
pub struct CustomerAccess {
    db: Database,
    scope: Arc<dyn CustomerDataScopePort>,
}
impl CustomerAccess {
    /// 绑定客户仓储与统一授权端口。
    /// # 参数
    /// db 为客户数据库，scope 为组合根装配的 identity 接入。
    /// # 返回
    /// 无缓存访问器。
    /// # 错误
    /// 无。
    pub fn new(db: Database, scope: Arc<dyn CustomerDataScopePort>) -> Self {
        Self { db, scope }
    }
    /// 准备查询凭证和业务归属筛选，不预装全量客户主键。
    /// # 参数
    /// actor、action 为当前入口，executor 为原事务。
    /// # 返回
    /// 当前查询凭证与「我的客户」筛选事实。
    /// # 错误
    /// 身份、授权或业务归属读取失败时拒绝。
    pub async fn query(
        &self,
        actor: &AuditActor,
        action: &str,
        executor: &mut dyn Executor,
    ) -> Result<(CustomerResolvedScope, CustomerReadScope)> {
        let context = self.scope.resolve(actor, action, executor).await?;
        let assignments = self
            .db
            .customer_assignments()
            .find_active_assignments_for_user(actor.id(), business_date(context.as_of)?, executor)
            .await?;
        let owned = assignments
            .into_iter()
            .filter(|a| a.assignment_role == AssignmentRole::Owner)
            .map(|a| a.customer_id.to_string())
            .collect();
        Ok((
            context,
            CustomerReadScope {
                authorized_customer_ids: None,
                owned_customer_ids: owned,
                collaborative_customer_ids: vec![],
            },
        ))
    }
    /// 在同一执行器查询完整授权集合，并读取「我的客户」业务筛选。
    /// # 参数
    /// actor、action 为服务端入口，executor 为原事务。
    /// # 返回
    /// 不含权限条款的查询凭证和客户主键结果。
    /// # 错误
    /// 账号、授权、事实或查询超限时拒绝。
    pub async fn resolve(
        &self,
        actor: &AuditActor,
        action: &str,
        executor: &mut dyn Executor,
    ) -> Result<(CustomerResolvedScope, CustomerReadScope)> {
        let context = self.scope.resolve(actor, action, executor).await?;
        let compiled = context.compiled.as_ref().ok_or_else(|| Error::Internal("客户查询缺少凭证".into()))?;
        self.scope.validate(&context, executor).await?;
        let ids = authorized_ids(&self.db, compiled, None, executor).await?;
        let assignments = self
            .db
            .customer_assignments()
            .find_active_assignments_for_user(actor.id(), business_date(context.as_of)?, executor)
            .await?;
        let owned = assignments
            .into_iter()
            .filter(|a| a.assignment_role == AssignmentRole::Owner)
            .map(|a| a.customer_id.to_string())
            .collect();
        self.scope.validate(&context, executor).await?;
        Ok((
            context,
            CustomerReadScope {
                authorized_customer_ids: Some(ids),
                owned_customer_ids: owned,
                collaborative_customer_ids: vec![],
            },
        ))
    }
    /// 在独立事务执行完整客户查询授权。
    /// # 参数
    /// actor 为账号，action 为固定动作，customer_id 为实际客户。
    /// # 返回
    /// 本阶段授权上下文。
    /// # 错误
    /// 客户缺失或无权时拒绝。
    pub async fn require(
        &self,
        actor: &AuditActor,
        action: &str,
        customer_id: &str,
    ) -> Result<CustomerResolvedScope> {
        let this = self.clone();
        let actor = actor.clone();
        let action = action.to_owned();
        let id = customer_id.to_owned();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { this.require_with(actor, &action, &id, executor).await })
            })
            .await
    }
    /// 在原业务事务使用 identity 完整查询约束检查目标成员资格。
    /// # 参数
    /// actor、action、customer_id 为真实入口和目标，executor 为原事务。
    /// # 返回
    /// 成功时返回授权上下文。
    /// # 错误
    /// 缺失、无权、版本变化或接入失败时拒绝。
    pub async fn require_with(
        &self,
        actor: AuditActor,
        action: &str,
        customer_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<CustomerResolvedScope> {
        let context = self.scope.resolve(&actor, action, executor).await?;
        let compiled = context.compiled.as_ref().ok_or_else(|| Error::Internal("客户查询缺少凭证".into()))?;
        self.scope.validate(&context, executor).await?;
        let ids = authorized_ids(&self.db, compiled, Some(customer_id), executor).await?;
        self.scope.validate(&context, executor).await?;
        if ids.is_empty() {
            return Err(deny_object(action));
        }
        self.ensure_owner_facts(customer_id, context.as_of, executor).await?;
        self.scope.validate(&context, executor).await?;
        Ok(context)
    }
    /// 完整对象读取校验全部实际负责人组织关系，避免其他授权分支掩盖异常事实。
    async fn ensure_owner_facts(&self, id: &str, at: Instant, executor: &mut dyn Executor) -> Result<()> {
        let owners = self
            .db
            .customer_assignments()
            .current_owners(Some(&[id.to_string()]), None, business_date(at)?, executor)
            .await?;
        for owner in owners {
            self.scope.own_org(&owner.user_id, at, executor).await?;
        }
        Ok(())
    }
    /// 当前业务资格使用关闭历史读取补充的独立入口。
    /// # 参数
    /// actor、customer_id 为真实账号和客户，executor 为原事务。
    /// # 返回
    /// 当前客户资格成立时成功。
    /// # 错误
    /// 仅有历史资格、客户缺失或无权时拒绝。
    pub async fn require_current(
        &self,
        actor: &AuditActor,
        customer_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<CustomerResolvedScope> {
        self.require_with(actor.clone(), "current", customer_id, executor).await
    }
    /// 检查创建按钮的功能资格。
    /// # 参数
    /// actor 为当前账号。
    /// # 返回
    /// 当前创建功能资格。
    /// # 错误
    /// 身份或功能资格无效时拒绝。
    pub async fn ensure_create(&self, actor: &AuditActor) -> Result<CustomerResolvedScope> {
        let this = self.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { this.require_create(&actor, executor).await })
            })
            .await
    }
    /// 创建前校验功能资格；真实新客户在原写事务内通过 create 查询重验。
    /// # 参数
    /// actor 为当前账号，executor 为原写事务。
    /// # 返回
    /// 当前功能资格元信息。
    /// # 错误
    /// 账号或权限无效时拒绝。
    pub async fn require_create(
        &self,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<CustomerResolvedScope> {
        self.scope.creation(actor, executor).await
    }
}
pub(crate) fn business_date(at: Instant) -> Result<BusinessDate> {
    let date = at.as_utc() + chrono::Duration::hours(8);
    Ok(date.format("%Y-%m-%d").to_string().parse()?)
}

pub(super) fn intersect_ids(left: Option<Vec<String>>, right: Option<Vec<String>>) -> Option<Vec<String>> {
    match (left, right) {
        (None, other) | (other, None) => other,
        (Some(left), Some(right)) => {
            let right = right.into_iter().collect::<BTreeSet<_>>();
            Some(left.into_iter().filter(|id| right.contains(id)).collect())
        },
    }
}

/// 拒绝不可见对象且不泄露存在性差异。
///
/// # 参数
/// * `action` - 客户动作
///
/// # 返回
/// 读取返回 NotFound；写入返回 Forbidden。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 不可读详情不得暴露客户是否存在。
pub(super) fn deny_object(action: &str) -> Error {
    if matches!(action, "list" | "detail") {
        Error::NotFound("客户不存在或无权查看".into())
    } else {
        Error::Forbidden("当前用户不在该客户的授权范围内".into())
    }
}

/// 拒绝超过查询上限的展开结果。
///
/// 列表与组织筛选共用同一上限（`scope` 复用本函数，避免各自硬编码 10000）。
///
/// # 参数
/// * `count` - 已展开数量
/// * `message` - 超限提示
///
/// # 返回
/// 未超限时返回 `Ok`。
///
/// # 错误
/// 超过 10000 时返回校验错误。
///
/// # 关键业务约束
/// 不得静默截断授权或筛选身份集合。
pub(super) fn ensure_limit(count: usize, message: &str) -> Result<()> {
    if count > QUERY_LIMIT {
        return Err(Error::ValidationError(format!("{message}，请收窄授权范围")));
    }
    Ok(())
}

/// 授权与筛选集合的查询上限。
pub(super) const QUERY_LIMIT: usize = 10_000;
