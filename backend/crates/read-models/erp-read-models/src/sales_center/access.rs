//! 销售事实映射与 identity 授权入口；业务不解释权限条款。
mod registry;
use std::collections::BTreeSet;
use std::sync::Arc;

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_identity::entity::organization::OrgTree;
use erp_identity::repository::OrganizationRepository;
use erp_identity::{
    AccessTarget, AuthorizationService, CompiledAccess, Permission, ResourceTargets, SharedRbacService,
};
use erp_sales::entity::sales_order::SalesOrder;
use erp_sales::repository::prelude::*;
use erp_sales::repository::sales_order::scope::SalesReadScope;
use erp_sales::repository::{SalesOrderExt, SalesReviewExt};
use mongodb::Database;
use persistence_core::{Executor, Transactional};
pub(crate) use registry::approval_permissions as approval_source_permissions;

use super::repository::authorization::LoadedSales;
use crate::{Error, Result};

/// 当前读取阶段的展示版本和不可伪造凭证。
#[derive(Clone)]
pub struct SalesReadContext {
    pub scope_version: String,
    pub as_of: Instant,
    pub compiled: Arc<CompiledAccess>,
    authorization: AuthorizationService,
}
impl SalesReadContext {
    /// 组合业务版本，不改变 identity 凭证。
    /// # 参数
    /// map 为业务版本组合器。
    /// # 返回
    /// 无。
    /// # 错误
    /// 无。
    pub fn map_read_version(&mut self, map: impl FnOnce(&str) -> String) {
        self.scope_version = map(&self.scope_version);
    }
    /// 在原读取执行器复验身份和全部映射依赖。
    /// # 参数
    /// executor 为本次读取执行器。
    /// # 返回
    /// 仍有效时成功。
    /// # 错误
    /// 返回 identity 的原始分类。
    pub async fn validate(&self, executor: &mut dyn Executor) -> Result<()> {
        self.authorization.validate(&self.compiled, executor).await?;
        Ok(())
    }
}

