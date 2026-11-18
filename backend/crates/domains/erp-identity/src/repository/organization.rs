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
pub const ORG_MANAGEMENT: &str = "org_management_assignments";
pub const ORG_REVISIONS: &str = "org_revisions";
pub const ORG_CHANGES: &str = "org_changes";

/// 身份域拥有的组织仓储，提供批量事实及原子变更所需操作。
pub struct OrganizationRepository<'a> {
    db: &'a Database,
}

impl<'a> OrganizationRepository<'a> {
    /// 绑定数据库。
    /// 返回仅拥有组织集合的仓储实例。
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// 在调用方同一事务中读取版本与完整组织事实。
    /// 返回组织单元、成员和版本；旧管理关系仅从历史回执读取；尚无版本文档时版本为 `0`。
    ///
    /// 任一集合读取失败时返回仓储错误，不返回残缺状态。
    ///
    /// `executor`：调用方事务或非事务执行器。
    pub async fn state(&self, executor: &mut dyn Executor) -> Result<OrganizationState> {
        Ok(OrganizationState {
            version: self.revision(executor).await?.map(|value| value.revision).unwrap_or(0),
            units: Repository::<OrgUnit>::new(self.db, ORG_UNITS).list_all(executor).await?,
            memberships: Repository::<OrgMembership>::new(self.db, ORG_MEMBERSHIPS)
                .list_all(executor)
                .await?,
            management: Vec::new(),
        })
    }

    /// 读取当前组织版本，用于跨页重验与缓存失效。
    /// 返回标识为 `organization` 的版本文档；尚无文档时返回 `None`。
    ///
    /// 底层读取失败时返回仓储错误。
    pub async fn revision(&self, executor: &mut dyn Executor) -> Result<Option<OrganizationRevision>> {
        Repository::new(self.db, ORG_REVISIONS).find_by_id("organization", executor).await
    }

    /// 查询幂等命令回执；仅组织管理用例可返回审计详情。
    /// 返回匹配回执；不存在时返回 `None`。
    ///
    /// 底层读取失败时返回仓储错误。
    pub async fn receipt(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<OrganizationChangeReceipt>> {
        Repository::new(self.db, ORG_CHANGES).find_by_id(id, executor).await
    }

    /// 推进全局版本，写入有变化的组织单元和成员，并创建含前后状态的变更回执。
    ///
    /// 不写入管理范围集合。必须传入同一业务事务执行器。
    ///
    /// 版本、变化的组织单元与成员以及回执都写入成功时无额外返回值。
    ///
    /// 版本与预期不符、回执键重复或任一写入失败时返回仓储错误，由调用方回滚。
    ///
    /// `receipt`：含预期版本和前后组织状态的变更回执；`executor`：调用方的同一事务执行器。
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

    // 组织写事务先竞争推进统一修订文档，以便并发移动及成员有效期变更不能绕过跨行校验。
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

// 持久化组织状态差异时只插入新增实体或更新变动行，沿用实体乐观锁并保留历史。
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

/// 声明组织版本提交后的授权刷新来源。
/// 组织事务完成后才可见的版本事件。
///
/// 不执行 I/O。
///
/// topic 为消费方主题。
pub fn changes<T>(topic: T) -> ChangeSource<T> {
    ChangeSource::shared(ORG_REVISIONS, topic)
}
