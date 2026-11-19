use std::collections::HashMap;
use std::io;

use async_trait::async_trait;
use casbin::error::AdapterError;
use casbin::{Adapter, Filter, Model};
use futures_util::StreamExt;
use mongodb::Database;
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Error as PersistenceError, Executor, ResetRetention, Result, mongo_ops};
use serde::{Deserialize, Serialize};

use crate::entity::role::ROOT_ROLE_ID;

pub const CASBIN_RULES: &str = "casbin_rules";
const CASBIN_POLICY_STATE: &str = "casbin_policy_state";
const POLICY_STATE_ID: &str = "policy";

// 协调 Casbin 策略写入的 MongoDB 全局版本文档。
#[derive(Debug, Deserialize)]
struct CasbinPolicyState {
    // 已提交的策略修订号，用于发现策略变化和并发写冲突。
    revision: i64,
}

// 一条持久化的 Casbin 权限或分组规则。
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CasbinRule {
    // 根据区段、断言类型和有序列值生成的稳定 MongoDB 主键。
    #[serde(rename = "_id")]
    id: String,
    // 模型区段，通常为权限 `p` 或角色分组 `g`。
    sec: String,
    // 区段内的 Casbin 断言类型，决定规则装入的断言。
    ptype: String,
    // 按断言定义排列的规则列值，不可重排。
    values: Vec<String>,
}

impl CasbinRule {
    // 构造待落库的规则文档，并从完整有序规则生成稳定主键。
    fn new(sec: &str, ptype: &str, values: Vec<String>) -> Self {
        Self { id: Self::id(sec, ptype, &values), sec: sec.to_string(), ptype: ptype.to_string(), values }
    }

    // 用单元分隔符连接区段、断言类型及有序列值，生成查询和持久化共用的 `_id`。
    fn id(sec: &str, ptype: &str, values: &[String]) -> String {
        format!("{sec}\u{1f}{ptype}\u{1f}{}", values.join("\u{1f}"))
    }
}

/// 为 Casbin 全量加载 MongoDB 规则，并为业务事务提供规则读写和版本操作。
#[derive(Clone)]
pub struct MongoCasbinAdapter {
    // 保存 Casbin 规则与全局修订的 MongoDB 数据库。
    db: Database,
}

/// 由同一执行器读取的主体直接策略，不含缓存、继承结果或跨事务证明。
#[derive(Default)]
pub(crate) struct SubjectPolicy {
    /// 主体直接绑定的父角色键，排序并去重。
    pub(crate) roles: Vec<String>,
    /// 主体直接拥有的 `(资源, 动作)` 权限对，排序并去重。
    pub(crate) permissions: Vec<(String, String)>,
}

impl MongoCasbinAdapter {
    /// 绑定规则和策略版本所在的数据库。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 全库重置时在调用方事务内登记 `admin` 的直接 root 绑定、root 的全量权限及版本文档，不保留其他授权。按 `account_id` 构造主体键，读取或登记失败直接返回。
    pub(crate) async fn retain_root_authorization(
        &self,
        account_id: &str,
        retained: &mut ResetRetention,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let root = format!("role:{ROOT_ROLE_ID}");
        let subject = format!("user:admin:{account_id}");
        for (sec, values) in [("p", vec![root.clone(), "*".into(), "*".into()]), ("g", vec![subject, root])] {
            let rule = CasbinRule::new(sec, sec, values);
            let filter = doc! { "_id": rule.id, "sec": sec, "ptype": sec, "values": rule.values };
            retained.require(&self.db, CASBIN_RULES, filter, executor).await?;
        }
        retained.require(&self.db, CASBIN_POLICY_STATE, Self::policy_revision_filter(), executor).await?;
        Ok(())
    }

    // 获取 Casbin 规则的类型化集合句柄，供读取和改写策略使用。
    fn collection(&self) -> mongodb::Collection<CasbinRule> {
        self.db.collection(CASBIN_RULES)
    }

