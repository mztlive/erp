use std::collections::BTreeMap;
use std::sync::Arc;

use mongodb::Database;

use super::facilities::{Facilities, IdentityFacilities};
use super::{AuthorizationService, rules};
use crate::entity::access_control::scope_spec::{self, ActionKind};
use crate::entity::authorization::QueryOutput;
use crate::repository::authorization::binding::{FrozenMapping, validate_rule};
use crate::{AccessFactsPort, AccessMapping, Error, Permission, Result, SharedRbacService};

/// 应用启动时装配授权实现；目标能力、完整资格和数据规则均由 identity 固定。
pub struct AuthorizationBuilder {
    /// 构建后供共享服务使用的身份取证设施。
    pub(super) facilities: Arc<dyn Facilities>,
    /// 构建期间按资源登记的对象事实端口。
    pub(super) facts: BTreeMap<String, Arc<dyn AccessFactsPort>>,
    /// 构建期间按权限登记、尚待冻结校验的查询映射。
    pub(super) mappings: BTreeMap<Permission, Arc<dyn AccessMapping>>,
}

impl AuthorizationService {
    /// 应用启动时用身份库和共享 RBAC 创建构建器；此步不读库，绑定在登记及 `build` 时校验。
    pub fn builder(db: Database, rbac: SharedRbacService) -> AuthorizationBuilder {
        AuthorizationBuilder {
            facilities: Arc::new(IdentityFacilities { db, rbac }),
            facts: BTreeMap::new(),
            mappings: BTreeMap::new(),
        }
    }
}

impl AuthorizationBuilder {
    /// 为固定资源登记唯一事实端口，供单条和批量授权读取真实对象；未知、仅功能资格或重复的资源登记均拒绝。
    pub fn facts(mut self, resource: &str, facts: Arc<dyn AccessFactsPort>) -> Result<Self> {
        if !scope_spec::actions(resource)
            .any(|permission| scope_spec::kind(permission).is_ok_and(|kind| kind != ActionKind::Function))
        {
            return Err(Error::ValidationError(format!("未知或无需 Facts 的资源：{resource}")));
        }
        if self.facts.insert(resource.into(), facts).is_some() {
            return Err(Error::ValidationError(format!("资源 Facts 重复绑定：{resource}")));
        }
        Ok(self)
    }

    /// 启动时为查询权限登记根模型及来源 Mapping，请求不能改选；不支持查询、重复登记或模型资源错配均拒绝。
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

    /// 启动时核对固定规则所需的 Facts、Mapping、来源能力与输出粒度，冻结映射并构造无请求状态的共享服务。缺失或错配时拒绝启动，不降级。
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

    // 冻结映射前检查固定权限的根对象、精确任务及查询绑定是否齐全，并确认固定对象规则可以构造。
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
