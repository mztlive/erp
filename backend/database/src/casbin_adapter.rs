use std::{future::Future, pin::Pin};

use async_trait::async_trait;
use casbin::{error::AdapterError, Adapter, Filter, Model};
use futures_util::StreamExt;
use mongodb::{
    bson::{doc, to_document, Document},
    ClientSession, Database,
};
use serde::{Deserialize, Serialize};

use crate::{Result, Transactional};

pub(crate) const CASBIN_RULES: &str = "casbin_rules";
const CASBIN_POLICY_STATE: &str = "casbin_policy_state";
const POLICY_STATE_ID: &str = "policy";

/// MongoDB 中的 Casbin policy 全局版本。
#[derive(Debug, Deserialize)]
struct CasbinPolicyState {
    revision: i64,
}

/// MongoDB 中的 Casbin policy 记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CasbinRule {
    #[serde(rename = "_id")]
    id: String,
    sec: String,
    ptype: String,
    values: Vec<String>,
}

impl CasbinRule {
    fn new(sec: &str, ptype: &str, values: Vec<String>) -> Self {
        Self {
            id: Self::id(sec, ptype, &values),
            sec: sec.to_string(),
            ptype: ptype.to_string(),
            values,
        }
    }

    fn id(sec: &str, ptype: &str, values: &[String]) -> String {
        format!("{sec}\u{1f}{ptype}\u{1f}{}", values.join("\u{1f}"))
    }
}

/// 将 MongoDB 持久化接入 Casbin 的 Adapter。
#[derive(Clone)]
pub struct MongoCasbinAdapter {
    db: Database,
    filtered: bool,
}

impl MongoCasbinAdapter {
    /// 创建 MongoDB Casbin Adapter。
    ///
    /// # 返回值
    /// 返回绑定指定数据库的 Adapter。
    pub fn new(db: Database) -> Self {
        Self { db, filtered: false }
    }

    fn collection(&self) -> mongodb::Collection<CasbinRule> {
        self.db.collection(CASBIN_RULES)
    }

    fn policy_state_collection(&self) -> mongodb::Collection<CasbinPolicyState> {
        self.db.collection(CASBIN_POLICY_STATE)
    }

    fn policy_revision_filter() -> Document {
        doc! { "_id": POLICY_STATE_ID }
    }

    fn policy_revision_increment() -> Document {
        doc! { "$inc": { "revision": 1_i64 } }
    }

    fn adapter_error(error: impl std::error::Error + Send + Sync + 'static) -> casbin::Error {
        AdapterError(Box::new(error)).into()
    }

    fn load_rule(model: &mut dyn Model, rule: CasbinRule) {
        let Some(section) = model.get_mut_model().get_mut(&rule.sec) else {
            return;
        };
        let Some(assertion) = section.get_mut(&rule.ptype) else {
            return;
        };
        assertion.get_mut_policy().insert(rule.values);
    }

    fn matches_filter(rule: &CasbinRule, filter: &Filter<'_>) -> bool {
        let values = if rule.sec == "p" {
            &filter.p
        } else if rule.sec == "g" {
            &filter.g
        } else {
            return false;
        };

        values.iter().enumerate().all(|(index, expected)| {
            expected.is_empty() || rule.values.get(index).is_some_and(|actual| actual == expected)
        })
    }

    fn filtered_query(sec: &str, ptype: &str, field_index: usize, field_values: &[String]) -> Document {
        let mut filter = doc! {
            "sec": sec,
            "ptype": ptype,
        };
        for (offset, value) in field_values.iter().enumerate() {
            if !value.is_empty() {
                filter.insert(format!("values.{}", field_index + offset), value);
            }
        }
        filter
    }

    fn role_permission_filter(role_key: &str) -> Document {
        doc! {
            "sec": "p",
            "ptype": "p",
            "values.0": role_key,
        }
    }

    fn role_binding_filter(role_key: &str) -> Document {
        doc! {
            "sec": "g",
            "ptype": "g",
            "values.1": role_key,
        }
    }

    fn role_permission_rules(role_key: &str, permissions: &[(String, String)]) -> Vec<CasbinRule> {
        permissions
            .iter()
            .map(|(resource, action)| {
                CasbinRule::new(
                    "p",
                    "p",
                    vec![role_key.to_string(), resource.clone(), action.clone()],
                )
            })
            .collect()
    }

    async fn replace_policy(&self, rules: Vec<CasbinRule>) -> casbin::Result<()> {
        let adapter = self.clone();
        self.run_policy_write(move |session| {
            Box::pin(async move {
                adapter.replace_policy_with_session(rules, session).await?;
                Ok(((), true))
            })
        })
        .await
        .map_err(Self::adapter_error)
    }

