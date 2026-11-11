//! 选品对象读取与写入范围；显式销售负责人和业务组织分别解释。

use std::collections::BTreeSet;
use std::sync::Arc;

use application_core::AuditActor;
use erp_identity::{
    AccessMapping, AccessSubject, AccessTarget, AuthorizationService, OrganizationDirectoryService,
    Permission, ResourceTargets, SharedRbacService,
};
use mongodb::Database;
use persistence_core::Executor;

use super::authorization;
use crate::entity::sales_selection::{SalesSelectionBooklet, SalesSelectionProposal};
use crate::error::{Error, Result};
use crate::ports::sales_selection::SelectionCustomerAccessPort;
use crate::repository::SalesSelectionExt;
use crate::repository::sales_selection::authorization::SelectionFacts;
use crate::repository::sales_selection::scope::SelectionReadScope;
use crate::service::authorization_error::identity_error;

/// 选品对象访问范围；列表、详情、候选、导出和写命令复用同一解析。
#[derive(Clone)]
pub struct SelectionAccess {
    db: Database,
    rbac: SharedRbacService,
    authorization: AuthorizationService,
    booklet: Arc<dyn AccessMapping>,
    proposal: Arc<dyn AccessMapping>,
    customer_mapping: Arc<dyn AccessMapping>,
    customers: Arc<dyn SelectionCustomerAccessPort>,
}

impl SelectionAccess {
    /// 绑定选品集合、共享身份设施与固定登记。
    /// # 参数
    /// db 为选品数据库，rbac 为共享身份服务；customer_mapping/customers 提供客户查询与对象授权。
    /// # 返回
    /// 不缓存跨请求授权的访问器。
    /// # 错误
    /// 固定登记非法时启动失败；不执行 I/O。
    pub fn new(
        db: Database,
        rbac: SharedRbacService,
        customer_mapping: Arc<dyn AccessMapping>,
        customers: Arc<dyn SelectionCustomerAccessPort>,
    ) -> Self {
        let (authorization, booklet, proposal) = authorization::service(
            db.clone(),
            rbac.clone(),
            Arc::new(SelectionFacts::new(db.clone())),
            customer_mapping.clone(),
        )
        .expect("固定选品授权目录完整");
        Self { db, rbac, authorization, booklet, proposal, customer_mapping, customers }
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
        resource: &str,
        action: &str,
        executor: &mut dyn Executor,
    ) -> Result<SelectionReadScope> {
        let mapping = match resource {
            "sales_selection_booklet" => &self.booklet,
            "sales_selection_proposal" => &self.proposal,
            _ => return Err(Error::ValidationError("非选品资源".into())),
        };
        let permission = Permission::parse(format!("{resource}:{action}"))?;
        let query = self
            .authorization
            .query(actor, &permission, AccessSubject::Direct, executor)
            .await
            .map_err(identity_error)?;
        let compiled = query.compile(mapping.as_ref(), executor).await.map_err(identity_error)?;
        self.authorization.validate(&compiled, executor).await.map_err(identity_error)?;
        Ok(SelectionReadScope::from_compiled(Arc::new(compiled), self.authorization.clone()))
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
        if action != "get" {
            self.require_object(actor, "sales_selection_booklet", "get", id, executor).await?;
        }
        self.require_object(actor, "sales_selection_booklet", action, id, executor).await?;
        let book = self
            .db
            .sales_selection_booklets()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("选品册不存在或无权查看".into()))?;
        self.customers.require_detail(actor, book.customer_id.as_ref(), executor).await?;
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
        self.require_object(actor, "sales_selection_proposal", action, id, executor).await?;
        let proposal = self
            .db
            .sales_selection_proposals()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("销售方案不存在或无权查看".into()))?;
        self.customers.require_detail(actor, proposal.customer_id.as_ref(), executor).await?;
        Ok(proposal)
    }

    /// 同一执行器内证明当前选品动作；客户资格由拥有领域独立解释。
    async fn require_object(
        &self,
        actor: &AuditActor,
        resource: &str,
        action: &str,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.authorization
            .require(
                actor,
                &Permission::parse(format!("{resource}:{action}"))?,
                AccessTarget::Records {
                    subject: AccessSubject::Direct,
                    ids: ResourceTargets::new(&[id]).map_err(identity_error)?,
                },
                executor,
            )
            .await
            .map_err(identity_error)
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
            .map_err(identity_error)
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
        self.customers.require_detail(actor, customer_id, executor).await?;
        let (authorization, _, _) = authorization::service(
            self.db.clone(),
            self.rbac.clone(),
            Arc::new(SelectionFacts::proposed(self.db.clone(), owner, org)),
            self.customer_mapping.clone(),
        )
        .map_err(identity_error)?;
        let permission = Permission::parse("sales_selection_booklet:create")?;
        authorization
            .require(
                actor,
                &permission,
                AccessTarget::Records {
                    subject: AccessSubject::Direct,
                    ids: ResourceTargets::new(&["selection_new"]).map_err(identity_error)?,
                },
                executor,
            )
            .await
            .map_err(identity_error)
    }
}
