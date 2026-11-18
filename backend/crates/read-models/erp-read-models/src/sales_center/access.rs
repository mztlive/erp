//! 销售事实映射与 identity 授权入口；业务不解释权限条款。
use std::collections::BTreeSet;
use std::sync::Arc;

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_identity::entity::organization::OrgTree;
use erp_identity::repository::OrganizationRepository;
use erp_identity::{AccessTarget, AuthorizationService, CompiledAccess};
use erp_permissions::{
    Permission as AccessPermission, internal as internal_permissions, sales_order as sales_order_permissions,
    sales_order,
};
use erp_sales::entity::sales_order::SalesOrder;
use erp_sales::repository::prelude::*;
use erp_sales::repository::sales_order::scope::SalesReadScope;
use erp_sales::repository::{SalesOrderExt, SalesReviewExt};
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use super::repository::authorization::sales_facts;
use crate::{Error, Result};

/// 当前读取阶段的展示版本和不可伪造凭证。
#[derive(Clone)]
pub struct SalesReadContext {
    pub scope_version: String,
    pub as_of: Instant,
    access: Arc<CompiledAccess>,
    authorization: Arc<AuthorizationService>,
}
impl SalesReadContext {
    // 生成销售对象范围视图时，记录同次解析的根交付凭证。
    //
    // 调用参数：
    // access 为原始根凭证，authorization 为共享服务。
    //
    // 展示元信息及最终交付检查载体。
    //
    // 失败边界：
    // 无；交付时复验原根凭证。
    pub(crate) fn from_access(access: Arc<CompiledAccess>, authorization: Arc<AuthorizationService>) -> Self {
        Self { scope_version: access.version(), as_of: access.as_of(), access, authorization }
    }
    /// 生成销售对象范围视图时，组合业务版本，不改变 identity 凭证。
    ///
    /// # 参数
    /// map 为业务版本组合器。
    pub fn map_read_version(&mut self, map: impl FnOnce(&str) -> String) {
        self.scope_version = map(&self.scope_version);
    }
    /// 销售对象读取和展示补齐完成后，沿签发凭证的原执行器复验身份和全部映射依赖。
    ///
    /// # 参数
    /// executor 为本次读取执行器。
    ///
    /// # 返回
    /// 仍有效时成功。
    ///
    /// # 错误
    /// 返回 identity 的原始分类。
    pub async fn validate(&self, executor: &mut dyn Executor) -> Result<()> {
        self.authorization.validate(&self.access, executor).await?;
        Ok(())
    }
    // 核对销售对象范围边界时，核验边界及期限，不结束本次读取阶段。
    //
    // 调用参数：
    // executor 为原执行器。
    //
    // 本阶段仍有效时成功。
    //
    // 失败边界：
    // 跨阶段或到期拒绝。
    pub(crate) fn ensure_current(&self, executor: &mut dyn Executor) -> Result<()> {
        self.access.ensure_current(executor)?;
        Ok(())
    }
}

/// 仅提供业务事实；全部操作和数据判断委托 identity。
#[derive(Clone)]
pub struct SalesAccess {
    db: Database,
    rbac: Arc<AuthorizationService>,
}
impl SalesAccess {
    /// 装配销售对象范围时，绑定销售及历史参与事实所在数据库和共享身份设施。
    ///
    /// # 参数
    /// db 为数据库，rbac 为身份设施。
    ///
    /// # 返回
    /// 无缓存访问器。
    pub fn new(db: Database, rbac: Arc<AuthorizationService>) -> Self {
        Self { db, rbac }
    }

