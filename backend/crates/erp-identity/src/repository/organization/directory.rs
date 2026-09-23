//! 目录授权所需的有界组织事实；历史关系不进入当前目录索引。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::common::time::Instant;
use mongodb::Database;
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Executor, mongo_ops};
use serde::de::DeserializeOwned;

use super::{ORG_MANAGEMENT, ORG_MEMBERSHIPS, ORG_UNITS, OrganizationRepository};
use crate::entity::organization::{OrgManagementAssignment, OrgMembership, OrgUnit};
use crate::entity::organization_change::OrganizationState;
use crate::repository::person_directory_query::DIRECTORY_LIMIT;
use crate::{Error, Result};

impl OrganizationRepository<'_> {
    /// 读取目录授权所需的组织树和操作人当前关系。
    /// # 参数
    /// `actor_id` 为操作人，`at` 为同一授权时点，`executor` 为调用方事务。
    /// # 返回
    /// 有界组织树及操作人关系；不预加载其他人员或历史关系。
    /// # 错误
    /// 任一索引超过 10000 项或读取失败时整体拒绝。
    pub(crate) async fn directory_state(
        &self,
        actor_id: &str,
        at: Instant,
        executor: &mut dyn Executor,
    ) -> Result<OrganizationState> {
        let mut management_filter = current_filter(at);
        management_filter.insert("user_id", actor_id);
        Ok(OrganizationState {
            version: self.revision(executor).await?.map_or(0, |row| row.revision),
            units: bounded::<OrgUnit>(self.db, ORG_UNITS, doc! {}, executor).await?,
            memberships: self
                .directory_memberships(Some(&[actor_id.to_owned()]), None, at, executor)
                .await?,
            management: bounded::<OrgManagementAssignment>(
                self.db,
                ORG_MANAGEMENT,
                management_filter,
                executor,
            )
            .await?,
        })
    }

    /// 按账号或明确组织集合读取当前主属关系。
    /// # 参数
    /// `ids` 与 `orgs` 至少提供一个，均与数据库条件求交；`at` 与授权时点一致。
    /// # 返回
    /// 最多 10000 条当前关系，稳定排序；空身份集合不查库。
    /// # 错误
    /// 无限定、输入或结果超限、数据库失败均拒绝，不返回截断结果。
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
        if ids.is_some_and(|v| v.len() > DIRECTORY_LIMIT)
            || orgs.is_some_and(|v| v.len() > DIRECTORY_LIMIT)
        {
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

/// 有效期条件与 OrgValidity 的半开区间保持一致。
fn current_filter(at: Instant) -> Document {
    doc! {
        "valid_from": { "$lte": at.unix_secs() },
        "$or": [{ "valid_to": null }, { "valid_to": { "$gt": at.unix_secs() } }],
    }
}

/// 数据库最多读取上限加一，超限整体拒绝；不先装载全集再裁剪。
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
