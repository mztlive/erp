//! 采购单按当前采购负责人与单据业务组织授权，历史快照不提供访问资格。

use std::sync::Arc;

use application_core::AuditActor;
use erp_permissions::{Permission, purchase_order as purchase_order_permissions};
use erp_procurement::PurchaseResolvedScope;
use erp_procurement::entity::purchase_order::PurchaseOrder;
use erp_procurement::ports::PurchaseDataScopePort;
use erp_procurement::repository::PurchaseOrderExt;
use erp_procurement::repository::prelude::*;
use erp_procurement::repository::purchase_order::scope::PurchaseReadScope;
use erp_procurement::service::purchase_order::access::PurchaseAccess as DomainPurchaseAccess;
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use crate::{Error, Result};

/// 采购对象读取范围；供列表、详情、候选、导出、变更／退货和写命令复用。
#[derive(Clone)]
pub struct PurchaseAccess {
    db: Database,
    inner: DomainPurchaseAccess,
}

impl PurchaseAccess {
    /// 装配采购中心时，绑定采购集合与范围授权 Port。
    ///
    /// # 参数
    /// * `db` - 采购与参与事实所在数据库
    /// * `scope` - 组合层注入的采购范围 Port
    ///
    /// # 返回
    /// 返回无授权缓存的读取服务，构造不执行 I/O。
    ///
    /// # 关键业务约束
    /// 不得在构造时补公司范围或读取登录人默认组织。
    pub fn new(db: Database, scope: Arc<dyn PurchaseDataScopePort>) -> Self {
        Self { inner: DomainPurchaseAccess::new(db.clone(), scope), db }
    }

    /// 读取采购中心时，在独立事务中重验详情权限和当前责任，返回业务版本绑定的范围版本。
    ///
    /// # 参数
    /// * `id` - 采购单主键
    ///
    /// # 返回
    /// 返回已授权订单及范围版本；历史业绩不提供访问资格。
    ///
    /// # 错误
    /// 账号、动作或对象资格失效时拒绝；不可见订单与不存在订单均返回 NotFound。
    ///
    /// # 关键业务约束
    /// 列表已授权不能作为详情请求的长期凭证。
    pub async fn detail(&self, actor: &AuditActor, id: &str) -> Result<(PurchaseOrder, String)> {
        let this = self.clone();
        let actor = actor.clone();
        let id = id.to_string();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let (order, version, scope) = this.resolve_detail(&actor, &id, executor).await?;
                    scope.validate(executor).await?;
                    Ok((order, version))
                })
            })
            .await
    }

    /// 确定采购中心的适用条件时，在调用方执行器内证明详情资格，包含详情允许的有界历史参与。
    ///
    /// # 参数
    /// * `actor` / `id` - 当前账号及采购单身份。
    /// * `executor` - 调用方执行器；文件目录关系证明须使用同一事务。
    ///
    /// # 返回
    /// 返回授权采购单、绑定范围和对象版本的服务器校验标识及本阶段凭证。
    /// 调用方必须在全部关联读取完成后 validate，再交付视图或开始下一授权阶段。
    ///
    /// # 错误
    /// 账号、动作、范围或对象资格无效时拒绝，不泄露不可见对象存在性。
    pub async fn resolve_detail(
        &self,
        actor: &AuditActor,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<(PurchaseOrder, String, PurchaseReadScope)> {
        let (context, scope) = self.resolve(actor, &purchase_order_permissions::DETAIL, executor).await?;
        let order = self
            .db
            .purchase_orders()
            .find_authorized(id, &scope, executor)
            .await?
            .ok_or_else(|| Error::NotFound("采购单不存在或无权查看".into()))?;

        let version = format!("{}:{}:{}", context.scope_version, order.base.id, order.base.version);
        Ok((order, version, scope))
    }

    /// 依据资源动作证明范围，读取动作附加合法历史参与。
    ///
    /// # 参数
    /// * `entry` - 中央登记的采购权限常量
    ///
    /// # 返回
    /// 返回身份上下文和采购责任条件。
    ///
    /// # 错误
    /// 未注册资源、失效动作、不支持维度或查询超限均拒绝。
    ///
    /// # 关键业务约束
    /// 只接受内部组织维度；仓库与结算主体必须拒绝，不得并入部门条件。
    pub async fn resolve(
        &self,
        actor: &AuditActor,
        entry: &Permission,
        executor: &mut dyn Executor,
    ) -> Result<(PurchaseResolvedScope, PurchaseReadScope)> {
        self.inner.resolve(actor, entry, executor).await.map_err(crate::Error::from)
    }
}
