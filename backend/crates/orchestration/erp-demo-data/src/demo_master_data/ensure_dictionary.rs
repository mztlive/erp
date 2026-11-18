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
    /// 读取单位种子并创建或恢复已登记记录。
    ///
    /// # 参数
    /// `actor` - 操作人；`step` - 种子身份；`request` - 单位创建输入。
    ///
    /// # 返回
    /// 返回创建、恢复或已存在结果。
    ///
    /// # 错误
    /// 编号归属不符、领域校验或持久化失败时返回错误。
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

    /// 读取品牌种子并创建或恢复已登记记录。
    ///
    /// # 参数
    /// `actor` - 操作人；`step` - 种子身份；`request` - 品牌创建输入。
    ///
    /// # 返回
    /// 返回创建、恢复或已存在结果。
    ///
    /// # 错误
    /// 编号归属不符、领域校验或持久化失败时返回错误。
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

    /// 读取分类种子并创建或恢复已登记记录。
    ///
    /// # 参数
    /// `actor` - 操作人；`step` - 种子身份；`request` - 分类创建输入。
    ///
    /// # 返回
    /// 返回创建、恢复或已存在结果。
    ///
    /// # 错误
    /// 编号归属不符、领域校验或持久化失败时返回错误。
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

    /// 校验实际 ID 归属后恢复已有字典。
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

/// 按稳定编号读取含软删除的字典记录，并返回恢复所需的身份与状态。
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
