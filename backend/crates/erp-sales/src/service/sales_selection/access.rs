//! 选品对象读取与写入范围；显式销售负责人和业务组织分别解释。

use std::collections::BTreeSet;
use std::sync::Arc;

use application_core::AuditActor;
use mongodb::Database;
use persistence_core::Executor;

use crate::entity::sales_selection::{SalesSelectionBooklet, SalesSelectionProposal};
use crate::error::{Error, Result};
use crate::ports::{
    SelectionDataScopePort, SelectionResolvedClause, SelectionResolvedScope, SelectionScopeObject,
};
use crate::repository::SalesSelectionExt;
use crate::repository::sales_selection::scope::{SelectionReadScope, SelectionScopeClause};

/// 选品对象访问范围；列表、详情、候选、导出和写命令复用同一解析。
#[derive(Clone)]
pub struct SelectionAccess {
    db: Database,
    scope: Arc<dyn SelectionDataScopePort>,
}

impl SelectionAccess {
    /// 绑定选品集合与范围授权 Port。
    ///
    /// # 参数
    /// * `db` - 选品集合所在数据库
    /// * `scope` - 组合层注入的选品范围 Port
    ///
    /// # 返回
    /// 返回无授权缓存的访问服务，构造不执行 I/O。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 不得在构造时补公司范围；不得直接构造身份域 Service。
    pub fn new(db: Database, scope: Arc<dyn SelectionDataScopePort>) -> Self {
        Self { db, scope }
    }

    /// 在调用方事务内证明资源动作并映射选品责任条件。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `resource` - 选品册或方案资源
    /// * `action` - 已注册动作
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回已解析范围事实和选品授权条件。
    ///
    /// # 错误
    /// 无动作权限返回 Forbidden；未装配时拒绝。
    ///
    /// # 关键业务约束
    /// 责任按显式销售负责人和业务组织解释；提交人不成为负责人。
    pub async fn resolve(
        &self,
        actor: &AuditActor,
        resource: &str,
        action: &str,
        executor: &mut dyn Executor,
    ) -> Result<(SelectionResolvedScope, SelectionReadScope)> {
        let access = self.scope.resolve(actor, resource, action, executor).await?;
        Ok((access.clone(), selection_scope(&access, actor.id())))
    }

