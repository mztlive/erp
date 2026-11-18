//! 目录授权所需的有界组织事实；历史关系不进入当前目录索引。

use std::collections::HashMap;

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::common::time::Instant;
use mongodb::Database;
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Executor, Result as RepositoryResult, mongo_ops};
use serde::de::DeserializeOwned;

use super::{ORG_MEMBERSHIPS, ORG_UNITS, OrganizationRepository};
use crate::entity::organization::{OrgMembership, OrgUnit};
use crate::entity::organization_change::OrganizationState;
use crate::repository::person_directory_query::DIRECTORY_LIMIT;
use crate::{Error, Result};

impl OrganizationRepository<'_> {
    /// 只读取一个账号在授权时点的有效主属关系。
    /// 返回至多两条未删除关系；两条已足以证明主属关系冲突，不读取其他组织集合。
    ///
    /// 数据库查询或成员事实反序列化失败时返回仓储错误。
    ///
    /// `user_id` 为明确账号；`at` 与调用方授权时点一致；沿用调用方执行器。
    pub async fn primary_memberships(
        &self,
        user_id: &str,
        at: Instant,
        executor: &mut dyn Executor,
    ) -> RepositoryResult<Vec<OrgMembership>> {
        let mut filter = current_filter(at);
        filter.insert("user_id", user_id);
        filter.insert("deleted_at", NOT_DELETED_TIMESTAMP_BSON);
        mongo_ops::find_many(
            &self.db.collection::<OrgMembership>(ORG_MEMBERSHIPS),
            filter,
            FindOptions::builder().limit(2).build(),
            executor,
        )
        .await
    }

    /// 读取账号当前主属关系及最近一条未来生效关系。
    /// 至多两条当前关系及一条最近未来关系，用于完整性和到期边界判断。
    ///
    /// 查询或反序列化失败时返回仓储错误。
    ///
    /// user_id 为明确账号，at 为解释时点，executor 为调用方原执行器。
    pub async fn primary_membership_context(
        &self,
        user_id: &str,
        at: Instant,
        executor: &mut dyn Executor,
    ) -> RepositoryResult<Vec<OrgMembership>> {
        let mut memberships = self.primary_memberships(user_id, at, executor).await?;
        let future = mongo_ops::find_many(
            &self.db.collection::<OrgMembership>(ORG_MEMBERSHIPS),
            doc! {"user_id": user_id, "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            "valid_from": {"$gt": at.unix_secs()}},
            FindOptions::builder().sort(doc! {"valid_from": 1}).limit(1).build(),
            executor,
        )
        .await?;
        memberships.extend(future);
        Ok(memberships)
    }

    // 读取有界组织树，供路径启用检查使用。
    // 当前未删除组织单元，不读取成员或管理关系。
    //
    // 超过组织上限或查询失败时拒绝。
    //
    // executor 为调用方执行器。
    pub(crate) async fn units(&self, executor: &mut dyn Executor) -> Result<Vec<OrgUnit>> {
        bounded::<OrgUnit>(self.db, ORG_UNITS, doc! {}, executor).await
    }

    // 读取目录授权所需的组织树和操作人当前关系。
    // 有界组织树及操作人关系；不预加载其他人员或历史关系。
    //
    // 任一索引超过 10000 项或读取失败时整体拒绝。
    //
    // `actor_id` 为操作人，`at` 为同一授权时点，`executor` 为调用方事务。
    pub(crate) async fn directory_state(
        &self,
        actor_id: &str,
        at: Instant,
        executor: &mut dyn Executor,
    ) -> Result<OrganizationState> {
        Ok(OrganizationState {
            version: self.revision(executor).await?.map_or(0, |row| row.revision),
            units: self.units(executor).await?,
            memberships: self.directory_memberships(Some(&[actor_id.to_owned()]), None, at, executor).await?,
            management: Vec::new(),
        })
    }

    // 按账号或明确组织集合读取当前主属关系。
    // 最多 10000 条当前关系，稳定排序；空身份集合不查库。
    //
    // 无限定、输入或结果超限、数据库失败均拒绝，不返回截断结果。
    //
    // `ids` 与 `orgs` 至少提供一个，均与数据库条件求交；`at` 与授权时点一致。
    pub(crate) async fn directory_memberships(
        &self,
        ids: Option<&[String]>,
        orgs: Option<&[String]>,
        at: Instant,
        executor: &mut dyn Executor,
    ) -> Result<Vec<OrgMembership>> {
        if ids.is_none() && orgs.is_none() {
            return Err(Error::ValidationError("人员组织查询必须限定账号或组织".into()));
        }
        if ids.is_some_and(|v| v.len() > DIRECTORY_LIMIT) || orgs.is_some_and(|v| v.len() > DIRECTORY_LIMIT) {
            return Err(Error::ValidationError("人员组织查询超过 10000 项".into()));
        }
        if ids.is_some_and(|values| values.is_empty()) || orgs.is_some_and(|values| values.is_empty()) {
            return Ok(Vec::new());
        }

        let mut filter = current_filter(at);
        if let Some(ids) = ids {
            filter.insert("user_id", doc! { "$in": ids });
        }
        if let Some(orgs) = orgs {
            filter.insert("org_unit_id", doc! { "$in": orgs });
        }
        bounded(self.db, ORG_MEMBERSHIPS, filter, executor).await
    }
}