    /// 在 Adapter 自有事务中写 policy，并在确有变更时递增全局 revision。
    async fn run_policy_write<T, F>(&self, operation: F) -> Result<T>
    where
        T: Send + 'static,
        F: for<'a> FnOnce(
                &'a mut ClientSession,
            ) -> Pin<Box<dyn Future<Output = Result<(T, bool)>> + Send + 'a>>
            + Send
            + 'static,
    {
        let adapter = self.clone();
        self.db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    let (value, changed) = operation(session).await?;
                    if changed {
                        adapter.bump_policy_revision_with_session(session).await?;
                    }
                    Ok(value)
                })
            })
            .await
    }

    /// 读取数据库中已提交的 Casbin policy 全局版本。
    ///
    /// 缺少版本文档表示尚未发生由新版本服务提交的 policy 变更，返回 `0`。
    ///
    /// # 返回值
    /// 返回单调递增的已提交 policy 版本。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败或持久化版本非法时返回错误。
    pub async fn policy_revision(&self) -> Result<u64> {
        let state = self
            .policy_state_collection()
            .find_one(Self::policy_revision_filter())
            .await?;
        let Some(state) = state else {
            return Ok(0);
        };
        u64::try_from(state.revision)
            .map_err(|_| crate::Error::EntityMetadataOutOfRange("casbin policy revision"))
    }

    /// 在调用方事务内递增 Casbin policy 全局版本。
    ///
    /// 所有 policy 事务写入同一个版本文档，使不同实例上的并发写产生 MongoDB
    /// 事务写冲突，而不是基于彼此不可见的快照同时提交。
    ///
    /// # 参数
    /// * `session` - 已启动事务的 MongoDB 会话
    ///
    /// # 错误
    /// 当版本文档无法更新时返回数据库错误。
    pub async fn bump_policy_revision_with_session(&self, session: &mut ClientSession) -> Result<()> {
        self.policy_state_collection()
            .update_one(Self::policy_revision_filter(), Self::policy_revision_increment())
            .upsert(true)
            .session(session)
            .await?;
        Ok(())
    }

    /// 在调用方事务内以比较并交换方式递增 policy 版本。
    ///
    /// 该方法用于把事务前完成的授权快照与最终 policy 写入绑定；版本已变化时返回
    /// 乐观锁冲突并回滚整个业务事务。
    ///
    /// # 参数
    /// * `expected_revision` - 授权检查使用的 policy 版本
    /// * `session` - 已启动事务的 MongoDB 会话
    ///
    /// # 错误
    /// 当版本超出 BSON 范围、版本已变化或 MongoDB 更新失败时返回错误。
    pub async fn bump_policy_revision_if_matches_with_session(
        &self,
        expected_revision: u64,
        session: &mut ClientSession,
    ) -> Result<()> {
        let expected_revision = i64::try_from(expected_revision)
            .map_err(|_| crate::Error::EntityMetadataOutOfRange("casbin policy revision"))?;
        let result = self
            .policy_state_collection()
            .update_one(
                doc! {
                    "_id": POLICY_STATE_ID,
                    "revision": expected_revision,
                },
                Self::policy_revision_increment(),
            )
            .upsert(expected_revision == 0)
            .session(session)
            .await?;
        if result.matched_count == 0 && result.upserted_id.is_none() {
            return Err(crate::Error::OptimisticLockingError);
        }
        Ok(())
    }

    /// 使用调用方 MongoDB 会话覆盖指定主体的角色绑定。
    ///
    /// 该方法不会提交事务，调用方可以把账号、Profile 与角色绑定放入同一原子边界。
    ///
    /// # 参数
    /// * `subject` - Casbin 主体标识
    /// * `role_keys` - 目标 Casbin 角色键
    /// * `session` - 已启动事务的 MongoDB 会话
    ///
    /// # 错误
    /// 当规则删除或写入失败时返回数据库错误。
    pub async fn replace_subject_roles_with_session(
        &self,
        subject: &str,
        role_keys: &[String],
        session: &mut ClientSession,
    ) -> Result<()> {
        let collection = self.collection();
        collection
            .delete_many(doc! {
                "sec": "g",
                "ptype": "g",
                "values.0": subject,
            })
            .session(&mut *session)
            .await?;

        if role_keys.is_empty() {
            return Ok(());
        }

        let rules = role_keys
            .iter()
            .map(|role| CasbinRule::new("g", "g", vec![subject.to_string(), role.clone()]))
            .collect::<Vec<_>>();
        collection.insert_many(rules).session(&mut *session).await?;
        Ok(())
    }

    /// 使用调用方 MongoDB 会话清除指定主体的全部角色绑定。
    ///
    /// # 参数
    /// * `subject` - Casbin 主体标识
    /// * `session` - 已启动事务的 MongoDB 会话
    ///
    /// # 错误
    /// 当规则删除失败时返回数据库错误。
    pub async fn clear_subject_roles_with_session(
        &self,
        subject: &str,
        session: &mut ClientSession,
    ) -> Result<()> {
        self.replace_subject_roles_with_session(subject, &[], session)
            .await
    }

    /// 使用调用方 MongoDB 会话覆盖指定角色的全部权限规则。
    ///
    /// 该方法不会提交事务。调用方应把角色实体更新与本方法放入同一个事务。
    ///
    /// # 参数
    /// * `role_key` - Casbin 角色键
    /// * `permissions` - 目标权限的 `(resource, action)` 集合
    /// * `session` - 已启动事务的 MongoDB 会话
    ///
    /// # 返回值
    /// 权限规则全部替换成功时返回 `Ok(())`。
    ///
    /// # 错误
    /// 当规则删除或写入失败时返回数据库错误。
    pub async fn replace_role_permissions_with_session(
        &self,
        role_key: &str,
        permissions: &[(String, String)],
        session: &mut ClientSession,
    ) -> Result<()> {
        let collection = self.collection();
        collection
            .delete_many(Self::role_permission_filter(role_key))
            .session(&mut *session)
            .await?;

        let rules = Self::role_permission_rules(role_key, permissions);
        if !rules.is_empty() {
            collection.insert_many(rules).session(&mut *session).await?;
        }
        Ok(())
    }

    /// 使用调用方 MongoDB 会话删除指定角色的全部 Casbin 规则。
    ///
    /// 该方法同时删除角色的 `p` 权限规则，以及以该角色为目标的 `g` 绑定规则，
    /// 但不会提交事务。调用方应把角色实体删除与本方法放入同一个事务。
    ///
    /// # 参数
    /// * `role_key` - Casbin 角色键
    /// * `session` - 已启动事务的 MongoDB 会话
    ///
    /// # 返回值
    /// 相关规则全部删除成功时返回 `Ok(())`。
    ///
    /// # 错误
    /// 当任一规则删除失败时返回数据库错误。
    pub async fn remove_role_with_session(&self, role_key: &str, session: &mut ClientSession) -> Result<()> {
        let collection = self.collection();
        collection
            .delete_many(Self::role_permission_filter(role_key))
            .session(&mut *session)
            .await?;
        collection
            .delete_many(Self::role_binding_filter(role_key))
            .session(&mut *session)
            .await?;
        Ok(())
    }

    /// 使用调用方 MongoDB 会话覆盖全部 Casbin policy。
    async fn replace_policy_with_session(
        &self,
        rules: Vec<CasbinRule>,
        session: &mut ClientSession,
    ) -> Result<()> {
        self.collection()
            .delete_many(doc! {})
            .session(&mut *session)
            .await?;
        if !rules.is_empty() {
            self.collection()
                .insert_many(rules)
                .session(&mut *session)
                .await?;
        }
        Ok(())
    }
}

