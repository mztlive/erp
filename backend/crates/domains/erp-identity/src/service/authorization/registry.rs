use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::entity::access_control::authorization_policy::AuthorizationPolicy;
use crate::entity::access_control::consumer_spec::ConsumerSpec;
use crate::entity::authorization::{
    AccessSubject, FactKind, QueryModel, QueryOutput, ReadAccess, RelationKey, RelationMatch,
};
use crate::ports::{AccessFactsPort, AccessMapping};
use crate::service::access_control::consumers::registration;
use crate::{Error, Permission, Result};

/// 组合根登记的政策。调用参数不能提供政策或开关。
#[derive(Debug, Clone)]
pub enum AccessPolicy {
    Operation,
    Scope {
        permission: Permission,
        read: ReadAccess,
    },
    Assigned,
    Participant,
    All(Vec<Self>),
    Any(Vec<Self>),
    Related {
        relation: RelationKey,
        resource: String,
        mode: RelationMatch,
        required_facts: BTreeSet<FactKind>,
        policy: Box<Self>,
    },
}

impl AccessPolicy {
    /// 任一来源只证明受限查询，不能证明整个对象可操作。
    pub(super) fn has_partial_relation(&self) -> bool {
        match self {
            Self::Related { mode, policy, .. } => {
                *mode == RelationMatch::Any || policy.has_partial_relation()
            },
            Self::All(items) | Self::Any(items) => items.iter().any(Self::has_partial_relation),
            _ => false,
        }
    }

    /// Source 主体必须绑定根模型实际声明的关系。
    fn has_relation(&self, key: &RelationKey) -> bool {
        match self {
            Self::Related { relation, .. } => relation == key,
            Self::All(items) | Self::Any(items) => items.iter().any(|item| item.has_relation(key)),
            _ => false,
        }
    }
    /// 同资源必须证明当前动作的范围，不能登记读取范围放行修改。
    fn validate_entry(&self, entry: &Permission) -> Result<()> {
        match self {
            Self::Scope { permission, .. }
                if permission.resource() == entry.resource() && permission != entry =>
            {
                Err(Error::ValidationError("同资源授权必须使用当前操作的范围".into()))
            },
            Self::All(items) | Self::Any(items) => {
                for item in items {
                    item.validate_entry(entry)?;
                }
                Ok(())
            },
            Self::Related { policy, .. } => policy.validate_entry(entry),
            _ => Ok(()),
        }
    }
    /// 限制登记深度和节点数，并拒绝空组合与嵌套无对象放行。
    fn validate(&self, depth: usize) -> Result<()> {
        if depth > 8 {
            return Err(Error::ValidationError("授权政策嵌套超限".into()));
        }
        match self {
            Self::Operation if depth != 0 => Err(Error::ValidationError("无对象政策不能嵌入数据规则".into())),
            Self::Scope { permission, read } => {
                let consumer = registration(permission.resource(), permission.action())?;
                read.validate(permission, consumer.allows_history())
            },
            Self::All(items) | Self::Any(items) => {
                if items.is_empty() || items.len() > 32 {
                    return Err(Error::ValidationError("授权组合必须包含 1 至 32 个条件".into()));
                }
                for item in items {
                    item.validate(depth + 1)?;
                }
                Ok(())
            },
            Self::Related { policy, .. } => policy.validate(depth + 1),
            _ => Ok(()),
        }
    }
}

/// 启动时提供的操作登记；对象检查另需事实端口，查询接入只需映射。
pub struct AccessRegistration {
    pub permission: Permission,
    pub subject: AccessSubject,
    pub required_permissions: Vec<Permission>,
    pub required_facts: BTreeSet<FactKind>,
    pub policy: AccessPolicy,
    pub mapping: Option<Arc<dyn AccessMapping>>,
    pub facts: Option<Arc<dyn AccessFactsPort>>,
}

