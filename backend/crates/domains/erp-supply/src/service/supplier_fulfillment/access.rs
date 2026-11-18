//! 供应商履约订单对象读取与写入范围；跟进人和业务组织分别解释。

use std::sync::Arc;

use application_core::AuditActor;
use erp_identity::entity::authorization::{FactValue, ObjectFacts};
use erp_identity::{AccessTarget, AuthorizationService, OrganizationDirectoryService};
use erp_permissions::{Permission, supplier_fulfillment};
use mongodb::Database;
use persistence_core::Executor;

use crate::authorization::{Authorization, CompiledScope};
use crate::entity::supplier_fulfillment::SupplierFulfillmentOrder;
use crate::error::{Error, Result};
use crate::repository::SupplierFulfillmentExt;

/// 履约订单对象访问范围；列表、详情、候选、导出和写命令复用同一解析。
#[derive(Clone)]
pub struct FulfillmentOrderAccess {
    db: Database,
    authorization: Option<Authorization>,
}

impl FulfillmentOrderAccess {
    /// 绑定履约订单集合与共享身份设施。
    /// 返回无授权缓存的访问服务，构造不执行 I/O。
    pub fn new(db: Database, service: Arc<AuthorizationService>) -> Self {
        let authorization =
            Authorization::new(service, "supplier_fulfillment_order").expect("固定资源授权目录有效");
        Self { db, authorization: Some(authorization) }
    }

    /// 创建尚未接线的访问器，授权调用保持失败关闭。
    /// 未绑定身份设施的访问器。
    /// 构造不执行 I/O。
    pub fn unwired(db: Database) -> Self {
        Self { db, authorization: None }
    }

    // 提供已注入的身份设施，禁止默认扩大范围。
    fn authorization(&self) -> Result<&Authorization> {
        self.authorization.as_ref().ok_or_else(|| Error::Internal("供应链授权未接线".into()))
    }

    /// 在调用方事务内证明资源动作并映射跟进责任条件。
    /// 返回本次查询的单份授权凭证。
    /// 无动作权限返回 Forbidden；未装配时拒绝。
    /// `permission`：内置权限。
    pub async fn resolve(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        executor: &mut dyn Executor,
    ) -> Result<CompiledScope> {
        Ok(self.authorization()?.resolve(actor, permission, executor).await?)
    }

    /// 在调用方事务内重验订单对象资格。
    /// 对象在范围内时返回订单。
    /// 不可见对象返回 NotFound，不泄露存在性。
    /// `permission`：内置权限。
    pub async fn require_order(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<SupplierFulfillmentOrder> {
        self.authorization()?.require(actor, permission, id, executor).await?;
        self.db
            .supplier_fulfillment_orders()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("对象不存在或无权访问".into()))
    }

    // 在原事务内同时检查原履约单与服务端完成交接后的归属。
    // 原记录和拟保存的跟进人、业务组织均通过当前固定政策时成功。
    // 功能权限、对象或目标归属拒绝，事实缺失及执行器不是快照事务时返回错误。
    // actor 为操作人，order 为已应用领域交接规则的候选实体，executor 为原写事务。
    pub(crate) async fn require_owner_change(
        &self,
        actor: &AuditActor,
        order: &SupplierFulfillmentOrder,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        Ok(self
            .authorization()?
            .service
            .require(
                actor,
                &supplier_fulfillment::ORDER_HANDOVER,
                AccessTarget::OwnershipChange {
                    id: order.base.id.clone(),
                    candidate: ObjectFacts {
                        resource: "supplier_fulfillment_order".into(),
                        id: order.base.id.clone(),
                        owner_id: FactValue::Known(Some(order.follow_up_user_id.clone())),
                        org_unit_id: FactValue::Known(Some(order.business_org_unit_id.clone())),
                        ..Default::default()
                    },
                },
                executor,
            )
            .await?)
    }

    /// 展开请求组织及其可选下级，与授权用同一执行器。
    /// 返回展开后的组织身份集合。
    /// 授权设施 展开失败时返回对应错误。
    pub async fn expand_org_units(
        &self,
        org_unit_ids: &[String],
        include_descendants: bool,
        executor: &mut dyn Executor,
    ) -> Result<std::collections::BTreeSet<String>> {
        Ok(OrganizationDirectoryService::new(self.db.clone())
            .expand_org_units(org_unit_ids, include_descendants, executor)
            .await?)
    }

    /// 检查共享资料当前动作的功能资格；对象操作另行检查完整目标。
    /// 当前有效账号具备动作资格时成功。
    /// 未接线、未登记动作或账号无操作资格时拒绝。
    /// `permission`：内置权限。
    pub async fn ensure_writable(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.authorization()?.ensure_permission(permission)?;
        if !self.authorization()?.service.has_permission(actor, permission, executor).await? {
            return Err(Error::Forbidden("没有该操作权限".into()));
        }
        Ok(())
    }
}