    // 获取全局策略版本集合的类型化句柄，供取证和版本写入使用。
    fn policy_state_collection(&self) -> mongodb::Collection<CasbinPolicyState> {
        self.db.collection(CASBIN_POLICY_STATE)
    }

    // 定位唯一的全局版本文档，供读取、更新和重置保留流程共用。
    fn policy_revision_filter() -> Document {
        doc! { "_id": POLICY_STATE_ID }
    }

    // 构造将全局策略版本原子递增一次的 BSON 更新文档。
    fn policy_revision_increment() -> Document {
        doc! { "$inc": { "revision": 1_i64 } }
    }

    // 将底层错误包装为 Casbin 适配器错误，保留原错误作为来源。
    fn adapter_error(error: impl std::error::Error + Send + Sync + 'static) -> casbin::Error {
        AdapterError(Box::new(error)).into()
    }

    // 将持久化规则装入模型中匹配的区段及断言；模型未登记对应项时跳过。
    fn load_rule(model: &mut dyn Model, rule: CasbinRule) {
        let Some(section) = model.get_mut_model().get_mut(&rule.sec) else {
            return;
        };
        let Some(assertion) = section.get_mut(&rule.ptype) else {
            return;
        };
        assertion.get_mut_policy().insert(rule.values);
    }

    // 按 `p` 规则第一列筛选角色直接权限，供权限读取或清理使用。
    fn role_permission_filter(role_key: &str) -> Document {
        doc! {
            "sec": "p",
            "ptype": "p",
            "values.0": role_key,
        }
    }

    // 按 `g` 规则第二列筛选绑定到目标角色的主体，避免误删该角色自身向上的绑定。
    fn role_binding_filter(role_key: &str) -> Document {
        doc! {
            "sec": "g",
            "ptype": "g",
            "values.1": role_key,
        }
    }

    // 按 `g` 规则第一列筛选指定主体的直接角色绑定。
    fn subject_role_filter(subject: &str) -> Document {
        doc! {
            "sec": "g",
            "ptype": "g",
            "values.0": subject,
        }
    }

    // 从绑定行第二列提取角色键，复用已有字符串并排序去重。
    fn subject_role_keys(rules: Vec<CasbinRule>) -> Vec<String> {
        let mut role_keys =
            rules.into_iter().filter_map(|rule| rule.values.into_iter().nth(1)).collect::<Vec<_>>();
        role_keys.sort();
        role_keys.dedup();
        role_keys
    }

    // 把目标角色的资源和动作对编码为 `[角色, 资源, 动作]` 顺序的 `p` 规则。
    fn role_permission_rules(role_key: &str, permissions: &[(String, String)]) -> Vec<CasbinRule> {
        permissions
            .iter()
            .map(|(resource, action)| {
                CasbinRule::new("p", "p", vec![role_key.to_string(), resource.clone(), action.clone()])
            })
            .collect()
    }

    /// 用调用方执行器读取已提交的单调递增策略修订；历史库没有版本文档时返回 0，查询失败或持久化版本非法时返回错误。
    pub async fn policy_revision(&self, executor: &mut dyn Executor) -> Result<u64> {
        let state =
            mongo_ops::find_one(&self.policy_state_collection(), Self::policy_revision_filter(), executor)
                .await?;
        let Some(state) = state else {
            return Ok(0);
        };
        u64::try_from(state.revision)
            .map_err(|_| PersistenceError::EntityMetadataOutOfRange("casbin policy revision"))
    }

    /// 在调用方快照直接读取主体的 `g` 规则，返回排序去重的第二列角色键，不以 Enforcer 缓存替代；查询失败返回错误。
    pub async fn subject_roles(&self, subject: &str, executor: &mut dyn Executor) -> Result<Vec<String>> {
        let rules = mongo_ops::find_many(
            &self.collection(),
            Self::subject_role_filter(subject),
            FindOptions::default(),
            executor,
        )
        .await?;
        Ok(Self::subject_role_keys(rules))
    }

