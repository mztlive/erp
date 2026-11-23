//! 角色权限规则与全局策略修订的 MongoDB 存储和轻量内存快照。
//! 沿用 `casbin_rules` 与 `casbin_policy_state` 集合及原有文档结构，无须数据迁移。
use std::collections::{HashMap, HashSet};

use futures_util::StreamExt;
use mongodb::Database;
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Error as PersistenceError, Executor, ResetRetention, Result, mongo_ops};
use serde::{Deserialize, Serialize};

use crate::entity::rbac::{Permission, PermissionSet};
use crate::entity::role::ROOT_ROLE_ID;

pub const CASBIN_RULES: &str = "casbin_rules";
const CASBIN_POLICY_STATE: &str = "casbin_policy_state";
const POLICY_STATE_ID: &str = "policy";

#[derive(Debug, Deserialize)]
struct PolicyStateDoc {
    revision: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PolicyRule {
    #[serde(rename = "_id")]
    id: String,
    sec: String,
    ptype: String,
    values: Vec<String>,
}

impl PolicyRule {
    fn new(sec: &str, ptype: &str, values: Vec<String>) -> Self {
        Self { id: Self::id(sec, ptype, &values), sec: sec.to_string(), ptype: ptype.to_string(), values }
    }

    fn id(sec: &str, ptype: &str, values: &[String]) -> String {
        format!("{sec}\u{1f}{ptype}\u{1f}{}", values.join("\u{1f}"))
    }
}

/// 内存中冻结的角色权限和分组图，替代重型外部规则引擎，支持直接查询、防环继承展开及通配判定。
#[derive(Debug, Clone, Default)]
pub(crate) struct PolicySnapshot {
    permissions: HashMap<String, Vec<(String, String)>>,
    parents: HashMap<String, Vec<String>>,
    members: HashMap<String, Vec<String>>,
}

impl PolicySnapshot {
    /// 写入一条持久化规则行；未知区段或列数不符的行忽略。
    pub(crate) fn insert_rule(&mut self, sec: &str, ptype: &str, values: Vec<String>) {
        let mut iter = values.into_iter();
        match (sec, ptype, iter.next(), iter.next(), iter.next(), iter.next()) {
            ("p", "p", Some(subject), Some(resource), Some(action), None) => {
                self.add_permission(&subject, resource, action);
            },
            ("g", "g", Some(subject), Some(parent), None, None) => {
                self.add_role_edge(&subject, parent);
            },
            _ => {},
        }
    }

    /// 记录 `subject -> parent` 的直接继承或角色绑定边，并维护反向成员索引。
    pub(crate) fn add_role_edge(&mut self, subject: &str, parent: String) {
        self.members.entry(parent.clone()).or_default().push(subject.to_owned());
        self.parents.entry(subject.to_owned()).or_default().push(parent);
    }

    /// 记录 `subject` 的一条直接 `(resource, action)` 权限规则。
    pub(crate) fn add_permission(&mut self, subject: &str, resource: String, action: String) {
        self.permissions.entry(subject.to_owned()).or_default().push((resource, action));
    }

    /// 读取主体直接绑定的父角色键切片。
    pub(crate) fn direct_roles(&self, subject: &str) -> &[String] {
        self.parents.get(subject).map_or(&[], Vec::as_slice)
    }

    /// 读取直接绑定目标角色的主体键切片。
    pub(crate) fn role_members(&self, role_key: &str) -> &[String] {
        self.members.get(role_key).map_or(&[], Vec::as_slice)
    }

    /// 解析主体的直接权限集，不包含继承权限；任一持久化权限非法时返回错误。
    pub(crate) fn direct_permissions(&self, subject: &str) -> crate::Result<PermissionSet> {
        let Some(pairs) = self.permissions.get(subject) else {
            return Ok(PermissionSet::default());
        };
        parse_permission_pairs(pairs.iter().map(|(r, a)| (r.as_str(), a.as_str())))
    }

    /// 沿角色继承链防环展开主体的全部隐式权限；任一持久化权限非法时返回错误。
    pub(crate) fn implicit_permissions(&self, subject: &str) -> crate::Result<PermissionSet> {
        let mut pairs = Vec::new();
        self.visit_ancestors(subject, |node| {
            if let Some(direct) = self.permissions.get(node) {
                pairs.extend(direct.iter().map(|(r, a)| (r.as_str(), a.as_str())));
            }
            false
        });
        parse_permission_pairs(pairs)
    }

