//! 选品对象读取与写入范围；显式销售负责人和业务组织分别解释。

use std::collections::BTreeSet;
use std::sync::Arc;

use application_core::AuditActor;
use mongodb::Database;
use persistence_core::Executor;

use crate::entity::sales_selection::{SalesSelectionBooklet, SalesSelectionProposal};
use crate::error::{Error, Result};
use crate::ports::{SelectionDataScopePort, SelectionResolvedScope};
use crate::repository::SalesSelectionExt;
use crate::repository::sales_selection::scope::SelectionReadScope;

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
        let scope = SelectionReadScope::from_compiled(access.access.clone(), access.authorization.clone());
        Ok((access, scope))
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
        self.scope.require(actor, "sales_selection_booklet", action, id, executor).await?;
        self.db
            .sales_selection_booklets()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("选品册不存在或无权查看".into()))
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
        self.scope.require(actor, "sales_selection_proposal", action, id, executor).await?;
        self.db
            .sales_selection_proposals()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("销售方案不存在或无权查看".into()))
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

    /// 检查拟创建选品册的责任资格。
    /// # 参数
    /// actor 为操作人，owner/org 为拟写入责任，executor 为原事务。
    /// # 返回
    /// 统一授权通过时成功。
    /// # 错误
    /// 角色、范围或事实缺失时拒绝。
    pub async fn require_new(
        &self,
        actor: &AuditActor,
        owner: &str,
        org: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.scope.require_new(actor, owner, org, executor).await
    }
}