    /// 沿原执行器批量读取 `p`、`g` 规则，按主体返回已去重的直接权限和父角色；继承由调用方逐层展开。空输入不查库，无规则主体不出现在结果中，读取失败或任一行损坏时拒绝整批。
    pub(crate) async fn subject_policies(
        &self,
        subjects: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, SubjectPolicy>> {
        if subjects.is_empty() {
            return Ok(HashMap::new());
        }
        let rows = mongo_ops::find_many(
            &self.collection(),
            doc! {
                "values.0": { "$in": subjects },
                "$or": [{ "sec": "p", "ptype": "p" }, { "sec": "g", "ptype": "g" }],
            },
            FindOptions::default(),
            executor,
        )
        .await?;

        Self::group_subject_policies(rows)
    }

    // 按原主体拆分权限和父角色并排序去重；列数或规则类型错误时拒绝整批。
    fn group_subject_policies(rows: Vec<CasbinRule>) -> Result<HashMap<String, SubjectPolicy>> {
        let mut grouped: HashMap<String, SubjectPolicy> = HashMap::new();
        for row in rows {
            match (row.sec.as_str(), row.ptype.as_str(), row.values.as_slice()) {
                ("p", "p", [subject, resource, action]) => {
                    grouped
                        .entry(subject.clone())
                        .or_default()
                        .permissions
                        .push((resource.clone(), action.clone()));
                },
                ("g", "g", [subject, parent]) => {
                    grouped.entry(subject.clone()).or_default().roles.push(parent.clone());
                },
                _ => return Err(PersistenceError::EntityMetadataOutOfRange("Casbin subject policy")),
            }
        }

        for policy in grouped.values_mut() {
            policy.roles.sort();
            policy.roles.dedup();
            policy.permissions.sort();
            policy.permissions.dedup();
        }
        Ok(grouped)
    }

    /// 沿原执行器读取直接绑定目标角色的主体，最多读取 1001 条绑定并返回排序去重的主体键，用于判断继承和影响规模；查询失败或绑定缺少主体时拒绝。
    pub(crate) async fn role_subjects(
        &self,
        role_key: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        let rows = mongo_ops::find_many(
            &self.collection(),
            Self::role_binding_filter(role_key),
            FindOptions::builder().limit(1001).build(),
            executor,
        )
        .await?;
        let mut ids = Vec::new();
        for row in rows {
            let Some(subject) = row.values.first() else {
                return Err(PersistenceError::EntityMetadataOutOfRange("Casbin role binding"));
            };
            ids.push(subject.to_owned());
        }
        ids.sort();
        ids.dedup();
        Ok(ids)
    }

    /// 在原事务内读取角色的直接权限，返回排序去重的资源动作对，不展开继承；读取失败或策略行损坏时拒绝。
    pub(crate) async fn role_permissions(
        &self,
        role_key: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<(String, String)>> {
        let rows = mongo_ops::find_many(
            &self.collection(),
            Self::role_permission_filter(role_key),
            FindOptions::default(),
            executor,
        )
        .await?;

        let mut permissions = Vec::new();
        for row in rows {
            let [_, resource, action] = row.values.as_slice() else {
                return Err(PersistenceError::EntityMetadataOutOfRange("Casbin role permission"));
            };
            permissions.push((resource.clone(), action.clone()));
        }
        permissions.sort();
        permissions.dedup();
        Ok(permissions)
    }

    /// 在调用方策略写事务内递增全局修订；所有实例争用同一文档，使并发写入形成 MongoDB 事务冲突而非各自提交。版本更新失败时返回数据库错误。
    pub async fn bump_policy_revision(&self, executor: &mut dyn Executor) -> Result<()> {
        mongo_ops::update_one(
            &self.policy_state_collection(),
            Self::policy_revision_filter(),
            Self::policy_revision_increment(),
            true,
            executor,
        )
        .await?;
        Ok(())
    }

    /// 在策略写事务内用比较并交换递增修订，将事务前取得的授权快照绑定到最终写入。预期版本变化时返回乐观锁冲突并由调用方回滚；BSON 溢出或数据库失败同样返回错误。
    pub async fn bump_policy_revision_if_matches(
        &self,
        expected_revision: u64,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let expected_revision = i64::try_from(expected_revision)
            .map_err(|_| PersistenceError::EntityMetadataOutOfRange("casbin policy revision"))?;
        let result = mongo_ops::update_one(
            &self.policy_state_collection(),
            doc! {
                "_id": POLICY_STATE_ID,
                "revision": expected_revision,
            },
            Self::policy_revision_increment(),
            expected_revision == 0,
            executor,
        )
        .await?;

        if result.matched_count == 0 && result.upserted_id.is_none() {
            return Err(PersistenceError::OptimisticLockingError);
        }
        Ok(())
    }

    /// 先清除主体旧绑定，再写入目标 Casbin 角色键。调用方必须用同一事务覆盖账号、Profile 和绑定；任一删除或写入失败直接返回。
    pub async fn replace_subject_roles(
        &self,
        subject: &str,
        role_keys: &[String],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let collection = self.collection();
        mongo_ops::delete_many(
            &collection,
            doc! {
                "sec": "g",
                "ptype": "g",
                "values.0": subject,
            },
            &mut *executor,
        )
        .await?;

        let rules = role_keys
            .iter()
            .map(|role| CasbinRule::new("g", "g", vec![subject.to_string(), role.clone()]))
            .collect::<Vec<_>>();
        mongo_ops::insert_many(&collection, rules, executor).await
    }

    /// 清除指定主体的全部角色绑定；多步骤用例使用调用方事务，删除失败直接返回。
    pub async fn clear_subject_roles(&self, subject: &str, executor: &mut dyn Executor) -> Result<()> {
        self.replace_subject_roles(subject, &[], executor).await
    }

    /// 覆盖角色的所有 `(资源, 动作)` 权限规则，先删后写；调用方必须与角色实体更新共用事务，删除或写入失败直接返回。
    pub async fn replace_role_permissions(
        &self,
        role_key: &str,
        permissions: &[(String, String)],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let collection = self.collection();
        mongo_ops::delete_many(&collection, Self::role_permission_filter(role_key), &mut *executor).await?;

        let rules = Self::role_permission_rules(role_key, permissions);
        mongo_ops::insert_many(&collection, rules, executor).await
    }

    /// 删除指定角色的 `p` 权限规则及以该角色为目标的 `g` 绑定。两步删除必须与角色实体删除共用事务，任一步失败直接返回。
    pub async fn remove_role(&self, role_key: &str, executor: &mut dyn Executor) -> Result<()> {
        let collection = self.collection();
        mongo_ops::delete_many(&collection, Self::role_permission_filter(role_key), &mut *executor).await?;
        mongo_ops::delete_many(&collection, Self::role_binding_filter(role_key), executor).await?;
        Ok(())
    }

    // Casbin 只负责全量装载，策略写入必须走业务事务。
    fn unsupported_operation(operation: &str) -> casbin::Error {
        Self::adapter_error(io::Error::new(
            io::ErrorKind::Unsupported,
            format!("MongoCasbinAdapter does not support {operation}"),
        ))
    }
}

#[async_trait]
impl Adapter for MongoCasbinAdapter {
    // 全量加载持久化规则到 Casbin 模型，保留查询或游标错误。
    async fn load_policy(&mut self, model: &mut dyn Model) -> casbin::Result<()> {
        let mut cursor = self.collection().find(doc! {}).await.map_err(Self::adapter_error)?;
        while let Some(rule) = cursor.next().await {
            Self::load_rule(model, rule.map_err(Self::adapter_error)?);
        }
        Ok(())
    }

    async fn load_filtered_policy<'a>(
        &mut self,
        _model: &mut dyn Model,
        _filter: Filter<'a>,
    ) -> casbin::Result<()> {
        Err(Self::unsupported_operation("load_filtered_policy"))
    }

    async fn save_policy(&mut self, _model: &mut dyn Model) -> casbin::Result<()> {
        Err(Self::unsupported_operation("save_policy"))
    }

    async fn clear_policy(&mut self) -> casbin::Result<()> {
        Err(Self::unsupported_operation("clear_policy"))
    }

    fn is_filtered(&self) -> bool {
        false
    }

    async fn add_policy(&mut self, _sec: &str, _ptype: &str, _rule: Vec<String>) -> casbin::Result<bool> {
        Err(Self::unsupported_operation("add_policy"))
    }

    async fn add_policies(
        &mut self,
        _sec: &str,
        _ptype: &str,
        _rules: Vec<Vec<String>>,
    ) -> casbin::Result<bool> {
        Err(Self::unsupported_operation("add_policies"))
    }

    async fn remove_policy(&mut self, _sec: &str, _ptype: &str, _rule: Vec<String>) -> casbin::Result<bool> {
        Err(Self::unsupported_operation("remove_policy"))
    }

    async fn remove_policies(
        &mut self,
        _sec: &str,
        _ptype: &str,
        _rules: Vec<Vec<String>>,
    ) -> casbin::Result<bool> {
        Err(Self::unsupported_operation("remove_policies"))
    }

    async fn remove_filtered_policy(
        &mut self,
        _sec: &str,
        _ptype: &str,
        _field_index: usize,
        _field_values: Vec<String>,
    ) -> casbin::Result<bool> {
        Err(Self::unsupported_operation("remove_filtered_policy"))
    }
}

#[cfg(test)]
mod tests {
    use std::io;

