//! 内部组织持久化。Service 提供事务，仓储不自行开启事务。

mod authorization;
mod directory;
pub use authorization::primary_membership_query;
use entity_core::{BaseModel, HasBaseModel};
use mongodb::Database;
use persistence_core::change_feed::ChangeSource;
use persistence_core::{Executor, Repository, Result};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::entity::organization::*;
use crate::entity::organization_change::*;

pub const ORG_UNITS: &str = "org_units";
pub const ORG_MEMBERSHIPS: &str = "org_memberships";
pub const ORG_REVISIONS: &str = "org_revisions";
pub const ORG_CHANGES: &str = "org_changes";

/// 身份域组织仓储，读取组织事实并在用例事务内保存原子变更。
pub struct OrganizationRepository<'a> {
    // 组织单元、成员、版本及回执所在的身份数据库。
    db: &'a Database,
}

impl<'a> OrganizationRepository<'a> {
    /// 绑定身份数据库，供组织管理和授权事实读取使用。
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// 在调用方同一事务读取组织版本、单元和成员；没有版本文档时以 0 开始。任何集合读取失败都不返回部分状态。
    pub async fn state(&self, executor: &mut dyn Executor) -> Result<OrganizationState> {
        Ok(OrganizationState {
            version: self.revision(executor).await?.map(|value| value.revision).unwrap_or(0),
            units: Repository::<OrgUnit>::new(self.db, ORG_UNITS).list_all(executor).await?,
            memberships: Repository::<OrgMembership>::new(self.db, ORG_MEMBERSHIPS)
                .list_all(executor)
                .await?,
        })
    }

    /// 读取 `organization` 修订文档，供跨页复验和缓存失效使用；未初始化返回 `None`，读取失败返回仓储错误。
    pub async fn revision(&self, executor: &mut dyn Executor) -> Result<Option<OrganizationRevision>> {
        Repository::new(self.db, ORG_REVISIONS).find_by_id("organization", executor).await
    }

    /// 按幂等命令 ID 查询回执；缺失返回 `None`，读取失败返回仓储错误。仅组织管理用例能对外返回审计详情。
    pub async fn receipt(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<OrganizationChangeReceipt>> {
        Repository::new(self.db, ORG_CHANGES).find_by_id(id, executor).await
    }

    /// 在同一业务事务中推进组织版本，保存变化的单元和成员，并创建含前后状态的回执；不写管理范围集合。版本不符、回执键重复或写入失败时返回错误，由调用方回滚。
    pub async fn save(
        &self,
        receipt: &mut OrganizationChangeReceipt,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.advance(receipt.before.version, receipt.after.version, executor).await?;

        save_changed(self.db, ORG_UNITS, &receipt.before.units, &mut receipt.after.units, executor).await?;
        save_changed(
            self.db,
            ORG_MEMBERSHIPS,
            &receipt.before.memberships,
            &mut receipt.after.memberships,
            executor,
        )
        .await?;

        Repository::new(self.db, ORG_CHANGES).create(receipt, executor).await
    }

    // 先比较并推进统一修订，使并发组织移动或成员有效期变更无法绕过跨行校验。
    async fn advance(&self, expected: u64, next: u64, executor: &mut dyn Executor) -> Result<()> {
        let repository = Repository::<OrganizationRevision>::new(self.db, ORG_REVISIONS);
        let current = self.revision(executor).await?;

        if let Some(mut current) = current {
            if current.revision != expected {
                return Err(persistence_core::Error::OptimisticLockingError);
            }
            current.revision = next;
            return repository.update(&mut current, executor).await;
        }

        if expected != 0 {
            return Err(persistence_core::Error::OptimisticLockingError);
        }
        repository
            .create(
                &OrganizationRevision { base: BaseModel::new("organization".into()), revision: next },
                executor,
            )
            .await
    }
}

// 对比组织实体前后状态，只插入新增行或用乐观锁更新变更行，保留历史记录。
async fn save_changed<T>(
    db: &Database,
    collection: &str,
    before: &[T],
    after: &mut [T],
    executor: &mut dyn Executor,
) -> Result<()>
where
    T: Serialize + DeserializeOwned + Send + Sync + HasBaseModel + PartialEq,
{
    let repository = Repository::<T>::new(db, collection);
    for value in after {
        let old = before.iter().find(|old| old.base().id == value.base().id);
        if old == Some(value) {
            continue;
        }
        if old.is_some() {
            repository.update(value, executor).await?;
        } else {
            repository.create(value, executor).await?;
        }
    }
    Ok(())
}

/// 为授权刷新消费者声明组织版本变更源，只暴露已提交的事务事件；构造不访问数据库。
pub fn changes<T>(topic: T) -> ChangeSource<T> {
    ChangeSource::shared(ORG_REVISIONS, topic)
}