// 构造授权时点的成员过滤条件，保持开始包含、结束不包含的半开有效期语义。
fn current_filter(at: Instant) -> Document {
    doc! {
        "valid_from": { "$lte": at.unix_secs() },
        "$or": [{ "valid_to": null }, { "valid_to": { "$gt": at.unix_secs() } }],
    }
}

// 对有界组织查询最多读取上限加一条以发现溢出；超限拒绝整次读取，不将截断的组织事实当成完整结果。
async fn bounded<T: DeserializeOwned + Send + Sync>(
    db: &Database,
    name: &str,
    mut filter: Document,
    executor: &mut dyn Executor,
) -> Result<Vec<T>> {
    filter.insert("deleted_at", NOT_DELETED_TIMESTAMP_BSON);
    let options = FindOptions::builder().sort(doc! { "id": 1 }).limit(10001).build();
    let rows = mongo_ops::find_many(&db.collection::<T>(name), filter, options, executor).await?;

    if rows.len() > DIRECTORY_LIMIT {
        return Err(Error::ValidationError("目录组织索引超过 10000 项，请收窄范围或组织条件".into()));
    }
    Ok(rows)
}

impl OrganizationRepository<'_> {
    /// 批量读取明确账号当前主属组织的可读名称。
    /// 返回账号与有效组织名称；无关系或组织已停用时不补造名称。
    ///
    /// 查询超限、主属关系冲突或数据库错误时拒绝。
    ///
    /// `ids` 为调用方已授权展示的账号，`at` 为统一查询时点，沿用 `executor`。
    pub async fn primary_org_names(
        &self,
        ids: &[String],
        at: Instant,
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        let memberships = self.directory_memberships(Some(ids), None, at, executor).await?;
        let org_ids = memberships.iter().map(|row| row.org_unit_id.clone()).collect::<Vec<_>>();
        if org_ids.is_empty() {
            return Ok(Default::default());
        }

        let units = bounded::<OrgUnit>(
            self.db,
            ORG_UNITS,
            doc! { "id": { "$in": org_ids }, "enabled": true },
            executor,
        )
        .await?;

        primary_names(ids, &memberships, &units, at)
    }
}

// 给账号展示补齐唯一有效主属组织的名称；名称未查到时保留未知，不推测部门。
fn primary_names(
    ids: &[String],
    memberships: &[OrgMembership],
    units: &[OrgUnit],
    at: Instant,
) -> Result<HashMap<String, String>> {
    let mut names = HashMap::new();
    for id in ids {
        if let Some(org) = OrgMembership::primary_org(memberships, id, at)?
            && let Some(unit) =
                units.iter().find(|unit| unit.base.id == org && unit.enabled && !unit.base.is_deleted())
        {
            names.insert(id.clone(), unit.name.clone());
        }
    }
    Ok(names)
}

#[cfg(test)]
mod tests {
    use std::slice::from_ref;

    use entity_core::BaseModel;

    use super::*;
    use crate::entity::organization::{OrgUnitKind, OrgValidity};

    fn fixture() -> (OrgMembership, OrgUnit) {
        let mut unit = OrgUnit {
            base: BaseModel::fake(),
            name: "采购部".into(),
            parent_id: None,
            kind: OrgUnitKind::Department,
            leader: None,
            enabled: true,
            changed_by: "admin".into(),
            reason: "测试".into(),
        };
        unit.base.id = "purchasing".into();
        let membership = OrgMembership {
            base: BaseModel::fake(),
            user_id: "reviewer".into(),
            org_unit_id: unit.base.id.clone(),
            validity: OrgValidity {
                valid_from: Instant::from_unix_secs(10),
                valid_to: Some(Instant::from_unix_secs(20)),
            },
            changed_by: "admin".into(),
            reason: "测试".into(),
        };
        (membership, unit)
    }

    #[test]
    fn names_require_current_membership_and_enabled_unit() {
        let (membership, mut unit) = fixture();
        let ids = vec!["reviewer".into(), "missing".into()];
        let names =
            primary_names(&ids, from_ref(&membership), from_ref(&unit), Instant::from_unix_secs(10)).unwrap();
        assert_eq!(names.get("reviewer").map(String::as_str), Some("采购部"));
        assert!(!names.contains_key("missing"));

        assert!(
            primary_names(&ids, from_ref(&membership), from_ref(&unit), Instant::from_unix_secs(20))
                .unwrap()
                .is_empty()
        );

        unit.enabled = false;
        assert!(primary_names(&ids, &[membership], &[unit], Instant::from_unix_secs(15)).unwrap().is_empty());
    }

    #[test]
    fn conflicting_memberships_do_not_choose_an_arbitrary_department() {
        let (membership, unit) = fixture();
        let duplicate = membership.clone();
        assert!(
            primary_names(
                &["reviewer".into()],
                &[membership, duplicate],
                &[unit],
                Instant::from_unix_secs(15)
            )
            .is_err()
        );
    }
}
