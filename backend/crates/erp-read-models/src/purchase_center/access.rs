//! 采购单按当前采购负责人与单据业务组织授权，历史快照不提供访问资格。

use application_core::AuditActor;
use erp_identity::access_control::ScopeClause;
use erp_identity::service::access_control::resolve::{AuthorizedDataScope, DataScopeService};
use erp_identity::Permission;
use erp_identity::SharedRbacService;
use erp_procurement::entity::purchase_order::PurchaseOrder;
use erp_procurement::repository::purchase_order::scope::{PurchaseReadScope, PurchaseScopeClause};
use erp_procurement::repository::PurchaseOrderExt;
use erp_workflow::DocumentRegistryExt;
use mongodb::Database;
use persistence_core::Executor;
use persistence_core::Transactional;
use std::hash::{Hash, Hasher};

use crate::{Error, Result};

/// 采购对象读取范围；供列表、详情、候选、导出和写命令复用。
#[derive(Clone)]
pub struct PurchaseAccess {
    db: Database,
    rbac: SharedRbacService,
}

impl PurchaseAccess {
    /// 绑定应用的身份与采购事实来源。
    ///
    /// # 参数
    /// * `db` - 采购与身份集合所在数据库
    /// * `rbac` - 现有 RBAC 快照服务
    ///
    /// # 返回
    /// 返回无授权缓存的读取服务，构造不执行 I/O。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 不得在构造时补公司范围或读取登录人默认组织。
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self { db, rbac }
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
                    let (context, scope) = this.resolve(&actor, "detail", &[], executor).await?;
                    let order = this
                        .db
                        .purchase_orders()
                        .find_authorized(&id, &scope, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("采购单不存在或无权查看".into()))?;
                    let version = format!(
                        "{}:{}:{}",
                        context.scope_version, order.base.id, order.base.version
                    );
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
    /// * `permissions` - 同角色必须同时持有的额外权限
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
        permissions: &[Permission],
        executor: &mut dyn Executor,
    ) -> Result<PurchaseOrder> {
        let (_, scope) = self.resolve(actor, action, permissions, executor).await?;
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
        let submit = Permission::parse("purchase_order:submit")?;
        let create = Permission::parse("purchase_order:create")?;
        let (_, mut scope) = self.resolve(actor, "create", &[submit], executor).await?;
        let (_, submit_scope) = self.resolve(actor, "submit", &[create], executor).await?;
        scope.required_scopes.push(submit_scope);
        let owner = order.current_owner_user_id()?;
        if !scope.allows_creation(owner, &order.business_org_unit_id) {
            return Err(Error::Forbidden("没有该业务责任范围的采购建单权限".into()));
        }
        Ok(())
    }

    /// 按资源动作证明范围，并使用当前采购责任解释内部组织及个人范围。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `action` - 已注册的采购动作
    /// * `permissions` - 同角色必须同时持有的额外权限
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
        permissions: &[Permission],
        executor: &mut dyn Executor,
    ) -> Result<(AuthorizedDataScope, PurchaseReadScope)> {
        let mut access = DataScopeService::new(self.db.clone(), self.rbac.clone())
            .resolve_permissions(actor, "purchase_order", action, permissions, executor)
            .await?;
        let history = if allows_history(action) {
            self.participant_orders(actor.id(), executor).await?
        } else {
            Vec::new()
        };
        let mut fingerprint = std::collections::hash_map::DefaultHasher::new();
        history.hash(&mut fingerprint);
        access.scope_version = format!("{}:{:x}", access.scope_version, fingerprint.finish());
        let scope = purchase_scope(&access, actor.id(), history)?;
        Ok((access, scope))
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
        let ids = self
            .db
            .document_participants()
            .document_ids_by_user(user, executor)
            .await?;
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

/// 映射同角色正向范围和独立个人上限，保持两者的交集关系。
///
/// # 参数
/// * `access` - 身份域已解析授权
/// * `user` - 当前账号
/// * `history` - 合法历史参与单据
///
/// # 返回
/// 返回采购仓储条件。
///
/// # 错误
/// 出现结算主体或仓库维度时拒绝。
///
/// # 关键业务约束
/// 不支持的维度必须拒绝，不得静默丢弃或与部门 ID 求并。
fn purchase_scope(
    access: &AuthorizedDataScope,
    user: &str,
    history: Vec<String>,
) -> Result<PurchaseReadScope> {
    Ok(PurchaseReadScope {
        required_scopes: vec![],
        historical_order_ids: history,
        roles: access
            .scope
            .role_clauses
            .iter()
            .map(|clause| map_clause(clause, user))
            .collect::<Result<Vec<_>>>()?,
        user_limit: access
            .scope
            .user_limit
            .as_ref()
            .map(|clause| map_clause(clause, user))
            .transpose()?,
    })
}

/// 仅接受身份域已解析的内部组织范围，不读取历史业绩或仓库作为授权。
///
/// # 参数
/// * `scope` - 身份域正向范围
/// * `actor` - 当前账号
///
/// # 返回
/// 返回采购责任条款。
///
/// # 错误
/// 结算主体或仓库目标非空时拒绝。
///
/// # 关键业务约束
/// 协作范围不映射为采购对象；本人负责解释当前采购负责人。
fn map_clause(scope: &ScopeClause, actor: &str) -> Result<PurchaseScopeClause> {
    if !scope.settlement_party_ids.is_empty() || !scope.warehouse_ids.is_empty() {
        return Err(Error::ValidationError("采购范围不支持结算主体或仓库维度".into()));
    }
    Ok(PurchaseScopeClause {
        company: scope.company,
        owner_user_id: scope.self_owned.then(|| actor.into()),
        business_org_unit_ids: scope.org_unit_ids.iter().cloned().collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn historical_participation_never_grants_commands() {
        for action in ["list", "detail"] {
            assert!(allows_history(action));
        }
        for action in [
            "create",
            "update",
            "submit",
            "cancel_approval",
            "delete",
            "transfer",
            "*",
        ] {
            assert!(!allows_history(action));
        }
    }

    #[test]
    fn purchase_adapter_rejects_warehouse_and_settlement_dimensions() {
        let warehouse = ScopeClause {
            warehouse_ids: BTreeSet::from(["wh-1".into()]),
            ..ScopeClause::default()
        };
        match map_clause(&warehouse, "buyer-a") {
            Err(Error::ValidationError(message)) => assert!(message.contains("仓库")),
            other => panic!("expected validation error, got {other:?}"),
        }
        let settlement = ScopeClause {
            settlement_party_ids: BTreeSet::from(["party-1".into()]),
            ..ScopeClause::default()
        };
        assert!(matches!(
            map_clause(&settlement, "buyer-a"),
            Err(Error::ValidationError(_))
        ));
    }

    #[test]
    fn self_owned_and_org_are_kept_and_collaborative_does_not_expand() {
        let clause = map_clause(
            &ScopeClause {
                self_owned: true,
                collaborative: true,
                org_unit_ids: BTreeSet::from(["org-b".into(), "org-a".into()]),
                ..ScopeClause::default()
            },
            "buyer-a",
        )
        .unwrap();
        assert_eq!(clause.owner_user_id.as_deref(), Some("buyer-a"));
        assert_eq!(
            clause.business_org_unit_ids,
            vec!["org-a".to_string(), "org-b".to_string()]
        );
    }
}
