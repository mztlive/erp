//! 同域供给写入的实际Mongo provider与顺序合同；不开启事务。
use async_trait::async_trait;
use mongodb::Database;
use persistence_core::{Executor, Result, mongo_ops};

use super::{OFFERING_AVAILABILITIES, OFFERING_REVISIONS, OFFERINGS};
use crate::entity::supplier_offering::{
    SupplierOffering, SupplierOfferingAvailability, SupplierOfferingCommand, SupplierOfferingRevision,
};
use crate::repository::SupplierOfferingExt;
/// 同域持久化步骤，既有复合仓储和命令写入共用唯一实现。
#[async_trait]
#[allow(async_fn_in_trait)]
pub(crate) trait OfferingWritePort: Sync {
    /// 插入供给稳定身份。
    ///
    /// # 参数
    /// * `value` - 待插入的供给
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 插入成功时无返回值。
    ///
    /// # 错误
    /// 写入失败时返回对应错误。
    async fn offering(&self, value: &SupplierOffering, executor: &mut dyn Executor) -> Result<()>;
    /// 插入商业条款修订。
    ///
    /// # 参数
    /// * `value` - 待插入的修订
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 插入成功时无返回值。
    ///
    /// # 错误
    /// 写入失败时返回对应错误。
    async fn revision(&self, value: &SupplierOfferingRevision, executor: &mut dyn Executor) -> Result<()>;
    /// 插入可供投影。
    ///
    /// # 参数
    /// * `value` - 待插入的可供投影
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 插入成功时无返回值。
    ///
    /// # 错误
    /// 写入失败时返回对应错误。
    async fn availability(
        &self,
        value: &SupplierOfferingAvailability,
        executor: &mut dyn Executor,
    ) -> Result<()>;
    /// 更新供给稳定身份。
    ///
    /// # 参数
    /// * `value` - 待写回的供给
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 更新成功时无返回值。
    ///
    /// # 错误
    /// 写入失败时返回对应错误。
    async fn update_offering(&self, value: &mut SupplierOffering, executor: &mut dyn Executor) -> Result<()>;
    /// 更新可供投影。
    ///
    /// # 参数
    /// * `value` - 待写回的可供投影
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 更新成功时无返回值。
    ///
    /// # 错误
    /// 写入失败时返回对应错误。
    async fn update_availability(
        &self,
        value: &mut SupplierOfferingAvailability,
        executor: &mut dyn Executor,
    ) -> Result<()>;
    /// 写入供给命令去重记录。
    ///
    /// # 参数
    /// * `value` - 待写入的命令
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 写入成功时无返回值。
    ///
    /// # 错误
    /// 写入失败时返回对应错误。
    async fn command(&self, value: &SupplierOfferingCommand, executor: &mut dyn Executor) -> Result<()>;
}
pub(crate) struct MongoOfferingWrite<'a> {
    db: &'a Database,
}
impl<'a> MongoOfferingWrite<'a> {
    /// 绑定目标数据库的供给写入端口。
    ///
    /// # 参数
    /// * `db` - 目标数据库
    ///
    /// # 返回
    /// 返回写入端口。
    ///
    /// # 错误
    /// 不返回错误。
    pub(crate) fn new(db: &'a Database) -> Self {
        Self { db }
    }
}
#[async_trait]
impl OfferingWritePort for MongoOfferingWrite<'_> {
    async fn offering(&self, value: &SupplierOffering, executor: &mut dyn Executor) -> Result<()> {
        mongo_ops::insert_one(&self.db.collection::<SupplierOffering>(OFFERINGS), value, executor).await
    }
    async fn revision(&self, value: &SupplierOfferingRevision, executor: &mut dyn Executor) -> Result<()> {
        mongo_ops::insert_one(
            &self.db.collection::<SupplierOfferingRevision>(OFFERING_REVISIONS),
            value,
            executor,
        )
        .await
    }
    async fn availability(
        &self,
        value: &SupplierOfferingAvailability,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self.db.collection::<SupplierOfferingAvailability>(OFFERING_AVAILABILITIES),
            value,
            executor,
        )
        .await
    }
    async fn update_offering(&self, value: &mut SupplierOffering, executor: &mut dyn Executor) -> Result<()> {
        self.db.supplier_offerings().update(value, executor).await
    }
    async fn update_availability(
        &self,
        value: &mut SupplierOfferingAvailability,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.db.supplier_offering_availabilities().update(value, executor).await
    }
    async fn command(&self, value: &SupplierOfferingCommand, executor: &mut dyn Executor) -> Result<()> {
        self.db.supplier_offering_commands().create(value, executor).await
    }
}
/// 按供给、修订、可供投影的顺序写入；任一步失败则不再写后续集合。
///
/// # 参数
/// * `port` - 供给写入端口
/// * `offering` - 待插入的供给稳定身份
/// * `revision` - 待插入的商业条款修订
/// * `availability` - 待插入的可供投影
/// * `executor` - 调用方执行器；本函数不开启事务
///
/// # 返回
/// 三项都写入成功时无返回值。
///
/// # 错误
/// 任一步写入失败时返回该步错误，并停止后续写入。
pub(crate) async fn create_triple<P: OfferingWritePort>(
    port: &P,
    offering: &SupplierOffering,
    revision: &SupplierOfferingRevision,
    availability: &SupplierOfferingAvailability,
    executor: &mut dyn Executor,
) -> Result<()> {
    port.offering(offering, executor).await?;
    port.revision(revision, executor).await?;
    port.availability(availability, executor).await
}
/// 先插入新修订，成功后再更新供给当前指针。
///
/// # 参数
/// * `port` - 供给写入端口
/// * `offering` - 已改好当前修订指针、待更新的供给
/// * `revision` - 待插入的新商业条款修订
/// * `executor` - 调用方执行器；本函数不开启事务
///
/// # 返回
/// 修订插入且供给更新都成功时无返回值。
///
/// # 错误
/// 修订写入失败时不更新供给；供给更新失败时返回该错误。
pub(crate) async fn append_revision<P: OfferingWritePort>(
    port: &P,
    offering: &mut SupplierOffering,
    revision: &SupplierOfferingRevision,
    executor: &mut dyn Executor,
) -> Result<()> {
    port.revision(revision, executor).await?;
    port.update_offering(offering, executor).await
}
