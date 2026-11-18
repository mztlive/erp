use std::collections::BTreeMap;
use std::sync::Arc;

use mongodb::Database;

use super::facilities::{Facilities, IdentityFacilities};
use super::{AuthorizationService, rules};
use crate::entity::access_control::scope_spec::{self, ActionKind};
use crate::entity::authorization::QueryOutput;
use crate::repository::authorization::binding::{FrozenMapping, validate_rule};
use crate::{AccessFactsPort, AccessMapping, Error, Permission, Result, SharedRbacService};

/// 只绑定实现；目标能力、完整资格及数据条件均由 identity 固定。
pub struct AuthorizationBuilder {
    pub(super) facilities: Arc<dyn Facilities>,
    pub(super) facts: BTreeMap<String, Arc<dyn AccessFactsPort>>,
    pub(super) mappings: BTreeMap<Permission, Arc<dyn AccessMapping>>,
}

impl AuthorizationService {
    /// 应用组合根启动时，用同一身份库和共享 RBAC 服务创建 Builder。此步不访问数据库；绑定完整性由后续登记和 `build` 校验。
    pub fn builder(db: Database, rbac: SharedRbacService) -> AuthorizationBuilder {
        AuthorizationBuilder {
            facilities: Arc::new(IdentityFacilities { db, rbac }),
            facts: BTreeMap::new(),
            mappings: BTreeMap::new(),
        }
    }
}

impl AuthorizationBuilder {
    /// 启动时为固定资源登记唯一的对象事实端口，供单条与批量授权读取真实根对象。未知、仅功能资格或重复的资源登记均拒绝。
    pub fn facts(mut self, resource: &str, facts: Arc<dyn AccessFactsPort>) -> Result<Self> {
        if !scope_spec::actions(resource)
            .iter()
            .any(|permission| scope_spec::kind(permission).is_ok_and(|kind| kind != ActionKind::Function))
        {
            return Err(Error::ValidationError(format!("未知或无需 Facts 的资源：{resource}")));
        }
        if self.facts.insert(resource.into(), facts).is_some() {
            return Err(Error::ValidationError(format!("资源 Facts 重复绑定：{resource}")));
        }
        Ok(self)
    }

    /// 启动时为查询权限固定根模型及来源 Mapping，调用请求不能改选实现。权限不支持查询、重复登记或模型错配时拒绝。
    pub fn mapping(mut self, permission: &Permission, mapping: Arc<dyn AccessMapping>) -> Result<Self> {
        if !rules::query_supported(permission)? {
            return Err(Error::ValidationError(format!("此权限不支持 query：{permission}")));
        }
        mapping.model().validate()?;
        if mapping.model().resource != permission.resource() {
            return Err(Error::ValidationError("查询权限与根 Mapping 资源错配".into()));
        }
        if self.mappings.insert(permission.clone(), mapping).is_some() {
            return Err(Error::ValidationError(format!("查询 Mapping 重复绑定：{permission}")));
        }
        Ok(self)
    }

    /// 应用启动时核对所有固定规则的 Facts、Mapping、来源能力和输出粒度，校验通过后冻结描述并构造无请求状态的共享授权服务。缺失或错配的绑定不能降级。
    pub fn build(self) -> Result<AuthorizationService> {
        scope_spec::validate_catalog()?;
        self.ensure_complete()?;

        let mut cache = Vec::new();
        let mut mappings = BTreeMap::new();
        let mut errors = Vec::new();
        for (permission, mapping) in self.mappings {
            let mapping = FrozenMapping::freeze(mapping, &mut cache, 0)?;
            let rule = rules::binding_condition(&permission, &mapping.model().output)?;
            if let Err(error) = validate_rule(&rule, mapping.as_ref(), 0) {
                errors.push(format!("授权绑定 {permission}：{error}"));
            }
            mappings.insert(permission, mapping);
        }
        if !errors.is_empty() {
            return Err(Error::ValidationError(errors.join("\n")));
        }

        Ok(AuthorizationService {
            facilities: self.facilities,
            facts: self.facts,
            mappings,
            token: Arc::new(()),
        })
    }

    fn ensure_complete(&self) -> Result<()> {
        for permission in rules::permissions() {
            let function = scope_spec::kind(permission)? == ActionKind::Function;
            let only_task = scope_spec::kind(permission)? == ActionKind::Task
                && !rules::query_supported(permission)?
                && rules::records::condition(permission).is_none()
                && !super::approvals::supports(permission);
            if !function && !only_task && !self.facts.contains_key(permission.resource()) {
                return Err(Error::ValidationError(format!("缺少资源 Facts：{}", permission.resource())));
            }

            if rules::task_supported(permission) {
                let resource = rules::task_facts_resource(permission);
                if !self.facts.contains_key(resource) {
                    return Err(Error::ValidationError(format!("缺少任务 Facts：{resource}")));
                }
            }

            if rules::query_supported(permission)? && !self.mappings.contains_key(permission) {
                return Err(Error::ValidationError(format!("缺少查询 Mapping：{permission}")));
            }
            // 无数据规则的功能命令不需要 Mapping；精确任务的事实仍由上面的检查约束。
            if !function || rules::query_supported(permission)? {
                rules::binding_condition(permission, &QueryOutput::Object)?;
            }
        }
        Ok(())
    }
}