impl AccessRegistration {
    /// 检查登记形态，阻止无对象入口用于需要对象的数据政策。
    fn validate(&self) -> Result<()> {
        self.policy.validate(0)?;
        self.policy.validate_entry(&self.permission)?;
        if self.permission.resource().contains('*') || self.permission.action().contains('*') {
            return Err(Error::ValidationError("业务入口必须登记具体权限".into()));
        }
        if matches!(self.policy, AccessPolicy::Operation)
            && ConsumerSpec::find(self.permission.resource()).is_some()
            && AuthorizationPolicy::for_action(self.permission.resource(), self.permission.action())?
                != AuthorizationPolicy::Role
        {
            return Err(Error::ValidationError("已有对象范围的操作不能登记为无对象政策".into()));
        }
        if matches!(self.policy, AccessPolicy::Operation) {
            if self.subject != AccessSubject::Direct
                || self.mapping.is_some()
                || self.facts.is_some()
                || !self.required_facts.is_empty()
            {
                return Err(Error::ValidationError("无对象登记形态非法".into()));
            }
            return Ok(());
        }
        if self.mapping.is_none() && self.facts.is_none() {
            return Err(Error::ValidationError("数据政策至少提供查询 Mapping 或对象事实端口".into()));
        }
        if self.facts.is_some() && self.policy.has_partial_relation() {
            return Err(Error::ValidationError("任一来源政策不能用于完整对象事实检查".into()));
        }
        if let AccessSubject::Source(key) = &self.subject
            && !self.policy.has_relation(key)
        {
            return Err(Error::ValidationError("来源主体缺少登记关系".into()));
        }
        if let Some(mapping) = &self.mapping {
            self.validate_model(mapping.model())?;
        }
        Ok(())
    }

    /// 查询模型校验与对象端口独立，纯对象登记不需要实现查询方法。
    fn validate_model(&self, model: &QueryModel) -> Result<()> {
        if model.resource != self.permission.resource() {
            return Err(Error::ValidationError("根查询模型与入口资源不一致".into()));
        }
        if let AccessSubject::View(key) = &self.subject
            && key != &model.key
        {
            return Err(Error::ValidationError("查询主体与 Mapping 模型标识不一致".into()));
        }
        if !self.required_facts.is_subset(&model.facts) {
            return Err(Error::ValidationError("查询模型未提供登记的必需事实".into()));
        }
        if self.facts.is_some() && model.output != QueryOutput::Object {
            return Err(Error::ValidationError("受限查询视图不能登记完整对象事实端口".into()));
        }
        self.validate_output(&model.output)?;
        model.validate()
    }

    /// 部分读取必须使用独立视图和字段白名单。
    fn validate_output(&self, output: &QueryOutput) -> Result<()> {
        match output {
            QueryOutput::Object if self.policy.has_partial_relation() => {
                Err(Error::ValidationError("任一来源政策不能放行完整对象模型".into()))
            },
            QueryOutput::Share { .. } | QueryOutput::Existence { .. }
                if !matches!(&self.subject, AccessSubject::View(_)) =>
            {
                Err(Error::ValidationError("受限查询只能登记为只读视图".into()))
            },
            _ => Ok(()),
        }
    }
}

/// 不可变操作目录；缺少登记一律拒绝，禁止从请求创建登记。
#[derive(Default)]
pub struct AuthorizationRegistry {
    entries: BTreeMap<(String, AccessSubject), Arc<AccessRegistration>>,
}

impl AuthorizationRegistry {
    /// 校验并冻结服务端目录。
    /// # 参数
    /// entries 为组合根提供的全部已接入操作。
    /// # 返回
    /// 不可变授权目录。
    /// # 错误
    /// 重复或不完整登记拒绝。
    pub fn new(entries: Vec<AccessRegistration>) -> Result<Self> {
        let mut registry = Self::default();
        for entry in entries {
            entry.validate()?;
            let key = (code(&entry.permission), entry.subject.clone());
            if registry.entries.insert(key, Arc::new(entry)).is_some() {
                return Err(Error::ValidationError("授权操作重复登记".into()));
            }
        }
        Ok(registry)
    }

    /// 返回唯一的完整登记，不能按权限格式推断默认放行。
    pub(super) fn entry(
        &self,
        permission: &Permission,
        subject: &AccessSubject,
    ) -> Result<Arc<AccessRegistration>> {
        self.entries
            .get(&(code(permission), subject.clone()))
            .cloned()
            .ok_or_else(|| Error::ValidationError("操作或查询主体尚未接入统一权限".into()))
    }
}

