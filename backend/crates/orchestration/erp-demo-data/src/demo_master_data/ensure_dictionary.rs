//! 生成或恢复演示用的计量单位、品牌和分类。

use application_core::AuditActor;
use entity_core::HasBaseModel;
use erp_catalog::{
    CatalogExt, CreateProductBrandRequest, CreateProductCategoryRequest, CreateUnitOfMeasureRequest,
};
use erp_workflow::ports::WorkflowAuthorizationPort;
use persistence_core::{ReadExecutor, Repository};
use serde::Serialize;
use serde::de::DeserializeOwned;

use super::lifecycle::{self, DemoKindDictionary};
use super::plan::DemoStep;
use super::{DemoMasterDataProcess, record};
use crate::Result;

/// 一条演示主数据的处理结果。
pub(super) enum EnsureOutcome {
    /// 新写入。
    Created,
    /// 恢复了先前删除的同一条。
    Restored,
    /// 已经在列表中。
    Skipped,
    /// 前置条件不足，本条跳过并提示。
    Notice(String),
}

impl<A: WorkflowAuthorizationPort> DemoMasterDataProcess<A> {
    // 按稳定编号准备商品种子引用的计量单位：不存在则创建并登记，已存在时仅在登记归属一致后复用或恢复。
    // 归属、校验或写入失败时停止。
    pub(super) async fn ensure_unit(
        &self,
        actor: &AuditActor,
        step: &DemoStep,
        request: &CreateUnitOfMeasureRequest,
    ) -> Result<EnsureOutcome> {
        if let Some((id, live)) = lookup_coded(self.db.unit_of_measures(), "unit_code", &step.key).await? {
            return self.adopt_coded(actor, step, DemoKindDictionary::Unit, id, live).await;
        }

        let view = erp_master_data::create_unit_of_measure(
            self.db.clone(),
            self.authorization.clone(),
            request.clone(),
            actor.clone(),
        )
        .await?;
        record::save(&self.db, step, view.id, Vec::new()).await?;
        Ok(EnsureOutcome::Created)
    }

    // 按种子编号创建品牌或恢复同一已登记品牌，供后续商品解析品牌引用；不认领撞号的非演示记录，领域校验及持久化错误向上返回。
    pub(super) async fn ensure_brand(
        &self,
        actor: &AuditActor,
        step: &DemoStep,
        request: &CreateProductBrandRequest,
    ) -> Result<EnsureOutcome> {
        if let Some((id, live)) = lookup_coded(self.db.product_brands(), "brand_code", &step.key).await? {
            return self.adopt_coded(actor, step, DemoKindDictionary::Brand, id, live).await;
        }

        let view = self.ports.catalog.product_brand_create(request.clone(), actor).await?;
        record::save(&self.db, step, view.id, Vec::new()).await?;
        Ok(EnsureOutcome::Created)
    }

    // 为商品种子准备可引用的分类，按编号核对登记归属后复用或恢复，缺失时走正式创建；归属不符、校验或写入错误时停止。
    pub(super) async fn ensure_category(
        &self,
        actor: &AuditActor,
        step: &DemoStep,
        request: &CreateProductCategoryRequest,
    ) -> Result<EnsureOutcome> {
        if let Some((id, live)) =
            lookup_coded(self.db.product_categories(), "category_code", &step.key).await?
        {
            return self.adopt_coded(actor, step, DemoKindDictionary::Category, id, live).await;
        }

        let view = erp_master_data::create_product_category(
            self.db.clone(),
            self.authorization.clone(),
            self.ports.catalog.clone(),
            request.clone(),
            actor.clone(),
        )
        .await?;
        record::save(&self.db, step, view.id, Vec::new()).await?;
        Ok(EnsureOutcome::Created)
    }

    // 比对种子键对应的实际 ID 后才恢复或复用字典，避免把同编号的真实资料登记为演示数据。
    async fn adopt_coded(
        &self,
        actor: &AuditActor,
        step: &DemoStep,
        kind: DemoKindDictionary,
        id: String,
        live: bool,
    ) -> Result<EnsureOutcome> {
        record::ensure_owned(&self.db, &step.key, &id).await?;
        if !live {
            lifecycle::restore_dictionary(&self.db, self.authorization.clone(), actor, kind, &id).await?;
        }

        record::save(&self.db, step, id, Vec::new()).await?;
        Ok(if live { EnsureOutcome::Skipped } else { EnsureOutcome::Restored })
    }
}

// 按固定编号查字典并保留软删除项，返回实际 ID 和删除状态，使后续能区分创建、复用与恢复。
async fn lookup_coded<T>(
    repository: Repository<'_, T>,
    field: &str,
    key: &str,
) -> Result<Option<(String, bool)>>
where
    T: HasBaseModel + Serialize + DeserializeOwned + Send + Sync,
{
    let entity = repository
        .find_one_by_field_including_deleted(field, key.to_string(), &mut ReadExecutor::default())
        .await?;
    Ok(entity.map(|item| {
        let base = item.base();
        (base.id.clone(), !base.is_deleted())
    }))
}