    /// 判断主体（含自身和传递父角色）是否覆盖指定权限；支持 `*` 资源或动作通配。
    pub(crate) fn allows(&self, subject: &str, permission: &Permission) -> bool {
        let mut matched = false;
        self.visit_ancestors(subject, |node| {
            if self.permissions.get(node).is_some_and(|direct| {
                direct.iter().any(|(res, act)| {
                    (res == "*" || res == permission.resource()) && (act == "*" || act == permission.action())
                })
            }) {
                matched = true;
                return true;
            }
            false
        });
        matched
    }

    // 从 `start` 出发防环遍历自身及全部父节点；回调返回 `true` 时提前终止。
    fn visit_ancestors<'a>(&'a self, start: &'a str, mut visit: impl FnMut(&'a str) -> bool) {
        let mut visited = HashSet::new();
        let mut stack = vec![start];
        while let Some(node) = stack.pop() {
            if !visited.insert(node) {
                continue;
            }
            if visit(node) {
                return;
            }
            if let Some(parents) = self.parents.get(node) {
                stack.extend(parents.iter().map(String::as_str));
            }
        }
    }
}

fn parse_permission_pairs<'a>(
    pairs: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> crate::Result<PermissionSet> {
    let mut permissions = Vec::new();
    for (resource, action) in pairs {
        permissions.push(Permission::parse(format!("{resource}:{action}"))?);
    }
    Ok(PermissionSet::new(permissions))
}

/// 读写 MongoDB 策略规则与全局修订号的存储适配器。
#[derive(Clone)]
pub struct PolicyStore {
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

impl PolicyStore {
    /// 绑定规则和策略版本所在的数据库。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 全量读取 `casbin_rules` 集合构建本地策略快照；读取失败时返回持久化错误。
    pub(crate) async fn load_snapshot(&self) -> Result<PolicySnapshot> {
        let mut snapshot = PolicySnapshot::default();
        let mut cursor = self.collection().find(doc! {}).await?;
        while let Some(rule) = cursor.next().await {
            let rule = rule?;
            snapshot.insert_rule(&rule.sec, &rule.ptype, rule.values);
        }
        Ok(snapshot)
    }

    /// 全库重置时在调用方事务内登记 `admin` 的直接 root 绑定、root 的全量权限及版本文档，不保留其他授权。
    pub(crate) async fn retain_root_authorization(
        &self,
        account_id: &str,
        retained: &mut ResetRetention,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let root = format!("role:{ROOT_ROLE_ID}");
        let subject = format!("user:admin:{account_id}");
        for (sec, values) in [("p", vec![root.clone(), "*".into(), "*".into()]), ("g", vec![subject, root])] {
            let rule = PolicyRule::new(sec, sec, values);
            let filter = doc! { "_id": rule.id, "sec": sec, "ptype": sec, "values": rule.values };
            retained.require(&self.db, CASBIN_RULES, filter, executor).await?;
        }
        retained.require(&self.db, CASBIN_POLICY_STATE, Self::policy_revision_filter(), executor).await?;
        Ok(())
    }

    fn collection(&self) -> mongodb::Collection<PolicyRule> {
        self.db.collection(CASBIN_RULES)
    }

    fn policy_state_collection(&self) -> mongodb::Collection<PolicyStateDoc> {
        self.db.collection(CASBIN_POLICY_STATE)
    }

    fn policy_revision_filter() -> Document {
        doc! { "_id": POLICY_STATE_ID }
    }

    fn policy_revision_increment() -> Document {
        doc! { "$inc": { "revision": 1_i64 } }
    }

    fn role_permission_filter(role_key: &str) -> Document {
        doc! { "sec": "p", "ptype": "p", "values.0": role_key }
    }

    fn role_binding_filter(role_key: &str) -> Document {
        doc! { "sec": "g", "ptype": "g", "values.1": role_key }
    }

    fn subject_role_filter(subject: &str) -> Document {
        doc! { "sec": "g", "ptype": "g", "values.0": subject }
    }

