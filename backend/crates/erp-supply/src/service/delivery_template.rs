//! 配送模板单域用例：保存、删除、供给指定与生效政策解析。
//!
//! 供应商存在性、供给访问范围与审计由调用方 Process 在同一事务内完成；本服务
//! 只维护模板名称唯一、默认唯一、引用保护与供给指定的同供应商约束。

use std::collections::{HashMap, HashSet};

use erp_core::ids::{SupplierAccountId, SupplierDeliveryTemplateId};
use mongodb::Database;
use persistence_core::Executor;

use crate::entity::supplier_offering::{
    ResolvedDelivery, SupplierDeliveryTemplate, SupplierDeliveryTemplateContent, SupplierOffering,
    resolve_offering_delivery,
};
use crate::repository::SupplierOfferingExt;
use crate::repository::prelude::*;
use crate::{Error, Result};

/// 配送模板服务。
pub struct DeliveryTemplateService {
    db: Database,
}

impl DeliveryTemplateService {
    /// 创建服务。
    ///
    /// # 参数
    /// * `db` - 数据库
    ///
    /// # 返回
    /// 返回服务实例。
    ///
    /// # 错误
    /// 无。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 查询供应商全部模板。
    ///
    /// # 参数
    /// * `supplier_id` - 供应商
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 按名称排序的模板。
    ///
    /// # 错误
    /// 查询失败时返回错误。
    pub async fn supplier_templates(
        &self,
        supplier_id: &SupplierAccountId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierDeliveryTemplate>> {
        let templates = self
            .db
            .supplier_delivery_templates()
            .list_by_suppliers(std::slice::from_ref(supplier_id), executor)
            .await?;
        Ok(templates)
    }

    /// 新建模板；设为默认时取消原默认模板。
    ///
    /// # 参数
    /// * `id` - 调用方预先生成的模板主键
    /// * `supplier_id` - 已由调用方确认存在的供应商
    /// * `content` - 模板内容
    /// * `actor` - 操作人
    /// * `executor` - 调用方事务执行器
    ///
    /// # 返回
    /// 返回已保存模板。
    ///
    /// # 错误
    /// 内容不合法、名称重复或写入失败时返回错误。
    pub async fn create(
        &self,
        id: SupplierDeliveryTemplateId,
        supplier_id: SupplierAccountId,
        content: SupplierDeliveryTemplateContent,
        actor: &str,
        executor: &mut dyn Executor,
    ) -> Result<SupplierDeliveryTemplate> {
        let template = SupplierDeliveryTemplate::new(id, supplier_id, content, actor)?;
        self.ensure_name_available(&template, executor).await?;
        if template.is_default {
            self.clear_other_default(&template, actor, executor).await?;
        }
        self.db.supplier_delivery_templates().create(&template, executor).await?;
        Ok(template)
    }

    /// 整体保存模板内容。
    ///
    /// # 参数
    /// * `id` - 模板主键
    /// * `expected_version` - 调用方持有的版本
    /// * `content` - 新内容
    /// * `actor` - 操作人
    /// * `executor` - 调用方事务执行器
    ///
    /// # 返回
    /// 返回保存后的模板；内容未变化时原样返回。
    ///
    /// # 错误
    /// 模板不存在、版本不一致、内容不合法、名称重复或写入失败时返回错误。
    pub async fn update(
        &self,
        id: &str,
        expected_version: u64,
        content: SupplierDeliveryTemplateContent,
        actor: &str,
        executor: &mut dyn Executor,
    ) -> Result<SupplierDeliveryTemplate> {
        let mut template = self.require(id, expected_version, executor).await?;
        if !template.replace_content(content, actor)? {
            return Ok(template);
        }
        self.ensure_name_available(&template, executor).await?;
        if template.is_default {
            self.clear_other_default(&template, actor, executor).await?;
        }
        self.db.supplier_delivery_templates().update(&mut template, executor).await?;
        Ok(template)
    }

    /// 删除未被供给引用的模板。
    ///
    /// # 参数
    /// * `id` - 模板主键
    /// * `expected_version` - 调用方持有的版本
    /// * `executor` - 调用方事务执行器
    ///
    /// # 返回
    /// 返回删除前的模板。
    ///
    /// # 错误
    /// 模板不存在、版本不一致、仍被供给引用或写入失败时返回错误。
    pub async fn delete(
        &self,
        id: &str,
        expected_version: u64,
        executor: &mut dyn Executor,
    ) -> Result<SupplierDeliveryTemplate> {
        let mut template = self.require(id, expected_version, executor).await?;
        let template_id = SupplierDeliveryTemplateId::new(template.base.id.clone());
        if self.db.supplier_offerings().references_delivery_template(&template_id, executor).await? {
            return Err(Error::BusinessLogicError("该模板仍被供给使用，请先为这些供给更换模板".into()));
        }
        let snapshot = template.clone();
        self.db.supplier_delivery_templates().soft_delete(&mut template, executor).await?;
        Ok(snapshot)
    }

    /// 为已通过访问校验的供给指定模板或改回跟随默认。
    ///
    /// # 参数
    /// * `offerings` - 调用方已逐条校验写访问的供给
    /// * `template_id` - 目标模板；`None` 表示跟随供应商默认模板
    /// * `actor` - 操作人
    /// * `executor` - 调用方事务执行器
    ///
    /// # 返回
    /// 返回实际发生变化的供给数。
    ///
    /// # 错误
    /// 模板不存在、模板与供给不属于同一供应商或写入失败时返回错误。
    pub async fn assign(
        &self,
        offerings: Vec<SupplierOffering>,
        template_id: Option<&str>,
        actor: &str,
        executor: &mut dyn Executor,
    ) -> Result<usize> {
        let template = match template_id.map(str::trim).filter(|id| !id.is_empty()) {
            Some(id) => Some(self.find(id, executor).await?),
            None => None,
        };
        let mut updated = 0;
        for mut offering in offerings {
            if template.as_ref().is_some_and(|template| template.supplier_id != offering.supplier_id) {
                return Err(Error::ValidationError("配送模板不属于所选供给的供应商".into()));
            }
            let target =
                template.as_ref().map(|template| SupplierDeliveryTemplateId::new(template.base.id.clone()));
            if offering.assign_delivery_template(target, actor) {
                self.db.supplier_offerings().update(&mut offering, executor).await?;
                updated += 1;
            }
        }
        Ok(updated)
    }

    /// 批量解析供给当前生效的配送政策。
    ///
    /// # 参数
    /// * `offerings` - 供给
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 以供给主键索引的生效政策；查询次数与供给数量无关。
    ///
    /// # 错误
    /// 查询失败时返回错误。
    pub async fn resolve_for_offerings(
        &self,
        offerings: &[SupplierOffering],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, ResolvedDelivery>> {
        let mut seen = HashSet::new();
        let supplier_ids = offerings
            .iter()
            .filter(|offering| seen.insert(offering.supplier_id.clone()))
            .map(|offering| offering.supplier_id.clone())
            .collect::<Vec<_>>();
        let templates =
            self.db.supplier_delivery_templates().list_by_suppliers(&supplier_ids, executor).await?;
        Ok(offerings
            .iter()
            .map(|offering| (offering.base.id.clone(), resolve_offering_delivery(offering, &templates)))
            .collect())
    }

    async fn find(&self, id: &str, executor: &mut dyn Executor) -> Result<SupplierDeliveryTemplate> {
        self.db
            .supplier_delivery_templates()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("配送模板不存在".into()))
    }

    async fn require(
        &self,
        id: &str,
        expected_version: u64,
        executor: &mut dyn Executor,
    ) -> Result<SupplierDeliveryTemplate> {
        let template = self.find(id, executor).await?;
        if template.base.version != expected_version {
            return Err(Error::ConflictError("配送模板已被修改，请刷新后重试".into()));
        }
        Ok(template)
    }

    async fn ensure_name_available(
        &self,
        template: &SupplierDeliveryTemplate,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let existing = self
            .db
            .supplier_delivery_templates()
            .find_by_name(&template.supplier_id, &template.name, executor)
            .await?;
        if existing.is_some_and(|existing| existing.base.id != template.base.id) {
            return Err(Error::ConflictError("该供应商已有同名配送模板".into()));
        }
        Ok(())
    }

    async fn clear_other_default(
        &self,
        template: &SupplierDeliveryTemplate,
        actor: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let repository = self.db.supplier_delivery_templates();
        if let Some(mut current) = repository.find_default(&template.supplier_id, executor).await?
            && current.base.id != template.base.id
            && current.clear_default(actor)
        {
            repository.update(&mut current, executor).await?;
        }
        Ok(())
    }
}
