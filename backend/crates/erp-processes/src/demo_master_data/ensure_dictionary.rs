//! 生成或恢复演示用的计量单位、品牌和分类。

use application_core::AuditActor;
use entity_core::{BaseModel, NOT_DELETED_TIMESTAMP};
use erp_catalog::{
    CatalogExt, CreateProductBrandRequest, CreateProductCategoryRequest, CreateUnitOfMeasureRequest,
};
use persistence_core::NoTransaction;

use super::DemoMasterDataService;
use super::lifecycle::{self, DemoKindDictionary};
use super::plan::DemoStep;
use super::record::{self, DemoMasterRecord};
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

impl DemoMasterDataService {
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
        if let Some((id, live)) = self.lookup_unit(&step.key).await? {
            return self.adopt_coded(actor, step, DemoKindDictionary::Unit, id, live).await;
        }
        let view = crate::create_unit_of_measure(self.db.clone(), request.clone(), actor.clone()).await?;
        self.remember(step, view.id, Vec::new()).await?;
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
        if let Some((id, live)) = self.lookup_brand(&step.key).await? {
            return self.adopt_coded(actor, step, DemoKindDictionary::Brand, id, live).await;
        }
        let view = self.catalog().product_brand_create(request.clone(), actor).await?;
        self.remember(step, view.id, Vec::new()).await?;
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
        if let Some((id, live)) = self.lookup_category(&step.key).await? {
            return self.adopt_coded(actor, step, DemoKindDictionary::Category, id, live).await;
        }
        let view = crate::create_product_category(self.db.clone(), request.clone(), actor.clone()).await?;
        self.remember(step, view.id, Vec::new()).await?;
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
            lifecycle::restore_dictionary(&self.db, actor, kind, &id).await?;
        }
        self.remember(step, id, Vec::new()).await?;
        Ok(if live { EnsureOutcome::Skipped } else { EnsureOutcome::Restored })
    }

    /// 按稳定单位编号读取包含软删除的数据。
    async fn lookup_unit(&self, key: &str) -> Result<Option<(String, bool)>> {
        Ok(coded_state(
            self.db
                .unit_of_measures()
                .find_one_by_field_including_deleted("unit_code", key.to_string(), &mut NoTransaction)
                .await?,
        ))
    }

    /// 按稳定品牌编号读取包含软删除的数据。
    async fn lookup_brand(&self, key: &str) -> Result<Option<(String, bool)>> {
        Ok(coded_state(
            self.db
                .product_brands()
                .find_one_by_field_including_deleted("brand_code", key.to_string(), &mut NoTransaction)
                .await?,
        ))
    }

    /// 按稳定分类编号读取包含软删除的数据。
    async fn lookup_category(&self, key: &str) -> Result<Option<(String, bool)>> {
        Ok(coded_state(
            self.db
                .product_categories()
                .find_one_by_field_including_deleted("category_code", key.to_string(), &mut NoTransaction)
                .await?,
        ))
    }

    /// 登记本次实际写入的主键及关联 ID。
    ///
    /// # 参数
    /// `step` - 种子身份；`entity_id` - 主键；`related_ids` - 从属记录主键。
    ///
    /// # 返回
    /// 登记成功返回空结果。
    ///
    /// # 错误
    /// 同一种子绑定了其他 ID 或数据库写入失败时返回错误。
    pub(super) async fn remember(
        &self,
        step: &DemoStep,
        entity_id: String,
        related_ids: Vec<String>,
    ) -> Result<()> {
        record::save(
            &self.db,
            &DemoMasterRecord {
                key: step.key.clone(),
                kind: step.kind.as_str().to_string(),
                entity_id,
                related_ids,
                label: step_label(step),
                removed: false,
            },
        )
        .await
    }
}

/// 提取字典记录的主键与有效状态。
fn coded_state<T: Coded>(entity: Option<T>) -> Option<(String, bool)> {
    entity.map(|item| (item.identity(), item.base().deleted_at == NOT_DELETED_TIMESTAMP))
}

trait Coded {
    fn identity(&self) -> String;
    fn base(&self) -> &BaseModel;
}

macro_rules! coded {
    ($ty:ty) => {
        impl Coded for $ty {
            fn identity(&self) -> String {
                self.base.id.clone()
            }
            fn base(&self) -> &BaseModel {
                &self.base
            }
        }
    };
}

coded!(erp_catalog::UnitOfMeasure);
coded!(erp_catalog::ProductBrand);
coded!(erp_catalog::ProductCategory);

/// 读取 JSON 种子的显示名称。
///
/// # 参数
/// `step` - 已校验的种子。
///
/// # 返回
/// 返回种子显示名称的副本。
///
/// # 错误
/// 无。
pub(super) fn step_label(step: &DemoStep) -> String {
    step.request.label().to_string()
}
