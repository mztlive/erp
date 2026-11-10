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

/// 组合根登记的数据政策；无对象操作由 AccessMode 单独表达。
#[derive(Debug, Clone)]
pub enum AccessPolicy {
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
    /// 限制登记深度和每个组合的节点数，并拒绝空组合。
    fn validate(&self, depth: usize) -> Result<()> {
        if depth > 8 {
            return Err(Error::ValidationError("授权政策嵌套超限".into()));
        }
        match self {
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

/// 启动时提供的操作登记；功能资格与接入能力分别声明。
pub struct AccessRegistration {
    pub permission: Permission,
    pub required_permissions: Vec<Permission>,
    pub access: AccessMode,
}

/// 四种接入形态分别携带必需能力；无对象操作不能携带数据政策或端口。
pub enum AccessMode {
    Operation,
    Query { data: DataAccess, mapping: Arc<dyn AccessMapping> },
    Object { data: DataAccess, facts: Arc<dyn AccessFactsPort> },
    QueryAndObject { data: DataAccess, mapping: Arc<dyn AccessMapping>, facts: Arc<dyn AccessFactsPort> },
}

/// 数据接入的主体、必需事实与政策；查询和对象检查使用同一份声明。
#[derive(Debug, Clone)]
pub struct DataAccess {
    pub subject: AccessSubject,
    pub required_facts: BTreeSet<FactKind>,
    pub policy: AccessPolicy,
}

impl AccessMode {
    /// 无对象登记固定使用直接主体，数据登记使用已声明主体。
    pub(super) fn subject(&self) -> &AccessSubject {
        match self {
            Self::Operation => &AccessSubject::Direct,
            Self::Query { data, .. } | Self::Object { data, .. } | Self::QueryAndObject { data, .. } => {
                &data.subject
            },
        }
    }

    /// 只有数据登记可进入范围解析。
    pub(super) fn data(&self) -> Result<&DataAccess> {
        match self {
            Self::Operation => Err(Error::ValidationError("无对象登记不能生成数据范围".into())),
            Self::Query { data, .. } | Self::Object { data, .. } | Self::QueryAndObject { data, .. } => {
                Ok(data)
            },
        }
    }

    /// 查询入口只能取得登记的查询能力。
    pub(super) fn mapping(&self) -> Result<&Arc<dyn AccessMapping>> {
        match self {
            Self::Query { mapping, .. } | Self::QueryAndObject { mapping, .. } => Ok(mapping),
            Self::Operation | Self::Object { .. } => {
                Err(Error::ValidationError("本操作未接入查询 Mapping".into()))
            },
        }
    }

    /// 完整对象入口只能取得登记的对象事实能力。
    pub(super) fn facts(&self) -> Result<&Arc<dyn AccessFactsPort>> {
        match self {
            Self::Object { facts, .. } | Self::QueryAndObject { facts, .. } => Ok(facts),
            Self::Operation | Self::Query { .. } => {
                Err(Error::ValidationError("操作缺少对象事实端口".into()))
            },
        }
    }
}

impl AccessRegistration {
    /// 校验具体权限及各接入形态的授权语义，结构完整性由枚举保证。
    fn validate(&self) -> Result<()> {
        if self.permission.resource().contains('*') || self.permission.action().contains('*') {
            return Err(Error::ValidationError("业务入口必须登记具体权限".into()));
        }
        match &self.access {
            AccessMode::Operation => {
                if ConsumerSpec::find(self.permission.resource()).is_some()
                    && AuthorizationPolicy::for_action(self.permission.resource(), self.permission.action())?
                        != AuthorizationPolicy::Role
                {
                    return Err(Error::ValidationError("已有对象范围的操作不能登记为无对象政策".into()));
                }
                Ok(())
            },
            AccessMode::Query { data, mapping } => {
                data.validate(&self.permission)?;
                data.validate_model(&self.permission, mapping.model())
            },
            AccessMode::Object { data, .. } => {
                data.validate(&self.permission)?;
                data.validate_object()
            },
            AccessMode::QueryAndObject { data, mapping, .. } => {
                data.validate(&self.permission)?;
                data.validate_object()?;
                if mapping.model().output != QueryOutput::Object {
                    return Err(Error::ValidationError("受限查询视图不能登记完整对象事实端口".into()));
                }
                data.validate_model(&self.permission, mapping.model())
            },
        }
    }
}

impl DataAccess {
    /// 数据政策必须证明当前动作，来源主体必须对应实际登记关系。
    fn validate(&self, permission: &Permission) -> Result<()> {
        self.policy.validate(0)?;
        self.policy.validate_entry(permission)?;
        if let AccessSubject::Source(key) = &self.subject
            && !self.policy.has_relation(key)
        {
            return Err(Error::ValidationError("来源主体缺少登记关系".into()));
        }
        Ok(())
    }

    /// 完整对象检查不能采用任一来源命中的部分读取政策。
    fn validate_object(&self) -> Result<()> {
        if self.policy.has_partial_relation() {
            return Err(Error::ValidationError("任一来源政策不能用于完整对象事实检查".into()));
        }
        Ok(())
    }

    /// 查询模型校验与对象端口独立，纯对象登记不需要实现查询方法。
    fn validate_model(&self, permission: &Permission, model: &QueryModel) -> Result<()> {
        if model.resource != permission.resource() {
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
            let key = (code(&entry.permission), entry.access.subject().clone());
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
    use crate::repository::authorization::model_mapping;

    struct Facts;
    #[async_trait]
    impl AccessFactsPort for Facts {
        async fn load(
            &self,
            _: &AccessContext,
            _: &ResourceTargets,
            _: &mut dyn Executor,
        ) -> Result<Vec<ObjectFacts>> {
            panic!("登记校验不得读取对象事实")
        }
    }

    fn operation() -> AccessRegistration {
        AccessRegistration {
            permission: Permission::parse("report:run").unwrap(),
            required_permissions: vec![],
            access: AccessMode::Operation,
        }
    }

    fn data() -> DataAccess {
        DataAccess {
            subject: AccessSubject::Direct,
            required_facts: [FactKind::Owner].into(),
            policy: AccessPolicy::Scope {
                permission: Permission::parse("sales_order:detail").unwrap(),
                read: ReadAccess::default(),
            },
        }
    }

    fn model() -> QueryModel {
        QueryModel {
            key: QueryModelKey::parse("orders").unwrap(),
            resource: "sales_order".into(),
            collection: "orders".into(),
            id_field: "id".into(),
            output: QueryOutput::Object,
            facts: [FactKind::Owner].into(),
        }
    }

    fn entry(access: AccessMode) -> AccessRegistration {
        AccessRegistration {
            permission: Permission::parse("sales_order:detail").unwrap(),
            required_permissions: vec![],
            access,
        }
    }

    fn query(data: DataAccess, model: QueryModel) -> AccessRegistration {
        entry(AccessMode::Query { data, mapping: model_mapping(model) })
    }

    fn related(mode: RelationMatch) -> DataAccess {
        DataAccess {
            policy: AccessPolicy::Related {
                relation: RelationKey::parse("sources").unwrap(),
                resource: "sales_order".into(),
                mode,
                required_facts: [FactKind::Owner].into(),
                policy: Box::new(data().policy),
            },
            ..data()
        }
    }

    fn rejects<T>(result: Result<T>, expected: &str) {
        assert!(matches!(result, Err(Error::ValidationError(message)) if message == expected));
    }

    #[test]
    fn four_modes_expose_only_their_registered_capabilities() {
        let mapping: Arc<dyn AccessMapping> = model_mapping(model());
        let facts: Arc<dyn AccessFactsPort> = Arc::new(Facts);
        let cases = [
            (operation(), false, false),
            (entry(AccessMode::Query { data: data(), mapping: mapping.clone() }), true, false),
            (entry(AccessMode::Object { data: data(), facts: facts.clone() }), false, true),
            (
                entry(AccessMode::QueryAndObject {
                    data: data(),
                    mapping: mapping.clone(),
                    facts: facts.clone(),
                }),
                true,
                true,
            ),
        ];
        for (entry, queries, objects) in cases {
            let permission = entry.permission.clone();
            let registry = AuthorizationRegistry::new(vec![entry]).unwrap();
            let registered = registry.entry(&permission, &AccessSubject::Direct).unwrap();
            if queries {
                assert!(Arc::ptr_eq(registered.access.mapping().unwrap(), &mapping));
            } else {
                rejects(registered.access.mapping(), "本操作未接入查询 Mapping");
            }
            if objects {
                assert!(Arc::ptr_eq(registered.access.facts().unwrap(), &facts));
            } else {
                rejects(registered.access.facts(), "操作缺少对象事实端口");
            }
            if queries || objects {
                assert_eq!(registered.access.data().unwrap().required_facts, [FactKind::Owner].into());
            } else {
                rejects(registered.access.data(), "无对象登记不能生成数据范围");
            }
        }
    }

    #[test]
    fn directory_rejects_duplicate_unknown_and_wildcard_entries() {
        rejects(AuthorizationRegistry::new(vec![operation(), operation()]), "授权操作重复登记");
        let registry = AuthorizationRegistry::new(vec![operation()]).unwrap();
        assert!(registry.entry(&Permission::parse("report:run").unwrap(), &AccessSubject::Direct).is_ok());
        rejects(
            registry.entry(&Permission::parse("report:delete").unwrap(), &AccessSubject::Direct),
            "操作或查询主体尚未接入统一权限",
        );
        for permission in ["sales_order:*", "*:detail"] {
            let mut entry = operation();
            entry.permission = Permission::parse(permission).unwrap();
            rejects(AuthorizationRegistry::new(vec![entry]), "业务入口必须登记具体权限");
        }
    }

    #[test]
    fn subjects_remain_distinct_in_the_directory() {
        let key = QueryModelKey::parse("orders").unwrap();
        let view = DataAccess { subject: AccessSubject::View(key.clone()), ..data() };
        let registry =
            AuthorizationRegistry::new(vec![query(data(), model()), query(view, model())]).unwrap();
        let permission = Permission::parse("sales_order:detail").unwrap();
        assert!(registry.entry(&permission, &AccessSubject::Direct).is_ok());
        assert!(registry.entry(&permission, &AccessSubject::View(key)).is_ok());
        rejects(
            registry.entry(&permission, &AccessSubject::Source(RelationKey::parse("sources").unwrap())),
            "操作或查询主体尚未接入统一权限",
        );
    }

    #[test]
    fn reading_scope_cannot_be_registered_as_original_object_write_policy() {
        let mut entry = entry(AccessMode::Object { data: data(), facts: Arc::new(Facts) });
        entry.permission = Permission::parse("sales_order:update").unwrap();
        rejects(AuthorizationRegistry::new(vec![entry]), "同资源授权必须使用当前操作的范围");
        assert!(data().policy.validate_entry(&Permission::parse("invoice:create").unwrap()).is_ok());
    }

    #[test]
    fn operation_registration_cannot_remove_existing_object_requirements() {
        for permission in ["sales_order:update", "invoice:detail", "approval_instance:decide", "product:list"]
        {
            let mut entry = operation();
            entry.permission = Permission::parse(permission).unwrap();
            rejects(AuthorizationRegistry::new(vec![entry]), "已有对象范围的操作不能登记为无对象政策");
        }
        for permission in ["sales_order:unknown", "integration_error_task:unknown"] {
            let mut entry = operation();
            entry.permission = Permission::parse(permission).unwrap();
            assert!(matches!(AuthorizationRegistry::new(vec![entry]), Err(Error::ValidationError(_))));
        }
        let mut entry = operation();
        entry.permission = Permission::parse("integration_error_task:create").unwrap();
        assert!(AuthorizationRegistry::new(vec![entry]).is_ok());
    }

    #[test]
    fn partial_sources_require_a_restricted_view_without_object_capability() {
        let data = related(RelationMatch::Any);
        rejects(
            AuthorizationRegistry::new(vec![query(data.clone(), model())]),
            "任一来源政策不能放行完整对象模型",
        );
        for output in [
            QueryOutput::Share { fields: ["id".into()].into() },
            QueryOutput::Existence { fields: ["id".into()].into() },
        ] {
            let model = QueryModel { output, ..model() };
            rejects(
                AuthorizationRegistry::new(vec![query(data.clone(), model.clone())]),
                "受限查询只能登记为只读视图",
            );
            let view = DataAccess { subject: AccessSubject::View(model.key.clone()), ..data.clone() };
            assert!(AuthorizationRegistry::new(vec![query(view.clone(), model.clone())]).is_ok());
            for access in [
                AccessMode::Object { data: view.clone(), facts: Arc::new(Facts) },
                AccessMode::QueryAndObject {
                    data: view,
                    mapping: model_mapping(model),
                    facts: Arc::new(Facts),
                },
            ] {
                rejects(
                    AuthorizationRegistry::new(vec![entry(access)]),
                    "任一来源政策不能用于完整对象事实检查",
                );
            }
        }
    }

    #[test]
    fn restricted_output_cannot_be_combined_with_object_facts() {
        let model = QueryModel { output: QueryOutput::Share { fields: ["id".into()].into() }, ..model() };
        let data = DataAccess { subject: AccessSubject::View(model.key.clone()), ..data() };
        assert!(AuthorizationRegistry::new(vec![query(data.clone(), model.clone())]).is_ok());
        let entry =
            entry(AccessMode::QueryAndObject { data, mapping: model_mapping(model), facts: Arc::new(Facts) });
        rejects(AuthorizationRegistry::new(vec![entry]), "受限查询视图不能登记完整对象事实端口");
    }

    #[test]
    fn source_subject_requires_its_registered_relation() {
        let subject = AccessSubject::Source(RelationKey::parse("sources").unwrap());
        let data = DataAccess { subject, ..related(RelationMatch::AllNonEmpty) };
        assert!(
            AuthorizationRegistry::new(vec![entry(AccessMode::Object {
                data: data.clone(),
                facts: Arc::new(Facts)
            })])
            .is_ok()
        );
        let invalid =
            DataAccess { subject: AccessSubject::Source(RelationKey::parse("other").unwrap()), ..data };
        rejects(
            AuthorizationRegistry::new(vec![entry(AccessMode::Object {
                data: invalid,
                facts: Arc::new(Facts),
            })]),
            "来源主体缺少登记关系",
        );
    }

    #[test]
    fn query_model_must_match_resource_view_and_required_facts() {
        for (model, message) in [
            (QueryModel { resource: "invoice".into(), ..model() }, "根查询模型与入口资源不一致"),
            (QueryModel { facts: BTreeSet::new(), ..model() }, "查询模型未提供登记的必需事实"),
        ] {
            rejects(AuthorizationRegistry::new(vec![query(data(), model)]), message);
        }
        let view =
            DataAccess { subject: AccessSubject::View(QueryModelKey::parse("other").unwrap()), ..data() };
        rejects(AuthorizationRegistry::new(vec![query(view, model())]), "查询主体与 Mapping 模型标识不一致");
        let invalid = QueryModel { collection: "$orders".into(), ..model() };
        rejects(AuthorizationRegistry::new(vec![query(data(), invalid)]), "授权查询模型非法");
    }

    #[test]
    fn policy_combinations_keep_nonempty_and_depth_limits() {
        for policy in [
            AccessPolicy::All(vec![]),
            AccessPolicy::Any(vec![]),
            AccessPolicy::All(vec![AccessPolicy::Assigned; 33]),
        ] {
            let data = DataAccess { policy, ..data() };
            rejects(
                AuthorizationRegistry::new(vec![query(data, model())]),
                "授权组合必须包含 1 至 32 个条件",
            );
        }
        let mut policy = AccessPolicy::Participant;
        for _ in 0..9 {
            policy = AccessPolicy::Any(vec![policy]);
        }
        rejects(
            AuthorizationRegistry::new(vec![query(DataAccess { policy, ..data() }, model())]),
            "授权政策嵌套超限",
        );
    }

    #[test]
    fn each_read_view_can_choose_supplements_but_writes_cannot_enable_them() {
        for read in [ReadAccess::default(), ReadAccess { history: true, records: true }] {
            let policy =
                AccessPolicy::Scope { permission: Permission::parse("sales_order:detail").unwrap(), read };
            assert!(
                AuthorizationRegistry::new(vec![query(DataAccess { policy, ..data() }, model())]).is_ok()
            );
        }
        let permission = Permission::parse("sales_order:update").unwrap();
        let policy = AccessPolicy::Scope {
            permission: permission.clone(),
            read: ReadAccess { history: false, records: true },
        };
        let mut entry =
            entry(AccessMode::Object { data: DataAccess { policy, ..data() }, facts: Arc::new(Facts) });
        entry.permission = permission;
        rejects(AuthorizationRegistry::new(vec![entry]), "写操作不能登记读取补充");
    }
}