    /// 处理销售对象范围时，读取并验证完整销售对象。
    ///
    /// # 参数
    /// actor 为操作人，id 为销售单身份。
    ///
    /// # 返回
    /// 销售单及本次身份和业务版本。
    ///
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
                    let (context, scope) =
                        this.resolve(&actor, &sales_order_permissions::DETAIL, executor).await?;
                    let order = this
                        .db
                        .sales_orders()
                        .find_authorized(&id, &scope, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("销售单不存在或无权查看".into()))?;
                    let version =
                        format!("{}:{}:{}", context.scope_version, order.base.id, order.base.version);
                    context.validate(executor).await?;
                    Ok((order, version))
                })
            })
            .await
    }
    /// 确定销售对象范围的适用条件时，生成完整销售查询凭证。
    ///
    /// # 参数
    /// actor 为当前操作人，permission 为公共权限常量，executor 为原执行器。
    ///
    /// # 返回
    /// 展示上下文及仓储执行载体。
    ///
    /// # 错误
    /// 未登记组合或授权失效返回错误。
    pub async fn resolve(
        &self,
        actor: &AuditActor,
        permission: &AccessPermission,
        executor: &mut dyn Executor,
    ) -> Result<(SalesReadContext, SalesReadScope)> {
        ensure_sales_permission(permission)?;
        let compiled = Arc::new(self.rbac.query(actor, permission, executor).await?);
        let scope = SalesReadScope::from_compiled(compiled.clone(), self.rbac.clone());
        Ok((SalesReadContext::from_access(compiled, self.rbac.clone()), scope))
    }
    /// 确定销售对象范围的适用条件时，在当前写事务验证完整销售对象；状态校验由业务执行。
    ///
    /// # 参数
    /// actor、permission、id 为实际操作目标；executor 为原事务。
    ///
    /// # 返回
    /// 已授权的完整销售单。
    ///
    /// # 错误
    /// 身份、对象或读取错误返回原始分类。
    pub async fn require_object(
        &self,
        actor: &AuditActor,
        permission: &AccessPermission,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<SalesOrder> {
        ensure_sales_permission(permission)?;
        self.rbac.require(actor, permission, AccessTarget::record(id)?, executor).await?;
        self.db
            .sales_orders()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("销售单不存在或无权操作".into()))
    }
    /// 确定销售对象范围的适用条件时，检查本次命令已经确定的完整销售责任事实。
    ///
    /// # 参数
    /// actor、permission 为当前操作，order 为服务端组装的完整候选，executor 为原事务。
    ///
    /// # 返回
    /// 候选事实满足 identity 政策时成功。
    ///
    /// # 错误
    /// 授权、事实或存储错误原样返回。
    pub async fn require_loaded(
        &self,
        actor: &AuditActor,
        permission: &AccessPermission,
        order: &SalesOrder,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        ensure_sales_permission(permission)?;
        let mut candidate = sales_facts(order);
        candidate.version.clear();
        if permission == &internal_permissions::SALES_ORDER_HANDOVER_RECIPIENT {
            candidate.id.clear();
        }
        let target = if permission == &sales_order::CREATE
            || permission == &sales_order::SUBMIT
            || permission == &internal_permissions::SALES_CREATE_AND_SUBMIT
        {
            AccessTarget::Candidate(candidate)
        } else if permission == &sales_order::HANDOVER {
            AccessTarget::OwnershipChange { id: order.base.id.clone(), candidate }
        } else if permission == &internal_permissions::SALES_ORDER_HANDOVER_RECIPIENT {
            AccessTarget::Candidate(candidate)
        } else {
            AccessTarget::record(&order.base.id)?
        };

        self.rbac.require(actor, permission, target, executor).await?;
        Ok(())
    }
    /// 准备销售对象范围候选时，展开请求中的内部组织及其可选有效下级。
    ///
    /// # 参数
    /// * `org_unit_ids` - 请求中的组织 ID
    /// * `include_descendants` - 是否包含有效下级
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

    /// 确定销售对象范围的适用条件时，独立事务中重验销售单据附件资格。
    ///
    /// # 参数
    /// * `permission` - 固定销售权限常量
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
        permission: &AccessPermission,
        document_id: &str,
    ) -> Result<()> {
        let this = self.clone();
        let actor = actor.clone();
        let permission = permission.clone();
        let document_id = document_id.to_string();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    this.require_attached_document(&actor, &permission, &document_id, executor).await
                })
            })
            .await
    }

    /// 确定销售对象范围的适用条件时，附件与独立打印消费者按单据类型重验销售范围。
    ///
    /// # 参数
    /// * `permission` - 固定销售读取或写权限常量
    /// * `document_id` - 业务单据主键
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
        permission: &AccessPermission,
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
        let Some(object_id) = sales_order
            .as_ref()
            .map(|_| document_id)
            .or_else(|| change_source.as_ref().map(|id| id.as_ref()))
        else {
            return Ok(());
        };
        self.require_object(actor, permission, object_id, executor).await?;
        Ok(())
    }
}

pub use super::repository::authorization::{SalesFacts, SalesMapping};

// 核对销售对象范围边界时，访问器只解释销售根，其他资源的许可不能用来读取同名销售身份。
fn ensure_sales_permission(permission: &AccessPermission) -> Result<()> {
    if permission.resource() != sales_order::DETAIL.resource() {
        return Err(Error::ValidationError("销售访问器收到其他资源权限".into()));
    }
    Ok(())
}

#[cfg(test)]
mod permission_tests {
    use erp_permissions::customer;

    use super::*;

    #[test]
    fn sales_root_rejects_another_resources_permission() {
        assert!(ensure_sales_permission(&sales_order::DETAIL).is_ok());
        assert!(ensure_sales_permission(&internal_permissions::PURCHASE_EXPENSE_SOURCE).is_ok());
        assert!(matches!(ensure_sales_permission(&customer::DETAIL), Err(Error::ValidationError(_))));
    }
}
