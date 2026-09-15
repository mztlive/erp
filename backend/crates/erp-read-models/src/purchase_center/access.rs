//! 采购单按当前采购负责人与单据业务组织授权，历史快照不提供访问资格。

use std::sync::Arc;

use application_core::AuditActor;
use erp_procurement::PurchaseResolvedScope;
use erp_procurement::entity::purchase_order::PurchaseOrder;
use erp_procurement::ports::PurchaseDataScopePort;
use erp_procurement::repository::PurchaseOrderExt;
use erp_procurement::repository::purchase_order::scope::PurchaseReadScope;
use erp_procurement::service::purchase_order::access::{
    PurchaseAccess as DomainPurchaseAccess, attach_history,
};
use erp_workflow::DocumentRegistryExt;
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
    /// 绑定采购集合与范围授权 Port。
    ///
    /// # 参数
    /// * `db` - 采购与参与事实所在数据库
    /// * `scope` - 组合层注入的采购范围 Port
    ///
    /// # 返回
    /// 返回无授权缓存的读取服务，构造不执行 I/O。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 不得在构造时补公司范围或读取登录人默认组织。
    pub fn new(db: Database, scope: Arc<dyn PurchaseDataScopePort>) -> Self {
        Self { inner: DomainPurchaseAccess::new(db.clone(), scope), db }
    }

    /// 在独立事务中重验详情权限和当前责任，返回业务版本绑定的范围版本。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
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
                    let (context, scope) = this.resolve(&actor, "detail", executor).await?;
                    let order = this
                        .db
                        .purchase_orders()
                        .find_authorized(&id, &scope, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("采购单不存在或无权查看".into()))?;
                    let version =
                        format!("{}:{}:{}", context.scope_version, order.base.id, order.base.version);
                    Ok((order, version))
                })
            })
            .await
    }

    /// 在调用方事务内证明范围并读取当前可操作单据。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `action` - 已注册的采购动作
    /// * `id` - 采购单主键
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 对象在范围内时返回采购单。
    ///
    /// # 错误
    /// 读取动作对不可见对象返回 NotFound；写动作同样不泄露存在性。
    ///
    /// # 关键业务约束
    /// 写命令必须在原领域事务内调用；历史参与不授予修改。
    pub async fn require_object(
        &self,
        actor: &AuditActor,
        action: &str,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<PurchaseOrder> {
        self.inner.require_object(actor, action, id, &[], executor).await.map_err(crate::Error::from)
    }

    /// 新单使用即将持久化的显式责任解释创建与提交范围。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人，通常写入为采购负责人
    /// * `order` - 拟持久化的采购单
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 创建与提交范围同时覆盖责任事实时成功。
    ///
    /// # 错误
    /// 无创建或提交动作、范围为空或责任不在范围内时拒绝。
    ///
    /// # 关键业务约束
    /// 创建并提交必须由同一角色同时提供两个动作；不得由创建人审计字段兜底。
    pub async fn ensure_create_and_submit(
        &self,
        actor: &AuditActor,
        order: &PurchaseOrder,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.inner.ensure_create_and_submit(actor, order, executor).await.map_err(crate::Error::from)
    }

    /// 按资源动作证明范围，读取动作附加合法历史参与。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `action` - 已注册的采购动作
    /// * `executor` - 调用方执行器
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
        action: &str,
        executor: &mut dyn Executor,
    ) -> Result<(PurchaseResolvedScope, PurchaseReadScope)> {
        let (mut access, mut scope) = self.inner.resolve(actor, action, executor).await?;
        if allows_history(action) {
            let history = self.participant_orders(actor.id(), executor).await?;
            attach_history(&mut access, &mut scope, history);
        }
        Ok((access, scope))
    }

    /// 将已证明范围编译为来源采购单 ID 限制。
    ///
    /// # 参数
    /// * `scope` - 已证明的采购对象范围
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// `None` 表示公司范围不限制来源单；`Some` 为必须命中的来源单集合。
    ///
    /// # 错误
    /// 超过查询上限时整体拒绝。
    ///
    /// # 关键业务约束
    /// 变更单和退货必须沿原采购单责任接入，不得另建平行对象集合。
    pub async fn authorized_source_ids(
        &self,
        scope: &PurchaseReadScope,
        executor: &mut dyn Executor,
    ) -> Result<Option<Vec<String>>> {
        self.inner.authorized_source_ids(scope, executor).await.map_err(crate::Error::from)
    }

    /// 完整读取动作已由身份域证明，参与事实独立补充读取并继续受个人上限约束。
    ///
    /// # 参数
    /// * `user` - 当前账号
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回有界的历史参与单据 ID。
    ///
    /// # 错误
    /// 超过查询上限时整体拒绝。
    ///
    /// # 关键业务约束
    /// 不得由历史业绩快照推导参与资格。
    async fn participant_orders(&self, user: &str, executor: &mut dyn Executor) -> Result<Vec<String>> {
        let ids = self.db.document_participants().document_ids_by_user(user, executor).await?;
        if ids.len() > 10_000 {
            return Err(Error::ValidationError("历史参与范围超过查询上限".into()));
        }
        Ok(ids)
    }
}

/// 参与关系仅补充采购单明确登记的读取动作，不能为创建或状态迁移提供资格。
///
/// # 参数
/// * `action` - 本次解析的采购动作
///
/// # 返回
/// 列表和详情允许历史参与，写动作不允许。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 历史参与不提供修改、审批或资金操作权限。
fn allows_history(action: &str) -> bool {
    matches!(action, "list" | "detail")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn historical_participation_never_grants_commands() {
        for action in ["list", "detail"] {
            assert!(allows_history(action));
        }
        for action in ["create", "update", "submit", "cancel_approval", "delete", "transfer", "*"] {
            assert!(!allows_history(action));
        }
    }
}