    /// 在调用方事务内重验册对象资格。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `action` - 已注册动作
    /// * `id` - 选品册主键
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 对象在范围内时返回选品册。
    ///
    /// # 错误
    /// 不可见对象返回 NotFound，不泄露存在性。
    ///
    /// # 关键业务约束
    /// 写命令必须在原领域事务内调用；列表授权不替代对象重验。
    pub async fn require_booklet(
        &self,
        actor: &AuditActor,
        action: &str,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<SalesSelectionBooklet> {
        let (access, scope) = self.resolve(actor, "sales_selection_booklet", action, executor).await?;
        let found = self
            .db
            .sales_selection_booklets()
            .find_by_id(id, executor)
            .await?
            .filter(|item| in_scope(&scope, &item.sales_owner_user_id, &item.business_org_unit_id));
        let Some(book) = found else {
            return Err(Error::NotFound("选品册不存在或无权查看".into()));
        };
        let object = SelectionScopeObject {
            owned: book.sales_owner_user_id == access.user_id,
            historical_read_participant: false,
            org_unit_id: Some(book.business_org_unit_id.clone()),
        };
        if !self.scope.allows(&access, &object)? {
            return Err(Error::NotFound("选品册不存在或无权查看".into()));
        }
        Ok(book)
    }

    /// 在调用方事务内重验方案对象资格。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `action` - 已注册动作
    /// * `id` - 方案主键
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 对象在范围内时返回方案。
    ///
    /// # 错误
    /// 不可见对象返回 NotFound。
    ///
    /// # 关键业务约束
    /// 方案沿所属册责任关联解释，不得以客户提交人兜底。
    pub async fn require_proposal(
        &self,
        actor: &AuditActor,
        action: &str,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<SalesSelectionProposal> {
        let (access, scope) = self.resolve(actor, "sales_selection_proposal", action, executor).await?;
        let found = self
            .db
            .sales_selection_proposals()
            .find_by_id(id, executor)
            .await?
            .filter(|item| in_scope(&scope, &item.sales_owner_user_id, &item.business_org_unit_id));
        let Some(proposal) = found else {
            return Err(Error::NotFound("销售方案不存在或无权查看".into()));
        };
        let object = SelectionScopeObject {
            owned: proposal.sales_owner_user_id == access.user_id,
            historical_read_participant: false,
            org_unit_id: Some(proposal.business_org_unit_id.clone()),
        };
        if !self.scope.allows(&access, &object)? {
            return Err(Error::NotFound("销售方案不存在或无权查看".into()));
        }
        Ok(proposal)
    }

    /// 复用公共单对象判定检查已加载的册，供批量授权使用。
    ///
    /// # 参数
    /// * `access` - 当前动作已解析事实
    /// * `scope` - 同次解析的责任条件
    /// * `book` - 已加载选品册
    ///
    /// # 返回
    /// 范围允许时为 true。
    ///
    /// # 错误
    /// 资源动作不符或未装配时拒绝。
    pub fn allows_booklet(
        &self,
        access: &SelectionResolvedScope,
        scope: &SelectionReadScope,
        book: &SalesSelectionBooklet,
    ) -> Result<bool> {
        if !in_scope(scope, &book.sales_owner_user_id, &book.business_org_unit_id) {
            return Ok(false);
        }
        self.scope.allows(
            access,
            &SelectionScopeObject {
                owned: book.sales_owner_user_id == access.user_id,
                historical_read_participant: false,
                org_unit_id: Some(book.business_org_unit_id.clone()),
            },
        )
    }

    /// 展开请求组织及其可选下级，与授权用同一执行器。
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
    /// 未知组织或未装配时拒绝。
    pub async fn expand_org_units(
        &self,
        org_unit_ids: &[String],
        include_descendants: bool,
        executor: &mut dyn Executor,
    ) -> Result<BTreeSet<String>> {
        self.scope.expand_org_units(org_unit_ids, include_descendants, executor).await
    }

    /// 检查拟创建责任是否在创建动作范围内。
    ///
    /// # 参数
    /// * `access` - 创建动作已解析事实
    /// * `scope` - 同次解析的责任条件
    /// * `owner` - 拟写入的显式销售负责人
    /// * `org` - 拟写入的业务组织
    ///
    /// # 返回
    /// 范围允许时为 true。
    ///
    /// # 错误
    /// 资源动作不符或未装配时拒绝。
    pub fn allows_new(
        &self,
        access: &SelectionResolvedScope,
        scope: &SelectionReadScope,
        owner: &str,
        org: &str,
    ) -> Result<bool> {
        if !in_scope(scope, owner, org) {
            return Ok(false);
        }
        self.scope.allows(
            access,
            &SelectionScopeObject {
                owned: owner == access.user_id,
                historical_read_participant: false,
                org_unit_id: Some(org.to_string()),
            },
        )
    }
}

/// 映射同角色正向范围和独立个人上限，保持交集关系。
///
/// # 参数
/// * `access` - Port 返回的已解析授权
/// * `user` - 当前账号
///
/// # 返回
/// 返回选品仓储条件。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 本人负责解释显式销售负责人；组织解释业务组织。
pub fn selection_scope(access: &SelectionResolvedScope, user: &str) -> SelectionReadScope {
    SelectionReadScope {
        roles: access.role_clauses.iter().map(|clause| map_clause(clause, user)).collect(),
        user_limit: access.user_limit.as_ref().map(|clause| map_clause(clause, user)),
        historical_ids: Vec::new(),
    }
}

/// 仅接受已解析的内部组织范围，不读取创建人或提交人作为授权。
///
/// # 参数
/// * `clause` - Port 正向范围
/// * `actor` - 当前账号
///
/// # 返回
/// 返回选品责任条款。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 不得把创建人、提交人当作负责人。
fn map_clause(clause: &SelectionResolvedClause, actor: &str) -> SelectionScopeClause {
    SelectionScopeClause {
        company: clause.company,
        owner_user_id: clause.self_owned.then(|| actor.into()),
        business_org_unit_ids: clause.org_unit_ids.clone(),
    }
}

/// 按仓储条件精确预筛，公共判定仍由 Port 最终确认。
///
/// # 参数
/// * `scope` - 已映射的责任条件
/// * `owner` - 对象销售负责人
/// * `org` - 对象业务组织
///
/// # 返回
/// 预筛命中时为 true。
///
/// # 错误
/// 无。
fn in_scope(scope: &SelectionReadScope, owner: &str, org: &str) -> bool {
    scope.allows_object(owner, org)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn self_owned_and_org_are_kept() {
        let clause = map_clause(
            &SelectionResolvedClause {
                self_owned: true,
                org_unit_ids: vec!["org-a".into()],
                ..SelectionResolvedClause::default()
            },
            "sales-a",
        );
        assert_eq!(clause.owner_user_id.as_deref(), Some("sales-a"));
        assert_eq!(clause.business_org_unit_ids, vec!["org-a".to_string()]);
    }
}
