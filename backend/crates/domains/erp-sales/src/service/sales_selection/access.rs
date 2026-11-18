//! 选品对象读取与写入范围；显式销售负责人和业务组织分别解释。

use std::collections::BTreeSet;
use std::sync::Arc;

use application_core::AuditActor;
use erp_identity::{AccessTarget, AuthorizationService, OrganizationDirectoryService};
use erp_permissions::{Permission, sales_selection as permissions};
use mongodb::Database;
use persistence_core::Executor;

use crate::entity::sales_selection::{SalesSelectionBooklet, SalesSelectionProposal};
use crate::error::{Error, Result};
use crate::repository::SalesSelectionExt;
use crate::repository::sales_selection::authorization::candidate;
use crate::repository::sales_selection::scope::SelectionReadScope;

/// 选品对象访问范围；列表、详情、候选、导出和写命令复用同一解析。
#[derive(Clone)]
pub struct SelectionAccess {
    db: Database,
    authorization: Arc<AuthorizationService>,
}

impl SelectionAccess {
    /// 绑定选品集合、共享授权服务与客户访问端口。
    /// # 参数
    /// db 为选品数据库，authorization 为已经完整绑定客户来源的共享授权服务。
    /// # 返回
    /// 不缓存跨请求授权的访问器。
    /// # 错误
    /// 不执行 I/O，不返回错误。
    pub fn new(db: Database, authorization: Arc<AuthorizationService>) -> Self {
        Self { db, authorization }
    }

    /// 取得同一进程共享授权服务，供选品用例构造使用。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回共享授权服务引用，不包含请求身份或授权结果。
    ///
    /// # 错误
    /// 不执行 I/O，不返回错误。
    pub fn authorization(&self) -> Arc<AuthorizationService> {
        self.authorization.clone()
    }

    /// 在调用方事务内证明资源动作并映射选品责任条件。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `permission` - 内置选品册或方案权限
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回本阶段唯一的选品授权凭证。
    ///
    /// # 错误
    /// 无动作权限返回 Forbidden；身份设施失败时拒绝。
    ///
    /// # 关键业务约束
    /// 责任按显式销售负责人和业务组织解释；提交人不成为负责人。
    pub async fn resolve(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        executor: &mut dyn Executor,
    ) -> Result<SelectionReadScope> {
        if !matches!(permission.resource(), "sales_selection_booklet" | "sales_selection_proposal") {
            return Err(Error::ValidationError("不是选品查询资源".into()));
        }
        let compiled = self.authorization.query(actor, permission, executor).await?;
        Ok(SelectionReadScope::from_compiled(Arc::new(compiled), self.authorization.clone()))
    }

    /// 在调用方事务内重验册对象资格。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `permission` - 内置权限
    /// * `id` - 选品册主键
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 对象在范围内时返回选品册。
    ///
    /// # 错误
    /// 非快照执行器返回参数错误；不可见对象返回 NotFound，不泄露存在性。
    ///
    /// # 关键业务约束
    /// 写命令必须在原领域事务内调用；列表授权不替代对象重验。
    pub async fn require_booklet(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<SalesSelectionBooklet> {
        ensure_snapshot(executor)?;
        self.require_object(actor, "sales_selection_booklet", permission, id, executor).await?;
        let book = self
            .db
            .sales_selection_booklets()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("选品册不存在或无权查看".into()))?;
        Ok(book)
    }

    /// 在调用方事务内重验方案对象资格。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `permission` - 内置权限
    /// * `id` - 方案主键
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 对象在范围内时返回方案。
    ///
    /// # 错误
    /// 非快照执行器返回参数错误；不可见对象返回 NotFound。
    ///
    /// # 关键业务约束
    /// 方案沿所属册责任关联解释，不得以客户提交人兜底。
    pub async fn require_proposal(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<SalesSelectionProposal> {
        ensure_snapshot(executor)?;
        self.require_object(actor, "sales_selection_proposal", permission, id, executor).await?;
        let proposal = self
            .db
            .sales_selection_proposals()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("销售方案不存在或无权查看".into()))?;
        Ok(proposal)
    }

