//! 原执行器内的组织目录事实；不解释业务数据授权政策。
use std::collections::BTreeSet;

use erp_core::common::time::Instant;
use mongodb::Database;
use persistence_core::Executor;

use crate::Result;
use crate::entity::organization::{OrgMembership, OrgTree};
use crate::repository::OrganizationRepository;

/// 复用调用方快照读取组织树和成员关系，供业务筛选、责任归属及授权事实获取使用。
#[derive(Clone)]
pub struct OrganizationDirectoryService {
    // 组织事实所在数据库，保存节点和账号成员关系。
    db: Database,
}

impl OrganizationDirectoryService {
    /// 绑定组织数据库以便后续沿调用方执行器取证；构造时只保存句柄，读取时再获取当前组织事实。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 将业务筛选指定的组织展开为排序去重的启用节点，可选包含下级。未知组织、非法树或读取失败时返回错误。
    pub async fn expand_org_units(
        &self,
        ids: &[String],
        include_descendants: bool,
        executor: &mut dyn Executor,
    ) -> Result<BTreeSet<String>> {
        let state = OrganizationRepository::new(&self.db).state(executor).await?;
        OrgTree::new(&state.units)?.expand_all(ids, include_descendants)
    }

    /// 按组织 ID 集合和指定时点查找有效成员，返回排序去重的后台账号 ID，供按组织筛选业务数据。
    pub async fn org_member_ids(
        &self,
        ids: &BTreeSet<String>,
        at: Instant,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        let state = OrganizationRepository::new(&self.db).state(executor).await?;
        Ok(state
            .memberships
            .iter()
            .filter(|member| {
                !member.base.is_deleted() && ids.contains(&member.org_unit_id) && member.validity.contains(at)
            })
            .map(|member| member.user_id.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect())
    }

    /// 读取指定时点的唯一主属组织，供责任归属或目录展示使用。无主属关系时返回 `None`；重叠关系或读取失败时返回错误。
    pub async fn own_org(
        &self,
        user: &str,
        at: Instant,
        executor: &mut dyn Executor,
    ) -> Result<Option<String>> {
        let memberships =
            OrganizationRepository::new(&self.db).primary_memberships(user, at, executor).await?;
        Ok(OrgMembership::primary_org(&memberships, user, at)?.map(str::to_owned))
    }

    /// 返回指定时点的主属组织及下一次关系生效或失效的时间，供调用方限制依赖该关系的授权期限。当前关系重叠时返回错误。
    pub async fn own_org_with_validity(
        &self,
        user: &str,
        at: Instant,
        executor: &mut dyn Executor,
    ) -> Result<(Option<String>, Option<Instant>)> {
        let memberships =
            OrganizationRepository::new(&self.db).primary_membership_context(user, at, executor).await?;
        let organization = OrgMembership::primary_org(&memberships, user, at)?.map(str::to_owned);
        Ok((organization, OrgMembership::next_boundary(&memberships, user, at)))
    }
}