#[async_trait]
impl Adapter for MongoCasbinAdapter {
    async fn load_policy(&mut self, model: &mut dyn Model) -> casbin::Result<()> {
        self.filtered = false;
        let mut cursor = self
            .collection()
            .find(doc! {})
            .await
            .map_err(Self::adapter_error)?;
        while let Some(rule) = cursor.next().await {
            Self::load_rule(model, rule.map_err(Self::adapter_error)?);
        }
        Ok(())
    }

    async fn load_filtered_policy<'a>(
        &mut self,
        model: &mut dyn Model,
        filter: Filter<'a>,
    ) -> casbin::Result<()> {
        self.filtered = true;
        let mut cursor = self
            .collection()
            .find(doc! {})
            .await
            .map_err(Self::adapter_error)?;
        while let Some(rule) = cursor.next().await {
            let rule = rule.map_err(Self::adapter_error)?;
            if Self::matches_filter(&rule, &filter) {
                Self::load_rule(model, rule);
            }
        }
        Ok(())
    }

    async fn save_policy(&mut self, model: &mut dyn Model) -> casbin::Result<()> {
        let mut rules = Vec::new();
        for sec in ["p", "g"] {
            let Some(assertions) = model.get_model().get(sec) else {
                continue;
            };
            for (ptype, assertion) in assertions {
                rules.extend(
                    assertion
                        .get_policy()
                        .iter()
                        .cloned()
                        .map(|values| CasbinRule::new(sec, ptype, values)),
                );
            }
        }
        self.replace_policy(rules).await
    }

    async fn clear_policy(&mut self) -> casbin::Result<()> {
        let adapter = self.clone();
        self.run_policy_write(move |session| {
            Box::pin(async move {
                let result = adapter.collection().delete_many(doc! {}).session(session).await?;
                Ok(((), result.deleted_count > 0))
            })
        })
        .await
        .map_err(Self::adapter_error)?;
        self.filtered = false;
        Ok(())
    }

    fn is_filtered(&self) -> bool {
        self.filtered
    }

    async fn add_policy(&mut self, sec: &str, ptype: &str, rule: Vec<String>) -> casbin::Result<bool> {
        let rule = CasbinRule::new(sec, ptype, rule);
        let rule_doc = to_document(&rule).map_err(Self::adapter_error)?;
        let adapter = self.clone();
        self.run_policy_write(move |session| {
            Box::pin(async move {
                let result = adapter
                    .collection()
                    .update_one(doc! { "_id": &rule.id }, doc! { "$setOnInsert": rule_doc })
                    .upsert(true)
                    .session(session)
                    .await?;
                let changed = result.upserted_id.is_some();
                Ok((changed, changed))
            })
        })
        .await
        .map_err(Self::adapter_error)
    }

    async fn add_policies(
        &mut self,
        sec: &str,
        ptype: &str,
        rules: Vec<Vec<String>>,
    ) -> casbin::Result<bool> {
        let rules = rules
            .into_iter()
            .map(|rule| CasbinRule::new(sec, ptype, rule))
            .collect::<Vec<_>>();
        if rules.is_empty() {
            return Ok(false);
        }

        let ids = rules.iter().map(|rule| rule.id.clone()).collect::<Vec<_>>();
        let adapter = self.clone();
        self.run_policy_write(move |session| {
            Box::pin(async move {
                let existing = adapter
                    .collection()
                    .count_documents(doc! { "_id": { "$in": ids } })
                    .session(&mut *session)
                    .await?;
                if existing > 0 {
                    return Ok((false, false));
                }
                adapter.collection().insert_many(rules).session(session).await?;
                Ok((true, true))
            })
        })
        .await
        .map_err(Self::adapter_error)
    }

    async fn remove_policy(&mut self, sec: &str, ptype: &str, rule: Vec<String>) -> casbin::Result<bool> {
        let id = CasbinRule::id(sec, ptype, &rule);
        let adapter = self.clone();
        self.run_policy_write(move |session| {
            Box::pin(async move {
                let result = adapter
                    .collection()
                    .delete_one(doc! { "_id": id })
                    .session(session)
                    .await?;
                let changed = result.deleted_count > 0;
                Ok((changed, changed))
            })
        })
        .await
        .map_err(Self::adapter_error)
    }

    async fn remove_policies(
        &mut self,
        sec: &str,
        ptype: &str,
        rules: Vec<Vec<String>>,
    ) -> casbin::Result<bool> {
        if rules.is_empty() {
            return Ok(false);
        }
        let ids = rules
            .iter()
            .map(|rule| CasbinRule::id(sec, ptype, rule))
            .collect::<Vec<_>>();
        let expected_count = rules.len() as u64;
        let adapter = self.clone();
        self.run_policy_write(move |session| {
            Box::pin(async move {
                let result = adapter
                    .collection()
                    .delete_many(doc! { "_id": { "$in": ids } })
                    .session(session)
                    .await?;
                Ok((result.deleted_count == expected_count, result.deleted_count > 0))
            })
        })
        .await
        .map_err(Self::adapter_error)
    }

    async fn remove_filtered_policy(
        &mut self,
        sec: &str,
        ptype: &str,
        field_index: usize,
        field_values: Vec<String>,
    ) -> casbin::Result<bool> {
        if field_values.is_empty() {
            return Ok(false);
        }
        let filter = Self::filtered_query(sec, ptype, field_index, &field_values);
        let adapter = self.clone();
        self.run_policy_write(move |session| {
            Box::pin(async move {
                let result = adapter.collection().delete_many(filter).session(session).await?;
                let changed = result.deleted_count > 0;
                Ok((changed, changed))
            })
        })
        .await
        .map_err(Self::adapter_error)
    }
}

#[cfg(test)]
mod tests {
    use mongodb::bson::doc;

    use super::MongoCasbinAdapter;

    #[test]
    fn role_permission_rules_use_role_resource_action_order() {
        let permissions = vec![
            ("role".to_string(), "read".to_string()),
            ("consumer".to_string(), "create".to_string()),
        ];

        let rules = MongoCasbinAdapter::role_permission_rules("role:manager", &permissions);

        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].sec, "p");
        assert_eq!(rules[0].ptype, "p");
        assert_eq!(rules[0].values, ["role:manager", "role", "read"]);
        assert_eq!(rules[1].values, ["role:manager", "consumer", "create"]);
    }

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

    #[test]
    fn policy_revision_uses_one_monotonic_coordination_document() {
        assert_eq!(
            MongoCasbinAdapter::policy_revision_filter(),
            doc! { "_id": "policy" }
        );
        assert_eq!(
            MongoCasbinAdapter::policy_revision_increment(),
            doc! { "$inc": { "revision": 1_i64 } }
        );
    }
}