    fn subject_role_keys(rules: Vec<PolicyRule>) -> Vec<String> {
        let mut role_keys =
            rules.into_iter().filter_map(|rule| rule.values.into_iter().nth(1)).collect::<Vec<_>>();
        role_keys.sort();
        role_keys.dedup();
        role_keys
    }

    fn role_permission_rules(role_key: &str, permissions: &[(String, String)]) -> Vec<PolicyRule> {
        permissions
            .iter()
            .map(|(resource, action)| {
                PolicyRule::new("p", "p", vec![role_key.to_string(), resource.clone(), action.clone()])
            })
            .collect()
    }

    /// 用调用方执行器读取已提交的单调递增策略修订；历史库没有版本文档时返回 0。
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

    /// 在调用方快照直接读取主体的 `g` 规则，返回排序去重的第二列角色键。
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

    /// 沿原执行器批量读取 `p`、`g` 规则，按主体返回已去重的直接权限和父角色；任一行损坏时拒绝整批。
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

    fn group_subject_policies(rows: Vec<PolicyRule>) -> Result<HashMap<String, SubjectPolicy>> {
        let mut grouped: HashMap<String, SubjectPolicy> = HashMap::new();
        for row in rows {
            let mut values = row.values.into_iter();
            match (
                row.sec.as_str(),
                row.ptype.as_str(),
                values.next(),
                values.next(),
                values.next(),
                values.next(),
            ) {
                ("p", "p", Some(subject), Some(resource), Some(action), None) => {
                    grouped.entry(subject).or_default().permissions.push((resource, action));
                },
                ("g", "g", Some(subject), Some(parent), None, None) => {
                    grouped.entry(subject).or_default().roles.push(parent);
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

    /// 沿原执行器读取直接绑定目标角色的主体，最多读取 1001 条绑定并返回排序去重的主体键。
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

    /// 在原事务内读取角色的直接权限，返回排序去重的资源动作对，不展开继承。
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

    /// 在调用方策略写事务内递增全局修订。
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

    /// 在策略写事务内用比较并交换递增修订，预期版本变化时返回乐观锁冲突。
    pub async fn bump_policy_revision_if_matches(
        &self,
        expected_revision: u64,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let expected_revision = i64::try_from(expected_revision)
            .map_err(|_| PersistenceError::EntityMetadataOutOfRange("casbin policy revision"))?;
        let result = mongo_ops::update_one(
            &self.policy_state_collection(),
            doc! { "_id": POLICY_STATE_ID, "revision": expected_revision },
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

    /// 先清除主体旧绑定，再写入目标角色键。
    pub async fn replace_subject_roles(
        &self,
        subject: &str,
        role_keys: &[String],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let collection = self.collection();
        mongo_ops::delete_many(&collection, Self::subject_role_filter(subject), &mut *executor).await?;
        let rules = role_keys
            .iter()
            .map(|role| PolicyRule::new("g", "g", vec![subject.to_string(), role.clone()]))
            .collect::<Vec<_>>();
        mongo_ops::insert_many(&collection, rules, executor).await
    }

    /// 清除指定主体的全部角色绑定。
    pub async fn clear_subject_roles(&self, subject: &str, executor: &mut dyn Executor) -> Result<()> {
        self.replace_subject_roles(subject, &[], executor).await
    }

    /// 覆盖角色的所有 `(资源, 动作)` 权限规则，先删后写。
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

    /// 删除指定角色的 `p` 权限规则及以该角色为目标的 `g` 绑定。
    pub async fn remove_role(&self, role_key: &str, executor: &mut dyn Executor) -> Result<()> {
        let collection = self.collection();
        mongo_ops::delete_many(&collection, Self::role_permission_filter(role_key), &mut *executor).await?;
        mongo_ops::delete_many(&collection, Self::role_binding_filter(role_key), executor).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use mongodb::bson::doc;
    use persistence_core::Error as PersistenceError;

    use super::{PolicyRule, PolicySnapshot, PolicyStore};
    use crate::Permission;

    #[test]
    fn role_permission_rules_and_cleanup_filters_match_storage_contract() {
        let permissions =
            vec![("role".to_string(), "read".to_string()), ("customer".to_string(), "create".to_string())];
        let rules = PolicyStore::role_permission_rules("role:manager", &permissions);
        assert_eq!(rules.len(), 2);
        assert_eq!((rules[0].sec.as_str(), rules[0].ptype.as_str()), ("p", "p"));
        assert_eq!(rules[0].values, ["role:manager", "role", "read"]);
        assert_eq!(rules[1].values, ["role:manager", "customer", "create"]);

        assert_eq!(
            PolicyStore::role_permission_filter("role:manager"),
            doc! { "sec": "p", "ptype": "p", "values.0": "role:manager" }
        );
        assert_eq!(
            PolicyStore::role_binding_filter("role:manager"),
            doc! { "sec": "g", "ptype": "g", "values.1": "role:manager" }
        );
        assert_eq!(
            PolicyStore::subject_role_filter("user:admin:account-1"),
            doc! { "sec": "g", "ptype": "g", "values.0": "user:admin:account-1" }
        );
        assert_eq!(PolicyStore::policy_revision_filter(), doc! { "_id": "policy" });
        assert_eq!(PolicyStore::policy_revision_increment(), doc! { "$inc": { "revision": 1_i64 } });
    }

    #[test]
    fn subject_roles_and_policy_batches_deduplicate_and_reject_malformed_rows() {
        let roles = PolicyStore::subject_role_keys(vec![
            PolicyRule::new("g", "g", vec!["user:admin:account-1".into(), "role:sales".into()]),
            PolicyRule::new("g", "g", vec!["user:admin:account-1".into(), "role:finance".into()]),
            PolicyRule::new("g", "g", vec!["user:admin:account-1".into(), "role:sales".into()]),
            PolicyRule::new("g", "g", vec!["user:admin:account-1".into()]),
        ]);
        assert_eq!(roles, ["role:finance", "role:sales"]);

        let binding = PolicyRule::new("g", "g", vec!["user:admin:actor".into(), "role:sales".into()]);
        let permission =
            PolicyRule::new("p", "p", vec!["role:sales".into(), "customer".into(), "list".into()]);
        let rows = PolicyStore::group_subject_policies(vec![
            binding.clone(),
            permission.clone(),
            binding,
            permission,
            PolicyRule::new("g", "g", vec!["role:sales".into(), "role:base".into()]),
        ])
        .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows["user:admin:actor"].roles, ["role:sales"]);
        assert!(rows["user:admin:actor"].permissions.is_empty());
        assert_eq!(rows["role:sales"].roles, ["role:base"]);
        assert_eq!(rows["role:sales"].permissions, [("customer".into(), "list".into())]);

        for rule in [
            PolicyRule::new("p", "p", vec!["role:sales".into(), "customer".into()]),
            PolicyRule::new("g", "g", vec!["role:sales".into()]),
            PolicyRule::new(
                "p",
                "p",
                vec!["role:sales".into(), "customer".into(), "list".into(), "extra".into()],
            ),
            PolicyRule::new("g", "g", vec!["role:sales".into(), "role:base".into(), "extra".into()]),
            PolicyRule::new("p", "g", vec!["role:sales".into(), "role:base".into()]),
        ] {
            assert!(matches!(
                PolicyStore::group_subject_policies(vec![rule]),
                Err(PersistenceError::EntityMetadataOutOfRange("Casbin subject policy"))
            ));
        }
    }

    #[test]
    fn snapshot_handles_inheritance_cycles_and_wildcards() {
        let mut snapshot = PolicySnapshot::default();
        snapshot.insert_rule("g", "g", vec!["user:admin:1".into(), "role:a".into()]);
        snapshot.insert_rule("g", "g", vec!["role:a".into(), "role:b".into()]);
        snapshot.insert_rule("g", "g", vec!["role:b".into(), "role:a".into()]);
        snapshot.insert_rule("p", "p", vec!["role:b".into(), "sales_order".into(), "*".into()]);

        assert!(snapshot.allows("user:admin:1", &Permission::parse("sales_order:submit").unwrap()));
        assert!(!snapshot.allows("user:admin:1", &Permission::parse("customer:list").unwrap()));
        assert_eq!(snapshot.role_members("role:a"), ["user:admin:1", "role:b"]);
    }
}
