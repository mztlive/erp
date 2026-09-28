//! 生成或恢复演示用的计量单位、品牌和分类。

use application_core::AuditActor;
use entity_core::{BaseModel, NOT_DELETED_TIMESTAMP};
use erp_catalog::{
    CatalogExt, CreateProductBrandRequest, CreateProductCategoryRequest, CreateUnitOfMeasureRequest,
    ProductKind, UpdateProductBrandRequest, UpdateProductCategoryRequest,
};
use persistence_core::NoTransaction;

use super::DemoMasterDataService;
use super::lifecycle::{self, DemoKindDictionary};
use super::plan::{self, DemoStep};
use super::record::{self, DemoMasterRecord};
use crate::{Error, Result};

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
    pub(super) async fn ensure_unit(&self, actor: &AuditActor, step: &DemoStep) -> Result<EnsureOutcome> {
        let (name, symbol) =
            plan::unit_spec(&step.key).ok_or_else(|| Error::Internal("演示单位不存在".into()))?;
        if let Some((id, live)) = self.lookup_unit(&step.key).await? {
            return self.adopt_coded(actor, step, DemoKindDictionary::Unit, id, live).await;
        }
        let view = crate::create_unit_of_measure(
            self.db.clone(),
            CreateUnitOfMeasureRequest {
                unit_code: step.key.clone(),
                name: name.to_string(),
                symbol: symbol.to_string(),
                quantity_scale: 0,
                status: None,
            },
            actor.clone(),
        )
        .await?;
        self.remember(step, view.id, Vec::new()).await?;
        Ok(EnsureOutcome::Created)
    }

    pub(super) async fn ensure_brand(&self, actor: &AuditActor, step: &DemoStep) -> Result<EnsureOutcome> {
        if let Some((id, live)) = self.lookup_brand(&step.key).await? {
            return self.adopt_coded(actor, step, DemoKindDictionary::Brand, id, live).await;
        }
        let view = self
            .catalog()
            .product_brand_create(CreateProductBrandRequest::new(step.key.clone(), step_label(step)), actor)
            .await?;
        self.remember(step, view.id, Vec::new()).await?;
        Ok(EnsureOutcome::Created)
    }

    pub(super) async fn ensure_category(&self, actor: &AuditActor, step: &DemoStep) -> Result<EnsureOutcome> {
        if let Some((id, live)) = self.lookup_category(&step.key).await? {
            return self.adopt_coded(actor, step, DemoKindDictionary::Category, id, live).await;
        }
        let view = crate::create_product_category(
            self.db.clone(),
            CreateProductCategoryRequest {
                category_code: step.key.clone(),
                parent_category_id: None,
                name: step_label(step),
                product_kind: ProductKind::Physical,
                status: None,
            },
            actor.clone(),
        )
        .await?;
        self.remember(step, view.id, Vec::new()).await?;
        Ok(EnsureOutcome::Created)
    }

    async fn adopt_coded(
        &self,
        actor: &AuditActor,
        step: &DemoStep,
        kind: DemoKindDictionary,
        id: String,
        live: bool,
    ) -> Result<EnsureOutcome> {
        if !live {
            lifecycle::restore_dictionary(&self.db, actor, kind, &id).await?;
        }
        self.refresh_dictionary_name(actor, step, kind, &id).await?;
        self.remember(step, id, Vec::new()).await?;
        Ok(if live { EnsureOutcome::Skipped } else { EnsureOutcome::Restored })
    }

    async fn refresh_dictionary_name(
        &self,
        actor: &AuditActor,
        step: &DemoStep,
        kind: DemoKindDictionary,
        id: &str,
    ) -> Result<()> {
        let name = step_label(step);
        match kind {
            DemoKindDictionary::Unit => Ok(()),
            DemoKindDictionary::Brand => self.refresh_brand(actor, id, &name).await,
            DemoKindDictionary::Category => self.refresh_category(actor, id, &name).await,
        }
    }

    async fn refresh_brand(&self, actor: &AuditActor, id: &str, name: &str) -> Result<()> {
        let Some(brand) =
            self.db.product_brands().find_by_id_including_deleted(id, &mut NoTransaction).await?
        else {
            return Ok(());
        };
        if !brand.name.contains('演') {
            return Ok(());
        }
        self.catalog()
            .product_brand_update(
                id,
                UpdateProductBrandRequest {
                    version: brand.base.version,
                    name: Some(name.to_string()),
                    status: None,
                    logo_file_asset_id: None,
                },
                actor,
            )
            .await?;
        Ok(())
    }

    async fn refresh_category(&self, actor: &AuditActor, id: &str, name: &str) -> Result<()> {
        let Some(category) =
            self.db.product_categories().find_by_id_including_deleted(id, &mut NoTransaction).await?
        else {
            return Ok(());
        };
        if !category.name.contains('演') {
            return Ok(());
        }
        self.catalog()
            .product_category_update(
                id,
                UpdateProductCategoryRequest {
                    version: category.base.version,
                    name: Some(name.to_string()),
                    product_kind: None,
                    status: None,
                    parent_change: None,
                },
                actor,
            )
            .await?;
        Ok(())
    }

    async fn lookup_unit(&self, key: &str) -> Result<Option<(String, bool)>> {
        Ok(coded_state(
            self.db
                .unit_of_measures()
                .find_one_by_field_including_deleted("unit_code", key.to_string(), &mut NoTransaction)
                .await?,
        ))
    }

    async fn lookup_brand(&self, key: &str) -> Result<Option<(String, bool)>> {
        Ok(coded_state(
            self.db
                .product_brands()
                .find_one_by_field_including_deleted("brand_code", key.to_string(), &mut NoTransaction)
                .await?,
        ))
    }

    async fn lookup_category(&self, key: &str) -> Result<Option<(String, bool)>> {
        Ok(coded_state(
            self.db
                .product_categories()
                .find_one_by_field_including_deleted("category_code", key.to_string(), &mut NoTransaction)
                .await?,
        ))
    }

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

pub(super) fn step_label(step: &DemoStep) -> String {
    super::names::label(step).to_string()
}
