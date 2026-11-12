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
    /// 绑定身份设施，开始构造完整授权服务。
    /// # 参数
    /// db、rbac 为同一个身份数据库及其共享 RBAC 服务。
    /// # 返回
    /// 尚未构造服务的 Builder；不读取数据库。
    /// # 错误
    /// 构造不返回错误，引用及能力校验在绑定和 build 时执行。
    pub fn builder(db: Database, rbac: SharedRbacService) -> AuthorizationBuilder {
        AuthorizationBuilder {
            facilities: Arc::new(IdentityFacilities { db, rbac }),
            facts: BTreeMap::new(),
            mappings: BTreeMap::new(),
        }
    }
}

impl AuthorizationBuilder {
    /// 绑定资源唯一的根事实读取器。
    /// # 参数
    /// resource 为已接入固定规则的资源；facts 为拥有领域的读取实现。
    /// # 返回
    /// 包含该绑定的 Builder。
    /// # 错误
    /// 未知资源、仅功能资源或重复绑定返回接入错误。
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

    /// 为查询权限绑定唯一入口；业务不能在调用时替换 Mapping。
    /// # 参数
    /// permission 为具体查询权限，mapping 为固定根及来源描述。
    /// # 返回
    /// 包含该查询绑定的 Builder。
    /// # 错误
    /// 未知或不支持查询的权限、重复入口、模型错配返回接入错误。
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

    /// 校验全部固定规则的必要绑定并冻结查询描述。
    /// # 参数
    /// 消耗本 Builder；完整业务绑定由组合根提供。
    /// # 返回
    /// 不保存请求状态的统一授权服务。
    /// # 错误
    /// 目录错误、缺少 Facts/Mapping、来源能力缺失及输出错配均拒绝构造。
    pub fn build(self) -> Result<AuthorizationService> {
        scope_spec::validate_catalog()?;
        self.ensure_complete()?;
        let mut cache = Vec::new();
        let mut mappings = BTreeMap::new();
        for (permission, mapping) in self.mappings {
            let mapping = FrozenMapping::freeze(mapping, &mut cache, 0)?;
            let rule = rules::binding_condition(&permission, &mapping.model().output)?;
            validate_rule(&rule, mapping.as_ref(), 0)?;
            mappings.insert(permission, mapping);
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
            if scope_spec::kind(permission)? == ActionKind::Function {
                continue;
            }
            if !self.facts.contains_key(permission.resource()) {
                return Err(Error::ValidationError(format!("缺少资源 Facts：{}", permission.resource())));
            }
            if rules::query_supported(permission)? && !self.mappings.contains_key(permission) {
                return Err(Error::ValidationError(format!("缺少查询 Mapping：{permission}")));
            }
            // 同时验证没有查询入口的命令具有可执行的固定目标规则。
            rules::binding_condition(permission, &QueryOutput::Object)?;
        }
        Ok(())
    }
}
