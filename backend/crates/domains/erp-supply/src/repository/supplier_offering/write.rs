//! 同域供给写入的实际Mongo provider与顺序合同；不开启事务。
use async_trait::async_trait;
use mongodb::Database;
use persistence_core::{Executor, Result};

use crate::entity::supplier_offering::{
    SupplierOffering, SupplierOfferingAvailability, SupplierOfferingCommand, SupplierOfferingRevision,
};
use crate::repository::SupplierOfferingExt;
/// 同域持久化步骤，既有复合仓储和命令写入共用唯一实现。
#[async_trait]
#[allow(async_fn_in_trait)]
pub(crate) trait OfferingWritePort: Sync {
    // 插入供给稳定身份。
    // 写入失败时返回对应错误。
    async fn offering(&self, value: &SupplierOffering, executor: &mut dyn Executor) -> Result<()>;
    // 插入商业条款修订。
    // 写入失败时返回对应错误。
    async fn revision(&self, value: &SupplierOfferingRevision, executor: &mut dyn Executor) -> Result<()>;
    // 插入可供投影。
    // 写入失败时返回对应错误。
    async fn availability(
        &self,
        value: &SupplierOfferingAvailability,
        executor: &mut dyn Executor,
    ) -> Result<()>;
    // 更新供给稳定身份。
    // 写入失败时返回对应错误。
    async fn update_offering(&self, value: &mut SupplierOffering, executor: &mut dyn Executor) -> Result<()>;
    // 更新可供投影。
    // 写入失败时返回对应错误。
    async fn update_availability(
        &self,
        value: &mut SupplierOfferingAvailability,
        executor: &mut dyn Executor,
    ) -> Result<()>;
    // 写入供给命令去重记录。
    // 写入失败时返回对应错误。
    async fn command(&self, value: &SupplierOfferingCommand, executor: &mut dyn Executor) -> Result<()>;
}

#[async_trait]
impl OfferingWritePort for Database {
    async fn offering(&self, value: &SupplierOffering, executor: &mut dyn Executor) -> Result<()> {
        self.supplier_offerings().create(value, executor).await
    }
    async fn revision(&self, value: &SupplierOfferingRevision, executor: &mut dyn Executor) -> Result<()> {
        self.supplier_offering_revisions().create(value, executor).await
    }
    async fn availability(
        &self,
        value: &SupplierOfferingAvailability,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.supplier_offering_availabilities().create(value, executor).await
    }
    async fn update_offering(&self, value: &mut SupplierOffering, executor: &mut dyn Executor) -> Result<()> {
        self.supplier_offerings().update(value, executor).await
    }
    async fn update_availability(
        &self,
        value: &mut SupplierOfferingAvailability,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.supplier_offering_availabilities().update(value, executor).await
    }
    async fn command(&self, value: &SupplierOfferingCommand, executor: &mut dyn Executor) -> Result<()> {
        self.supplier_offering_commands().create(value, executor).await
    }
}
// 按供给、修订、可供投影的顺序写入；任一步失败则不再写后续集合。
// 三项都写入成功时无返回值。
// 任一步写入失败时返回该步错误，并停止后续写入。
// `executor`：本函数不开启事务。
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
// 先插入新修订，成功后再更新供给当前指针。
// 修订插入且供给更新都成功时无返回值。
// 修订写入失败时不更新供给；供给更新失败时返回该错误。
// `executor`：本函数不开启事务。
pub(crate) async fn append_revision<P: OfferingWritePort>(
    port: &P,
    offering: &mut SupplierOffering,
    revision: &SupplierOfferingRevision,
    executor: &mut dyn Executor,
) -> Result<()> {
    port.revision(revision, executor).await?;
    port.update_offering(offering, executor).await
}
