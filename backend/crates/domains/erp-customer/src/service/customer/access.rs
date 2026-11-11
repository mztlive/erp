//! 客户用例仅执行统一授权查询，不解释角色范围或记录授权。
use std::sync::Arc;

use application_core::AuditActor;
use erp_core::common::time::{BusinessDate, Instant};
use erp_identity::{
    AccessMapping, AccessSubject, AuthorizationService, OrganizationDirectoryService, Permission,
    SharedRbacService,
};
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use crate::authorization::{CustomerMapping, CustomerResolvedScope, service};
use crate::entity::customer::AssignmentRole;
use crate::repository::CustomerExt;
use crate::repository::prelude::*;
use crate::repository::scope::{CustomerReadScope, authorized_ids};
use crate::{Error, Result};

/// 客户查询与命令共用的统一授权接入。
#[derive(Clone)]
pub struct CustomerAccess {
    db: Database,
    service: AuthorizationService,
    current: AuthorizationService,
    mapping: Arc<dyn AccessMapping>,
}
impl CustomerAccess {
    /// 绑定客户仓储、固定资源登记与身份设施。
    /// # 参数
    /// db 为客户数据库，rbac 为应用共享身份服务。
    /// # 返回
    /// 不缓存跨请求授权的客户访问器。
    /// # 错误
    /// 固定登记非法时启动失败；不执行 I/O。
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        let mapping = CustomerMapping::shared(db.clone());
        Self {
            service: service(db.clone(), rbac.clone(), mapping.clone(), true),
            current: service(db.clone(), rbac, mapping.clone(), false),
            db,
            mapping,
        }
    }

    /// 返回身份域组织目录，用于业务筛选与责任事实。
    /// # 参数
    /// 无。
    /// # 返回
    /// 复用当前数据库的组织目录。
    /// # 错误
    /// 不执行 I/O。
    pub fn organization(&self) -> OrganizationDirectoryService {
        OrganizationDirectoryService::new(self.db.clone())
    }

    /// 复验本次查询凭证，保留当前资格与历史读取入口的差异。
    /// # 参数
    /// access 为本访问器生成的上下文，executor 为原执行器。
    /// # 返回
    /// 凭证仍有效时成功。
    /// # 错误
    /// 身份、映射、会话或有效期变化时拒绝。
    pub async fn validate(&self, access: &CustomerResolvedScope, executor: &mut dyn Executor) -> Result<()> {
        let compiled = access.compiled.as_ref().ok_or_else(|| Error::Internal("客户查询缺少凭证".into()))?;
        let service = if access.current { &self.current } else { &self.service };
        Ok(service.validate(compiled, executor).await?)
    }

    /// 在固定客户目录编译本阶段完整查询条件。
    async fn query_context(
        &self,
        actor: &AuditActor,
        action: &str,
        executor: &mut dyn Executor,
    ) -> Result<CustomerResolvedScope> {
        let current = action == "current";
        let permission = Permission::parse(format!("customer:{}", if current { "detail" } else { action }))?;
        let service = if current { &self.current } else { &self.service };
        let query = service.query(actor, &permission, AccessSubject::Direct, executor).await?;
        let compiled = query.compile(self.mapping.as_ref(), executor).await?;
        service.validate(&compiled, executor).await?;
        Ok(CustomerResolvedScope {
            user_id: actor.id().into(),
            current,
            scope_version: compiled.version(),
            as_of: Instant::now(),
            compiled: Some(Arc::new(compiled)),
        })
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
        let context = self.query_context(actor, action, executor).await?;
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
        Ok((context, CustomerReadScope { owned_customer_ids: owned }))
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
        let context = self.query_context(&actor, action, executor).await?;
        let compiled = context.compiled.as_ref().ok_or_else(|| Error::Internal("客户查询缺少凭证".into()))?;
        self.validate(&context, executor).await?;
        let ids = authorized_ids(&self.db, compiled, Some(customer_id), executor).await?;
        self.validate(&context, executor).await?;
        if ids.is_empty() {
            return Err(deny_object(action));
        }
        self.ensure_owner_facts(customer_id, context.as_of, executor).await?;
        self.validate(&context, executor).await?;
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
            self.organization().own_org(&owner.user_id, at, executor).await?;
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
        let permission = Permission::parse("customer:create")?;
        if !self.service.has_permission(actor, &permission, executor).await? {
            return Err(Error::Forbidden("没有创建客户权限".into()));
        }
        let at = Instant::now();
        self.organization()
            .own_org(actor.id(), at, executor)
            .await?
            .ok_or_else(|| Error::ValidationError("请先维护负责人的有效主属组织".into()))?;
        Ok(CustomerResolvedScope {
            user_id: actor.id().into(),
            current: false,
            scope_version: String::new(),
            as_of: at,
            compiled: None,
        })
    }
}
pub(crate) fn business_date(at: Instant) -> Result<BusinessDate> {
    let date = at.as_utc() + chrono::Duration::hours(8);
    Ok(date.format("%Y-%m-%d").to_string().parse()?)
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
