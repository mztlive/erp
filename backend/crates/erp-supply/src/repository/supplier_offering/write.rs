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
    async fn offering(&self, value: &SupplierOffering, executor: &mut dyn Executor) -> Result<()>;
    async fn revision(&self, value: &SupplierOfferingRevision, executor: &mut dyn Executor) -> Result<()>;
    async fn availability(
        &self,
        value: &SupplierOfferingAvailability,
        executor: &mut dyn Executor,
    ) -> Result<()>;
    async fn update_offering(&self, value: &mut SupplierOffering, executor: &mut dyn Executor) -> Result<()>;
    async fn update_availability(
        &self,
        value: &mut SupplierOfferingAvailability,
        executor: &mut dyn Executor,
    ) -> Result<()>;
    async fn command(&self, value: &SupplierOfferingCommand, executor: &mut dyn Executor) -> Result<()>;
}
pub(crate) struct MongoOfferingWrite<'a> {
    db: &'a Database,
}
impl<'a> MongoOfferingWrite<'a> {
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
/// 原三元组写序，首个错误直接停止后续写入。
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
/// 原新修订insert后才推进供给CAS。
pub(crate) async fn append_revision<P: OfferingWritePort>(
    port: &P,
    offering: &mut SupplierOffering,
    revision: &SupplierOfferingRevision,
    executor: &mut dyn Executor,
) -> Result<()> {
    port.revision(revision, executor).await?;
    port.update_offering(offering, executor).await
}
