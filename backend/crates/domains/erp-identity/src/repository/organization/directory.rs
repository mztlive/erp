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
    /// 在原执行器读取指定账号于授权时点的未删除主属关系，最多取两条以识别冲突，不查其他组织集合；数据库或反序列化错误直接返回。
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

    /// 使用同一执行器读取至多两条当前主属关系及最早生效的一条未来关系，供判断关系冲突和授权到期边界；查询或反序列化失败直接返回。
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

    /// 用调用方执行器读取有界的未删除组织单元，供组织路径启用检查；不读成员或管理关系，超限或查询失败时拒绝。
    pub(crate) async fn units(&self, executor: &mut dyn Executor) -> Result<Vec<OrgUnit>> {
        bounded::<OrgUnit>(self.db, ORG_UNITS, doc! {}, executor).await
    }

    /// 在目录用例事务内读取组织版本、有界组织树和操作人的当前主属关系，不预加载其他人员或历史关系；任一索引超过 10000 项或读取失败时整体拒绝。
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
        })
    }

    /// 在同一授权时点读取至多 10000 条当前主属关系，按 ID 排序。账号与组织至少指定一项，同时指定时取交集；空输入不查库，无限制、超限或数据库失败均不返回部分结果。
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

// 生成成员在授权时点的有效期条件，起点含、终点不含，空终点表示未设到期日。
fn current_filter(at: Instant) -> Document {
    doc! {
        "valid_from": { "$lte": at.unix_secs() },
        "$or": [{ "valid_to": null }, { "valid_to": { "$gt": at.unix_secs() } }],
    }
}

// 按 ID 排序读取未删除的组织事实，多查一条检测溢出，超限时拒绝而不截断。
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
    /// 沿用调用方执行器及统一时点，为已获展示授权的账号批量查当前主属组织名称。无关系或组织停用时省略名称；超限、主属冲突或数据库错误时拒绝。
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

// 只为主属关系唯一且关联组织启用、未删除的账号补齐组织名，不推测缺失名称。
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

    // 构造已启用的采购部门及一条具有明确起止时点的成员关系。
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

    // 验证仅当前有效成员且部门启用时才返回名称，区间终点和未知账号均不补名。
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

    // 验证当前主属关系重复时整次报错，不任择一个部门展示。
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