/// 仅提供业务事实；全部操作和数据判断委托 identity。
#[derive(Clone)]
pub struct SalesAccess {
    db: Database,
    rbac: SharedRbacService,
}
impl SalesAccess {
    /// 绑定销售及历史参与事实所在数据库和共享身份设施。
    /// # 参数
    /// db 为数据库，rbac 为身份设施。
    /// # 返回
    /// 无缓存访问器。
    /// # 错误
    /// 无。
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self { db, rbac }
    }

    /// 读取并验证完整销售对象。
    /// # 参数
    /// actor 为操作人，id 为销售单身份。
    /// # 返回
    /// 销售单及本次身份和业务版本。
    /// # 错误
    /// 身份、权限、对象或数据库错误原样返回。
    pub async fn detail(&self, actor: &AuditActor, id: &str) -> Result<(SalesOrder, String)> {
        let this = self.clone();
        let actor = actor.clone();
        let id = id.to_string();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let (context, scope) = this.resolve(&actor, "detail", &[], executor).await?;
                    let order = this
                        .db
                        .sales_orders()
                        .find_authorized(&id, &scope, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("销售单不存在或无权查看".into()))?;
                    context.validate(executor).await?;
                    let version =
                        format!("{}:{}:{}", context.scope_version, order.base.id, order.base.version);
                    Ok((order, version))
                })
            })
            .await
    }
    /// 生成完整销售查询凭证。
    /// # 参数
    /// actor 为当前操作人，action 为固定登记动作，permissions 为同角色附加功能权限，executor 为原执行器。
    /// # 返回
    /// 展示上下文及仓储执行载体。
    /// # 错误
    /// 未登记组合或授权失效返回错误。
    pub async fn resolve(
        &self,
        actor: &AuditActor,
        action: &str,
        permissions: &[Permission],
        executor: &mut dyn Executor,
    ) -> Result<(SalesReadContext, SalesReadScope)> {
        self.query(actor, "sales_order", action, permissions, false, executor).await
    }
    /// 按固定消费资源登记在销售事实模型上证明范围。
    /// # 参数
    /// actor、resource、action 为固定入口；permissions 为附加同角色功能权限；executor 为原执行器。
    /// # 返回
    /// 完整授权的销售行读取约束。
    /// # 错误
    /// 未登记入口及身份错误拒绝。
    pub async fn resolve_resource(
        &self,
        actor: &AuditActor,
        resource: &str,
        action: &str,
        permissions: &[Permission],
        executor: &mut dyn Executor,
    ) -> Result<(SalesReadContext, SalesReadScope)> {
        self.query(actor, resource, action, permissions, false, executor).await
    }
    /// 为固定业务入口读取实际销售来源；不额外要求来源详情功能权限。
    /// # 参数
    /// actor 为当前账号，entry 为服务端固定入口，executor 为原执行器。
    /// # 返回
    /// 来源销售的完整读取凭证。
    /// # 错误
    /// 未登记来源入口及授权错误返回原始分类。
    pub async fn resolve_source(
        &self,
        actor: &AuditActor,
        entry: &Permission,
        executor: &mut dyn Executor,
    ) -> Result<(SalesReadContext, SalesReadScope)> {
        self.query(actor, entry.resource(), entry.action(), &[], true, executor).await
    }
    async fn query(
        &self,
        actor: &AuditActor,
        resource: &str,
        action: &str,
        permissions: &[Permission],
        source: bool,
        executor: &mut dyn Executor,
    ) -> Result<(SalesReadContext, SalesReadScope)> {
        let (authorization, mapping, subject) =
            registry::service(&self.db, &self.rbac, resource, action, permissions, source)?;
        let permission = Permission::parse(format!("{resource}:{action}"))?;
        let query = authorization.query(actor, &permission, subject, executor).await?;
        let compiled = Arc::new(query.compile(mapping.as_ref(), executor).await?);
        authorization.validate(&compiled, executor).await?;
        let context = SalesReadContext {
            scope_version: compiled.version(),
            as_of: Instant::now(),
            compiled: compiled.clone(),
            authorization: authorization.clone(),
        };
        Ok((context, SalesReadScope::from_compiled(compiled, authorization)))
    }
    /// 在当前写事务验证完整销售对象；状态校验由业务执行。
    /// # 参数
    /// actor、action、id 为实际操作目标；permissions 为固定同角色附加权限；executor 为原事务。
    /// # 返回
    /// 已授权的完整销售单。
    /// # 错误
    /// 身份、对象或读取错误返回原始分类。
    pub async fn require_object(
        &self,
        actor: &AuditActor,
        action: &str,
        id: &str,
        permissions: &[Permission],
        executor: &mut dyn Executor,
    ) -> Result<SalesOrder> {
        let (authorization, _, subject) =
            registry::service(&self.db, &self.rbac, "sales_order", action, permissions, false)?;
        authorization
            .require(
                actor,
                &Permission::parse(format!("sales_order:{action}"))?,
                AccessTarget::Records { subject, ids: ResourceTargets::new(&[id])? },
                executor,
            )
            .await?;
        self.db
            .sales_orders()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("销售单不存在或无权操作".into()))
    }
    /// 检查本次命令已经确定的完整销售责任事实。
    /// # 参数
    /// actor、action 为当前操作，order 为服务端组装的完整候选，permissions 为固定同角色权限，executor 为原事务。
    /// # 返回
    /// 候选事实满足 identity 政策时成功。
    /// # 错误
    /// 授权、事实或存储错误原样返回。
    pub async fn require_loaded(
        &self,
        actor: &AuditActor,
        action: &str,
        order: &SalesOrder,
        permissions: &[Permission],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let facts = Arc::new(LoadedSales(order.clone()));
        let (service, _, subject) = registry::service_facts(
            &self.db,
            &self.rbac,
            "sales_order",
            action,
            permissions,
            false,
            Some(facts),
        )?;
        service
            .require(
                actor,
                &Permission::parse(format!("sales_order:{action}"))?,
                AccessTarget::Records { subject, ids: ResourceTargets::new(&[&order.base.id])? },
                executor,
            )
            .await?;
        Ok(())
    }
    /// 以业务入口检查本次完整来源集合。
    /// # 参数
    /// actor 为当前账号，entry 为固定入口，ids 为本次来源目标，executor 为原事务。
    /// # 返回
    /// 所有来源有权时成功。
    /// # 错误
    /// 缺失、无权或过量目标返回错误。
    pub async fn require_source(
        &self,
        actor: &AuditActor,
        entry: &Permission,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.require_source_permissions(actor, entry, ids, &[], executor).await
    }
    /// 按固定审批管理组合在同一次来源授权中证明同角色完整资格。
    /// # 参数
    /// actor、entry、ids 为当前真实入口和完整销售来源；required 为服务端审批政策要求，executor 为原事务。
    /// # 返回
    /// 同一角色同时满足全部功能权限和来源范围时成功。
    /// # 错误
    /// 未登记权限组合、无权、目标缺失或存储错误均失败。
    pub async fn require_source_permissions(
        &self,
        actor: &AuditActor,
        entry: &Permission,
        ids: &[String],
        required: &[Permission],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let (service, _, subject) =
            registry::service(&self.db, &self.rbac, entry.resource(), entry.action(), required, true)?;
        let ids = ids.iter().map(String::as_str).collect::<Vec<_>>();
        service
            .require(
                actor,
                entry,
                AccessTarget::Records { subject, ids: ResourceTargets::new(&ids)? },
                executor,
            )
            .await?;
        Ok(())
    }
    /// 在原读取阶段复验已编译的销售凭证。
    /// # 参数
    /// context 为原凭证，executor 为原执行器。
    /// # 返回
    /// 有效时成功。
    /// # 错误
    /// 版本或授权失效拒绝。
    pub async fn validate(&self, context: &SalesReadContext, executor: &mut dyn Executor) -> Result<()> {
        context.validate(executor).await
    }
    /// 展开请求中的内部组织及其可选有效下级。
    ///
    /// # 参数
    /// * `org_unit_ids` - 请求中的组织 ID
    /// * `include_descendants` - 是否包含有效下级
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回启用节点的组织 ID 集合。
    ///
    /// # 错误
    /// 未知组织或组织树非法时拒绝。组织状态读取失败会返回对应错误。
    ///
    /// # 关键业务约束
    /// 筛选只能收窄授权结果，不得忽略未知组织；停用分支不贡献范围。
    pub async fn expand_org_units(
        &self,
        org_unit_ids: &[String],
        include_descendants: bool,
        executor: &mut dyn Executor,
    ) -> Result<BTreeSet<String>> {
        let state = OrganizationRepository::new(&self.db).state(executor).await?;
        let tree = OrgTree::new(&state.units)?;
        let mut expanded = BTreeSet::new();
        for id in org_unit_ids {
            expanded.extend(tree.expand(id, include_descendants)?);
        }
        Ok(expanded)
    }

    /// 独立事务中重验销售单据附件资格。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `action` - 已注册的销售动作
    /// * `document_id` - 业务单据主键
    ///
    /// # 返回
    /// 销售单据在范围内或其他单据类型时成功。
    ///
    /// # 错误
    /// 销售单据不可见时返回 `NotFound`。事务或仓储失败会返回对应错误。
    ///
    /// # 关键业务约束
    /// 列表已授权不能作为附件或打印凭证。
    pub async fn require_attachment(
        &self,
        actor: &AuditActor,
        action: &str,
        document_id: &str,
    ) -> Result<()> {
        let this = self.clone();
        let actor = actor.clone();
        let action = action.to_string();
        let document_id = document_id.to_string();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    this.require_attached_document(&actor, &action, &document_id, executor).await
                })
            })
            .await
    }

    /// 附件与独立打印消费者按单据类型重验销售范围。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `action` - 已注册的销售读取或写动作
    /// * `document_id` - 业务单据主键
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 销售单或销售变更单在范围内时成功；其他单据类型原样放行给各自消费者。
    ///
    /// # 错误
    /// 销售单据不可见时返回 `NotFound`。单据读取失败会返回对应错误。
    ///
    /// # 关键业务约束
    /// 变更单沿来源销售单重验；不得用附件列表授权代替对象范围。
    pub async fn require_attached_document(
        &self,
        actor: &AuditActor,
        action: &str,
        document_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let sales_order = self.db.sales_orders().find_by_id(document_id, executor).await?;
        let change_source = if sales_order.is_none() {
            self.db
                .sales_change_orders()
                .find_by_id(document_id, executor)
                .await?
                .map(|change| change.sales_order_id)
        } else {
            None
        };
        let Some(object_id) = attached_sales_object_id(
            sales_order.as_ref().map(|_| document_id),
            change_source.as_ref().map(|id| id.as_ref()),
        ) else {
            return Ok(());
        };
        self.require_object(actor, action, object_id, &[], executor).await?;
        Ok(())
    }
}
fn attached_sales_object_id<'a>(sales: Option<&'a str>, change: Option<&'a str>) -> Option<&'a str> {
    sales.or(change)
}
