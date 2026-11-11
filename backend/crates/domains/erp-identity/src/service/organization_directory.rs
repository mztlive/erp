//! 原执行器内的组织目录事实；不解释业务数据授权政策。
use std::collections::BTreeSet;

use erp_core::common::time::Instant;
use mongodb::Database;
use persistence_core::Executor;

use crate::Result;
use crate::entity::organization::{OrgMembership, OrgTree};
use crate::repository::OrganizationRepository;

/// 业务筛选与责任归属共用的组织目录。
#[derive(Clone)]
pub struct OrganizationDirectoryService {
    db: Database,
}

impl OrganizationDirectoryService {
    /// 绑定组织事实数据库。
    /// # 参数
    /// db 为身份数据库。
    /// # 返回
    /// 不缓存组织快照的目录服务。
    /// # 错误
    /// 不执行 I/O。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 展开启用组织及可选下级。
    /// # 参数
    /// ids 为筛选组织，include_descendants 控制下级展开，executor 为原执行器。
    /// # 返回
    /// 排序去重的有效组织集合。
    /// # 错误
    /// 未知组织、非法组织树或读取失败时返回错误。
    pub async fn expand_org_units(
        &self,
        ids: &[String],
        include_descendants: bool,
        executor: &mut dyn Executor,
    ) -> Result<BTreeSet<String>> {
        let state = OrganizationRepository::new(&self.db).state(executor).await?;
        let tree = OrgTree::new(&state.units)?;
        let mut result = BTreeSet::new();
        for id in ids {
            result.extend(tree.expand(id, include_descendants)?);
        }
        Ok(result)
    }

    /// 读取指定组织在给定时点的有效成员。
    /// # 参数
    /// ids 为组织集合，at 为解释时点，executor 为原执行器。
    /// # 返回
    /// 排序去重的有效成员账号 ID。
    /// # 错误
    /// 组织事实读取失败时返回错误。
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

    /// 查询账号在给定时点的唯一主属组织。
    /// # 参数
    /// user 为真实账号，at 为解释时点，executor 为原执行器。
    /// # 返回
    /// 主属组织 ID；没有主属组织时为 None。
    /// # 错误
    /// 读取失败或存在重叠主属关系时返回错误。
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

    /// 查询账号当前唯一主属组织及下一次关系变化边界。
    /// # 参数
    /// user 为明确账号，at 为授权解释时点，executor 为原执行器。
    /// # 返回
    /// 当前组织及最近未来生效或失效时点；无关系时组织为 None。
    /// # 错误
    /// 查询失败或当前存在重叠主属关系时返回错误。
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