    /// 同一执行器内证明当前选品动作及读取前提；客户详情保留独立拒绝合同。
    async fn require_object(
        &self,
        actor: &AuditActor,
        resource: &str,
        permission: &Permission,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        ensure_resource(permission, resource)?;
        self.authorization
            .require(actor, permission, AccessTarget::record(id)?, executor)
            .await
            .map_err(Error::from)
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
    /// 未知组织或身份设施失败时拒绝。
    pub async fn expand_org_units(
        &self,
        org_unit_ids: &[String],
        include_descendants: bool,
        executor: &mut dyn Executor,
    ) -> Result<BTreeSet<String>> {
        OrganizationDirectoryService::new(self.db.clone())
            .expand_org_units(org_unit_ids, include_descendants, executor)
            .await
            .map_err(Error::from)
    }

    /// 检查拟创建选品册的责任资格。
    /// # 参数
    /// actor 为操作人，customer_id 为实际客户，owner/org 为拟写入责任，executor 为原事务。
    /// # 返回
    /// 统一授权通过时成功。
    /// # 错误
    /// 角色、范围或事实缺失时拒绝。
    pub async fn require_new(
        &self,
        actor: &AuditActor,
        customer_id: &str,
        owner: &str,
        org: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let candidate = candidate(customer_id, owner, org)?;
        self.authorization
            .require(actor, &permissions::BOOKLET_CREATE, AccessTarget::Candidate(candidate), executor)
            .await
            .map_err(Error::from)
    }
}

/// 完整对象授权后的业务读取必须保留同一快照，不能复用非事务对象检查。
fn ensure_snapshot(executor: &mut dyn Executor) -> Result<()> {
    if !executor.boundary().is_some_and(|boundary| boundary.is_snapshot()) {
        return Err(Error::ValidationError("选品对象授权与读取必须使用同一快照事务".into()));
    }
    Ok(())
}

/// 权限值不能切换拥有领域已经确定的册或方案对象语义。
fn ensure_resource(permission: &Permission, resource: &str) -> Result<()> {
    if permission.resource() != resource {
        return Err(Error::ValidationError("选品权限与目标资源不符".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use erp_identity::testing::unavailable_authorization;
    use mongodb::Client;
    use persistence_core::ReadExecutor;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;

    #[test]
    fn booklet_and_proposal_access_cannot_swap_resource_permissions() {
        assert!(ensure_resource(&permissions::BOOKLET_PREPARE, "sales_selection_booklet").is_ok());
        assert!(ensure_resource(&permissions::PROPOSAL_DETAIL, "sales_selection_proposal").is_ok());
        assert!(matches!(
            ensure_resource(&permissions::PROPOSAL_DETAIL, "sales_selection_booklet"),
            Err(Error::ValidationError(_))
        ));
        assert!(matches!(
            ensure_resource(&permissions::BOOKLET_LIST, "sales_selection_proposal"),
            Err(Error::ValidationError(_))
        ));
    }

    #[tokio::test]
    async fn complete_selection_reads_require_snapshot_and_current_qualification() {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1").await.unwrap().database("selection_reads");
        let access = SelectionAccess::new(db, unavailable_authorization());
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);

        let error = access
            .require_booklet(&actor, &permissions::BOOKLET_COPY_LINK, "book", &mut ReadExecutor::default())
            .await
            .unwrap_err();
        assert!(matches!(error, Error::ValidationError(message)
            if message == "选品对象授权与读取必须使用同一快照事务"));
        let error = access
            .require_proposal(&actor, &permissions::PROPOSAL_DETAIL, "proposal", &mut ReadExecutor::default())
            .await
            .unwrap_err();
        assert!(matches!(error, Error::ValidationError(message)
            if message == "选品对象授权与读取必须使用同一快照事务"));

        let error = access
            .require_booklet(
                &actor,
                &permissions::BOOKLET_COPY_LINK,
                "book",
                &mut SnapshotExecutor::default(),
            )
            .await
            .unwrap_err();
        assert!(matches!(error, Error::Internal(message) if message == "测试授权设施不可用"));
    }
}
