//! 采购对象读取与写入范围；当前采购负责人和单据业务组织分别解释。

use std::hash::{Hash, Hasher};
use std::sync::Arc;

use application_core::AuditActor;
use mongodb::Database;
use persistence_core::Executor;

use crate::entity::purchase_order::PurchaseOrder;
use crate::error::{Error, Result};
use crate::ports::{PurchaseDataScopePort, PurchaseResolvedClause, PurchaseResolvedScope};
use crate::repository::purchase_order::scope::{PurchaseReadScope, PurchaseScopeClause};
use crate::repository::PurchaseOrderExt;

/// 采购对象访问范围；列表、详情、候选、导出、变更／退货和写命令复用同一解析。
#[derive(Clone)]
pub struct PurchaseAccess {
    db: Database,
    scope: Arc<dyn PurchaseDataScopePort>,
}

impl PurchaseAccess {
    /// 绑定采购集合与范围授权 Port。
    ///
    /// # 参数
    /// * `db` - 采购集合所在数据库
    /// * `scope` - 组合层注入的采购范围 Port
    ///
    /// # 返回
    /// 返回无授权缓存的访问服务，构造不执行 I/O。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 不得在构造时补公司范围或读取登录人默认组织；不得直接构造身份域 Service。
    pub fn new(db: Database, scope: Arc<dyn PurchaseDataScopePort>) -> Self {
        Self { db, scope }
    }

    /// 在调用方事务内证明资源动作并映射采购责任条件。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `action` - 已注册的采购动作
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回已解析范围事实和采购授权条件。
    ///
    /// # 错误
    /// 无动作权限返回 Forbidden；未装配或查询超限时拒绝。
    ///
    /// # 关键业务约束
    /// 当前责任按采购负责人和 `business_org_unit_id` 解释；历史参与须由读取入口另行附加。
    pub async fn resolve(
        &self,
        actor: &AuditActor,
        action: &str,
        executor: &mut dyn Executor,
    ) -> Result<(PurchaseResolvedScope, PurchaseReadScope)> {
        let access = self.scope.resolve(actor, action, executor).await?;
        let scope = purchase_scope(&access, actor.id(), Vec::new());
        Ok((access, scope))
    }

    /// 在调用方事务内证明同角色完整权限集合并映射采购责任条件。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `action` - 已注册的采购动作
    /// * `permissions` - 同角色必须同时持有的额外权限码
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回已解析范围事实和采购授权条件。
    ///
    /// # 错误
    /// 无动作权限返回 Forbidden；权限码非法或未装配时拒绝。
    ///
    /// # 关键业务约束
    /// 创建并提交必须由同一角色同时提供两个动作；历史参与不授予写资格。
    pub async fn resolve_permissions(
        &self,
        actor: &AuditActor,
        action: &str,
        permissions: &[String],
        executor: &mut dyn Executor,
    ) -> Result<(PurchaseResolvedScope, PurchaseReadScope)> {
        let access = self
            .scope
            .resolve_permissions(actor, action, permissions, executor)
            .await?;
        let scope = purchase_scope(&access, actor.id(), Vec::new());
        Ok((access, scope))
    }

    /// 在调用方事务内重验对象资格。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `action` - 已注册的采购动作
    /// * `id` - 采购单主键
    /// * `permissions` - 同角色必须同时持有的额外权限码
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 对象在范围内时返回采购单。
    ///
    /// # 错误
    /// 读取或写动作对不可见对象均返回 NotFound，不泄露存在性。
    ///
    /// # 关键业务约束
    /// 写命令必须在原领域事务内调用；历史参与不授予修改。
    pub async fn require_object(
        &self,
        actor: &AuditActor,
        action: &str,
        id: &str,
        permissions: &[String],
        executor: &mut dyn Executor,
    ) -> Result<PurchaseOrder> {
        let (_, scope) = if permissions.is_empty() {
            self.resolve(actor, action, executor).await?
        } else {
            self.resolve_permissions(actor, action, permissions, executor)
                .await?
        };
        self.db
            .purchase_orders()
            .find_authorized(id, &scope, executor)
            .await?
            .ok_or_else(|| Error::NotFound("采购单不存在或无权操作".into()))
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
        let submit = "purchase_order:submit".to_string();
        let create = "purchase_order:create".to_string();
        let (_, mut scope) = self
            .resolve_permissions(actor, "create", std::slice::from_ref(&submit), executor)
            .await?;
        let (_, submit_scope) = self
            .resolve_permissions(actor, "submit", std::slice::from_ref(&create), executor)
            .await?;
        scope.required_scopes.push(submit_scope);
        let owner = order.current_owner_user_id()?;
        if !scope.allows_creation(owner, &order.business_org_unit_id) {
            return Err(Error::Forbidden("没有该业务责任范围的采购建单权限".into()));
        }
        Ok(())
    }