    use casbin::{Adapter, DefaultModel, Filter};
    use mongodb::Client;
    use mongodb::bson::doc;
    use persistence_core::Error as PersistenceError;

    use super::{CasbinRule, MongoCasbinAdapter};

    // 验证 Casbin 过滤加载及通用写入全部返回明确的错误，包括空批次。
    #[tokio::test]
    async fn adapter_rejects_filtered_loads_and_generic_writes() {
        let client = Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1").await.unwrap();
        let mut adapter = MongoCasbinAdapter::new(client.database("casbin_test"));
        let mut model = DefaultModel::default();
        let filter = Filter { p: vec![], g: vec![] };

        for (operation, error) in [
            ("load_filtered_policy", adapter.load_filtered_policy(&mut model, filter).await.unwrap_err()),
            ("save_policy", adapter.save_policy(&mut model).await.unwrap_err()),
            ("clear_policy", adapter.clear_policy().await.unwrap_err()),
            ("add_policy", adapter.add_policy("p", "p", vec![]).await.unwrap_err()),
            ("add_policies", adapter.add_policies("p", "p", vec![]).await.unwrap_err()),
            ("remove_policy", adapter.remove_policy("p", "p", vec![]).await.unwrap_err()),
            ("remove_policies", adapter.remove_policies("p", "p", vec![]).await.unwrap_err()),
            (
                "remove_filtered_policy",
                adapter.remove_filtered_policy("p", "p", 0, vec![]).await.unwrap_err(),
            ),
        ] {
            let casbin::Error::AdapterError(error) = error else {
                panic!("{operation} did not return an adapter error: {error}");
            };
            let unsupported = error.0.downcast_ref::<io::Error>().unwrap();
            assert_eq!(unsupported.kind(), io::ErrorKind::Unsupported);
            assert!(unsupported.to_string().contains(operation));
        }
        assert!(!adapter.is_filtered());
    }