/// 权限键与主体共同唯一标识登记。
fn code(permission: &Permission) -> String {
    format!("{}:{}", permission.resource(), permission.action())
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use persistence_core::Executor;

    use super::*;
    use crate::entity::authorization::{AccessContext, ObjectFacts, QueryModelKey, ResourceTargets};

    struct Facts;
    #[async_trait]
    impl AccessFactsPort for Facts {
        async fn load(
            &self,
            _: &AccessContext,
            _: &ResourceTargets,
            _: &mut dyn Executor,
        ) -> Result<Vec<ObjectFacts>> {
            Ok(vec![])
        }
    }

    fn operation() -> AccessRegistration {
        AccessRegistration {
            permission: Permission::parse("report:run").unwrap(),
            subject: AccessSubject::Direct,
            required_permissions: vec![],
            required_facts: BTreeSet::new(),
            policy: AccessPolicy::Operation,
            mapping: None,
            facts: None,
        }
    }

    #[test]
    fn directory_rejects_duplicate_unknown_and_malformed_registrations() {
        assert!(AuthorizationRegistry::new(vec![operation(), operation()]).is_err());
        let registry = AuthorizationRegistry::new(vec![operation()]).unwrap();
        assert!(registry.entry(&Permission::parse("report:run").unwrap(), &AccessSubject::Direct).is_ok());
        assert!(
            registry.entry(&Permission::parse("report:delete").unwrap(), &AccessSubject::Direct).is_err()
        );
        let mut invalid = operation();
        invalid.policy = AccessPolicy::All(vec![]);
        assert!(AuthorizationRegistry::new(vec![invalid]).is_err());
        let mut missing_mapping = operation();
        missing_mapping.policy = AccessPolicy::Scope {
            permission: Permission::parse("sales_order:detail").unwrap(),
            read: ReadAccess::default(),
        };
        assert!(AuthorizationRegistry::new(vec![missing_mapping]).is_err());
    }

    #[test]
    fn reading_scope_cannot_be_registered_as_original_object_write_policy() {
        let policy = AccessPolicy::Scope {
            permission: Permission::parse("sales_order:detail").unwrap(),
            read: ReadAccess::default(),
        };
        assert!(policy.validate_entry(&Permission::parse("sales_order:update").unwrap()).is_err());
        assert!(policy.validate_entry(&Permission::parse("invoice:create").unwrap()).is_ok());
        assert!(AccessPolicy::All(vec![AccessPolicy::Operation]).validate(0).is_err());
    }

    #[test]
    fn operation_registration_cannot_remove_existing_object_requirements() {
        for permission in [
            "sales_order:update",
            "invoice:detail",
            "approval_instance:decide",
            "product:list",
            "sales_order:unknown",
            "integration_error_task:unknown",
        ] {
            let mut entry = operation();
            entry.permission = Permission::parse(permission).unwrap();
            assert!(AuthorizationRegistry::new(vec![entry]).is_err());
        }
        let mut entry = operation();
        entry.permission = Permission::parse("integration_error_task:create").unwrap();
        assert!(AuthorizationRegistry::new(vec![entry]).is_ok());
    }

    #[test]
    fn partial_sources_require_restricted_read_only_view() {
        let mut entry = operation();
        entry.policy = AccessPolicy::Related {
            relation: RelationKey::parse("sources").unwrap(),
            resource: "sales_order".into(),
            mode: RelationMatch::Any,
            required_facts: BTreeSet::new(),
            policy: Box::new(AccessPolicy::Scope {
                permission: Permission::parse("sales_order:detail").unwrap(),
                read: ReadAccess::default(),
            }),
        };
        assert!(entry.validate_output(&QueryOutput::Object).is_err());
        let output = QueryOutput::Share { fields: ["id".into()].into() };
        assert!(entry.validate_output(&output).is_err());
        entry.subject = AccessSubject::View(QueryModelKey::parse("shares").unwrap());
        assert!(entry.validate_output(&output).is_ok());
        assert!(entry.policy.has_relation(&RelationKey::parse("sources").unwrap()));
        assert!(!entry.policy.has_relation(&RelationKey::parse("unknown").unwrap()));
    }
    #[test]
    fn object_registration_requires_facts_without_query_mapping() {
        let mut entry = operation();
        entry.permission = Permission::parse("sales_order:update").unwrap();
        entry.policy =
            AccessPolicy::Scope { permission: entry.permission.clone(), read: ReadAccess::default() };
        entry.required_facts = [FactKind::Owner].into();
        assert!(matches!(entry.validate(), Err(Error::ValidationError(_))));
        entry.facts = Some(Arc::new(Facts));
        let permission = entry.permission.clone();
        let registry = AuthorizationRegistry::new(vec![entry]).unwrap();
        let registered = registry.entry(&permission, &AccessSubject::Direct).unwrap();
        assert!(registered.mapping.is_none());
        assert!(registered.facts.is_some());
    }

    #[test]
    fn each_read_view_can_choose_supplements_but_writes_cannot_enable_them() {
        let permission = Permission::parse("sales_order:detail").unwrap();
        for read in [ReadAccess::default(), ReadAccess { history: true, records: true }] {
            let policy = AccessPolicy::Scope { permission: permission.clone(), read };
            assert!(policy.validate(0).is_ok());
            assert!(policy.validate_entry(&permission).is_ok());
        }
        let policy = AccessPolicy::Scope {
            permission: Permission::parse("sales_order:update").unwrap(),
            read: ReadAccess { history: false, records: true },
        };
        assert!(policy.validate(0).is_err());
    }
}
