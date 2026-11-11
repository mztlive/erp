//! 供应商履约订单对象读取与写入范围；跟进人和业务组织分别解释。

use application_core::AuditActor;
use erp_identity::{OrganizationDirectoryService, SharedRbacService};
use mongodb::Database;
use persistence_core::Executor;

use crate::authorization::{self, Authorization, FulfillmentOrderResolvedScope};
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
    ///
    /// # 参数
    /// * `db` - 订单集合所在数据库
    /// * `rbac` - 共享身份设施
    ///
    /// # 返回
    /// 返回无授权缓存的访问服务，构造不执行 I/O。
    ///
    /// # 错误
    /// 无。
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        let authorization =
            Authorization::new(&db, rbac, "supplier_fulfillment_order").expect("固定资源授权目录有效");
        Self { db, authorization: Some(authorization) }
    }

    /// 创建尚未接线的访问器，授权调用保持失败关闭。
    /// # 参数
    /// db 为业务集合所在数据库。
    /// # 返回
    /// 未绑定身份设施的访问器。
    /// # 错误
    /// 构造不执行 I/O。
    pub fn unwired(db: Database) -> Self {
        Self { db, authorization: None }
    }

    /// 取得已注入的身份设施，禁止默认扩大范围。
    fn authorization(&self) -> Result<&Authorization> {
        self.authorization.as_ref().ok_or_else(|| Error::Internal("供应链授权未接线".into()))
    }

    /// 在调用方事务内证明资源动作并映射跟进责任条件。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `action` - 已注册动作
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回本次查询的单份授权凭证。
    ///
    /// # 错误
    /// 无动作权限返回 Forbidden；未装配时拒绝。
    pub async fn resolve(
        &self,
        actor: &AuditActor,
        action: &str,
        executor: &mut dyn Executor,
    ) -> Result<FulfillmentOrderResolvedScope> {
        Ok(self.authorization()?.resolve(actor, action, executor).await?)
    }

    /// 在调用方事务内重验订单对象资格。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `action` - 已注册动作
    /// * `id` - 订单主键
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 对象在范围内时返回订单。
    ///
    /// # 错误
    /// 不可见对象返回 NotFound，不泄露存在性。
    pub async fn require_order(
        &self,
        actor: &AuditActor,
        action: &str,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<SupplierFulfillmentOrder> {
        self.authorization()?.require(actor, action, id, executor).await?;
        self.db
            .supplier_fulfillment_orders()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("对象不存在或无权访问".into()))
    }

    /// 展开请求组织及其可选下级，与授权用同一执行器。
    ///
    /// # 参数
    /// * `org_unit_ids` - 请求中的组织身份。
    /// * `include_descendants` - 为真时包含下级组织。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 返回展开后的组织身份集合。
    ///
    /// # 错误
    /// 授权设施 展开失败时返回对应错误。
    pub async fn expand_org_units(
        &self,
        org_unit_ids: &[String],
        include_descendants: bool,
        executor: &mut dyn Executor,
    ) -> Result<std::collections::BTreeSet<String>> {
        self.authorization()?;
        Ok(OrganizationDirectoryService::new(self.db.clone())
            .expand_org_units(org_unit_ids, include_descendants, executor)
            .await?)
    }

    /// 查询账号在解析时点的唯一主属组织。
    ///
    /// # 参数
    /// * `user_id` - 账号身份。
    /// * `at` - 解析时点。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 返回授权设施 给出的主属组织。`None` 表示该时点没有返回主属组织。
    ///
    /// # 错误
    /// 授权设施 查询失败时返回对应错误。
    pub async fn own_org(
        &self,
        user_id: &str,
        at: erp_core::common::time::Instant,
        executor: &mut dyn Executor,
    ) -> Result<Option<String>> {
        self.authorization()?;
        Ok(OrganizationDirectoryService::new(self.db.clone()).own_org(user_id, at, executor).await?)
    }

    /// 检查共享资料当前动作的功能资格；对象操作另行检查完整目标。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人。
    /// * `action` - 已注册动作。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 当前有效账号具备动作资格时成功。
    ///
    /// # 错误
    /// 未接线、未登记动作或账号无操作资格时拒绝。
    pub async fn ensure_writable(
        &self,
        actor: &AuditActor,
        action: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let permission = authorization::action_permission("supplier_fulfillment_order", action)?;
        if !self.authorization()?.has_permission(actor, &permission, executor).await? {
            return Err(Error::Forbidden("没有该操作权限".into()));
        }
        Ok(())
    }
}