    /// 将已证明范围编译为来源采购单 ID 限制，供变更单和退货沿原单接入。
    ///
    /// # 参数
    /// * `scope` - 已证明的采购对象范围
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// `None` 表示公司范围不限制来源单；`Some` 为必须命中的来源单集合，空集表示无可见对象。
    ///
    /// # 错误
    /// 超过查询上限时整体拒绝。
    ///
    /// # 关键业务约束
    /// 不得把仓库 ID 与部门 ID 放入同一并集；缺范围保持空集。
    pub async fn authorized_source_ids(
        &self,
        scope: &PurchaseReadScope,
        executor: &mut dyn Executor,
    ) -> Result<Option<Vec<String>>> {
        if scope.is_empty() {
            return Ok(Some(Vec::new()));
        }
        if scope.is_company() {
            return Ok(None);
        }
        let ids = self
            .db
            .purchase_orders()
            .list_authorized_ids(scope, executor)
            .await?;
        if ids.len() > 10_000 {
            return Err(Error::ValidationError(
                "采购单查询超过上限，请收窄组织或负责人条件".into(),
            ));
        }
        Ok(Some(ids))
    }
}

/// 把合法历史参与附加到已解析的读取范围，并纳入跨页版本。
///
/// # 参数
/// * `access` - 身份域已解析授权
/// * `scope` - 本域对象条件
/// * `history` - 合法历史参与采购单
///
/// # 返回
/// 无。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 历史参与只补充读取，且继续受个人上限约束；不得用于创建或状态迁移。
pub fn attach_history(
    access: &mut PurchaseResolvedScope,
    scope: &mut PurchaseReadScope,
    history: Vec<String>,
) {
    let mut fingerprint = std::collections::hash_map::DefaultHasher::new();
    history.hash(&mut fingerprint);
    access.scope_version = format!("{}:{:x}", access.scope_version, fingerprint.finish());
    scope.historical_order_ids = history;
}

/// 映射同角色正向范围和独立个人上限，保持两者的交集关系。
///
/// # 参数
/// * `access` - Port 返回的已解析授权
/// * `user` - 当前账号
/// * `history` - 合法历史参与单据
///
/// # 返回
/// 返回采购仓储条件。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 协作范围不映射为采购对象；本人负责解释当前采购负责人。
fn purchase_scope(
    access: &PurchaseResolvedScope,
    user: &str,
    history: Vec<String>,
) -> PurchaseReadScope {
    PurchaseReadScope {
        required_scopes: vec![],
        historical_order_ids: history,
        roles: access
            .role_clauses
            .iter()
            .map(|clause| map_clause(clause, user))
            .collect(),
        user_limit: access
            .user_limit
            .as_ref()
            .map(|clause| map_clause(clause, user)),
    }
}

/// 仅接受已解析的内部组织范围，不读取历史业绩或仓库作为授权。
///
/// # 参数
/// * `scope` - Port 正向范围
/// * `actor` - 当前账号
///
/// # 返回
/// 返回采购责任条款。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 不得把仓库 ID 与部门 ID 放入同一并集；创建人不得充当负责人。
fn map_clause(scope: &PurchaseResolvedClause, actor: &str) -> PurchaseScopeClause {
    PurchaseScopeClause {
        company: scope.company,
        owner_user_id: scope.self_owned.then(|| actor.into()),
        business_org_unit_ids: scope.org_unit_ids.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn self_owned_and_org_are_kept_and_collaborative_does_not_expand() {
        let clause = map_clause(
            &PurchaseResolvedClause {
                self_owned: true,
                collaborative: true,
                org_unit_ids: vec!["org-a".into(), "org-b".into()],
                ..PurchaseResolvedClause::default()
            },
            "buyer-a",
        );
        assert_eq!(clause.owner_user_id.as_deref(), Some("buyer-a"));
        assert_eq!(
            clause.business_org_unit_ids,
            vec!["org-a".to_string(), "org-b".to_string()]
        );
    }

    #[test]
    fn attach_history_only_enriches_reads_and_updates_scope_version() {
        let mut access = PurchaseResolvedScope {
            user_id: "buyer-a".into(),
            resource: "purchase_order".into(),
            action: "list".into(),
            role_clauses: vec![],
            user_limit: None,
            policy_version: 1,
            organization_version: 1,
            scope_version: "v1".into(),
            as_of: erp_core::common::time::Instant::from_unix_secs(0),
        };
        let mut scope = PurchaseReadScope::default();
        attach_history(&mut access, &mut scope, vec!["old-order".into()]);
        assert_eq!(scope.historical_order_ids, vec!["old-order".to_string()]);
        assert!(access.scope_version.starts_with("v1:"));
        assert_ne!(access.scope_version, "v1");
    }
}
