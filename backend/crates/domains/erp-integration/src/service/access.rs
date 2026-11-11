//! 集成授权、查询编译与业务组织事实入口。
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_identity::{
    AccessMapping, AccessSubject, AccessTarget, AuthorizationService, CompiledAccess,
    OrganizationDirectoryService, Permission, ResourceTargets, SharedRbacService,
};
use mongodb::Database;
use persistence_core::Executor;

use crate::repository::authorization::registration;
use crate::{Error, Result};

/// 固定登记的集成授权设施；查询凭证绑定本实例。
#[derive(Clone)]
pub struct IntegrationAccess {
    db: Database,
    service: AuthorizationService,
    mappings: BTreeMap<String, Arc<dyn AccessMapping>>,
}
impl IntegrationAccess {
    /// 绑定共享身份设施和本域固定目录。
    /// # 参数
    /// db、rbac 为组合根共享身份设施。
    /// # 返回
    /// 不缓存授权结果的访问器。
    /// # 错误
    /// 固定目录非法时启动失败；构造不执行 I/O。
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        let (registry, mappings) = registration(&db);
        Self { service: AuthorizationService::new(db.clone(), rbac, registry), db, mappings }
    }

    /// 查询并编译固定资源的完整授权条件。
    /// # 参数
    /// actor、resource、action 指定服务端入口，executor 为原执行器。
    /// # 返回
    /// identity 生成的不可伪造查询凭证。
    /// # 错误
    /// 未登记入口、无权、映射或事实失败时拒绝。
    pub async fn resolve(
        &self,
        actor: &AuditActor,
        resource: &str,
        action: &str,
        executor: &mut dyn Executor,
    ) -> Result<CompiledAccess> {
        let mapping =
            self.mappings.get(resource).ok_or_else(|| Error::ValidationError("集成资源未登记".into()))?;
        Ok(self
            .service
            .query(
                actor,
                &Permission::parse(format!("{resource}:{action}"))?,
                AccessSubject::Direct,
                executor,
            )
            .await?
            .compile(mapping.as_ref(), executor)
            .await?)
    }

    /// 复验同一服务生成的读取凭证。
    /// # 参数
    /// access、executor 为本次凭证及原执行器。
    /// # 返回
    /// 仍有效时成功。
    /// # 错误
    /// 身份、权限、映射或版本变化时拒绝。
    pub async fn validate(&self, access: &CompiledAccess, executor: &mut dyn Executor) -> Result<()> {
        Ok(self.service.validate(access, executor).await?)
    }

    /// 按真实记录身份执行完整对象授权。
    /// # 参数
    /// actor、resource、action 为固定入口，id 为实际根记录，executor 为原执行器。
    /// # 返回
    /// 全部对象事实及身份检查通过时成功。
    /// # 错误
    /// 对象缺失、无权或版本变化拒绝。
    pub async fn require_handler(
        &self,
        actor: &AuditActor,
        resource: &str,
        action: &str,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let target = if action == "create" {
            AccessTarget::Operation
        } else {
            AccessTarget::Records { subject: AccessSubject::Direct, ids: ResourceTargets::new(&[id])? }
        };
        Ok(self
            .service
            .require(actor, &Permission::parse(format!("{resource}:{action}"))?, target, executor)
            .await?)
    }
    /// 查询处理人在解析时点的唯一主属组织。
    ///
    /// # 参数
    /// * `user_id` - 拟写入的处理人
    /// * `at` - 与授权相同的解析时点
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 存在唯一主属组织时返回其 ID。
    ///
    /// # 错误
    /// 处理人 ID 为 `me` 时返回 `ValidationError`；组织关系非法或缺少有效内部组织时拒绝。
    pub async fn require_handler_org(
        &self,
        user_id: &str,
        at: Instant,
        executor: &mut dyn Executor,
    ) -> Result<String> {
        if user_id.trim().eq_ignore_ascii_case("me") {
            return Err(Error::ValidationError("处理人不得使用 me 作为人员 ID".into()));
        }
        OrganizationDirectoryService::new(self.db.clone())
            .own_org(user_id, at, executor)
            .await?
            .filter(|org| !org.eq_ignore_ascii_case("company"))
            .ok_or_else(|| Error::ValidationError("处理人缺少有效内部组织".into()))
    }

    /// 展开请求组织及其可选下级。
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
    /// 未知组织时拒绝。
    pub async fn expand_org_units(
        &self,
        org_unit_ids: &[String],
        include_descendants: bool,
        executor: &mut dyn Executor,
    ) -> Result<BTreeSet<String>> {
        Ok(OrganizationDirectoryService::new(self.db.clone())
            .expand_org_units(org_unit_ids, include_descendants, executor)
            .await?)
    }
}