    // 验证角色权限稳定按角色、资源、动作顺序生成 `p` 规则。
    #[test]
    fn role_permission_rules_use_role_resource_action_order() {
        let permissions =
            vec![("role".to_string(), "read".to_string()), ("customer".to_string(), "create".to_string())];

        let rules = MongoCasbinAdapter::role_permission_rules("role:manager", &permissions);

        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].sec, "p");
        assert_eq!(rules[0].ptype, "p");
        assert_eq!(rules[0].values, ["role:manager", "role", "read"]);
        assert_eq!(rules[1].values, ["role:manager", "customer", "create"]);
    }

    // 验证角色清理分别匹配权限第一列和绑定第二列。
    #[test]
    fn role_cleanup_filters_cover_permission_and_binding_positions() {
        assert_eq!(
            MongoCasbinAdapter::role_permission_filter("role:manager"),
            doc! {
                "sec": "p",
                "ptype": "p",
                "values.0": "role:manager",
            }
        );
        assert_eq!(
            MongoCasbinAdapter::role_binding_filter("role:manager"),
            doc! {
                "sec": "g",
                "ptype": "g",
                "values.1": "role:manager",
            }
        );
    }

    // 验证主体绑定用 `g` 规则第一列筛选，并从第二列提取排序去重的角色键。
    #[test]
    fn subject_roles_use_group_subject_filter_and_second_value() {
        assert_eq!(
            MongoCasbinAdapter::subject_role_filter("user:admin:account-1"),
            doc! {
                "sec": "g",
                "ptype": "g",
                "values.0": "user:admin:account-1",
            }
        );

        let roles = MongoCasbinAdapter::subject_role_keys(vec![
            CasbinRule::new("g", "g", vec!["user:admin:account-1".to_string(), "role:sales".to_string()]),
            CasbinRule::new("g", "g", vec!["user:admin:account-1".to_string(), "role:finance".to_string()]),
            CasbinRule::new("g", "g", vec!["user:admin:account-1".to_string(), "role:sales".to_string()]),
            CasbinRule::new("g", "g", vec!["user:admin:account-1".to_string()]),
        ]);

        assert_eq!(roles, ["role:finance", "role:sales"]);
    }

    // 验证批量策略按原主体区分父角色和直接权限，并排序去重。
    #[test]
    fn subject_policy_batch_preserves_each_subject_and_rule_kind() {
        let binding = CasbinRule::new("g", "g", vec!["user:admin:actor".into(), "role:sales".into()]);
        let permission =
            CasbinRule::new("p", "p", vec!["role:sales".into(), "customer".into(), "list".into()]);
        let rows = MongoCasbinAdapter::group_subject_policies(vec![
            binding.clone(),
            permission.clone(),
            binding,
            permission,
            CasbinRule::new("g", "g", vec!["role:sales".into(), "role:base".into()]),
        ])
        .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows["user:admin:actor"].roles, ["role:sales"]);
        assert!(rows["user:admin:actor"].permissions.is_empty());
        assert_eq!(rows["role:sales"].roles, ["role:base"]);
        assert_eq!(rows["role:sales"].permissions, [("customer".into(), "list".into())]);
    }

    // 验证权限或绑定行的类型、列数损坏时整批拒绝，而不是解释为无策略。
    #[test]
    fn subject_policy_batch_rejects_malformed_permission_or_binding_rows() {
        for rule in [
            CasbinRule::new("p", "p", vec!["role:sales".into(), "customer".into()]),
            CasbinRule::new("g", "g", vec!["role:sales".into()]),
            CasbinRule::new("p", "g", vec!["role:sales".into(), "role:base".into()]),
        ] {
            assert!(matches!(
                MongoCasbinAdapter::group_subject_policies(vec![rule]),
                Err(PersistenceError::EntityMetadataOutOfRange("Casbin subject policy"))
            ));
        }
    }

    // 验证策略写入共用一个主键，并以原子自增推进版本。
    #[test]
    fn policy_revision_uses_one_monotonic_coordination_document() {
        assert_eq!(MongoCasbinAdapter::policy_revision_filter(), doc! { "_id": "policy" });
        assert_eq!(MongoCasbinAdapter::policy_revision_increment(), doc! { "$inc": { "revision": 1_i64 } });
    }
}
