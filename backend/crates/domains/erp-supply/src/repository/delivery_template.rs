//! 供应商配送模板仓储查询。

#![allow(async_fn_in_trait)]

use erp_core::ids::{SupplierAccountId, SupplierDeliveryTemplateId};
use mongodb::bson::doc;
use persistence_core::{Executor, Result};

use crate::entity::supplier_offering::SupplierDeliveryTemplate;
use crate::repository::owned::{SupplierDeliveryTemplateRepository, SupplierOfferingRepository};

/// 配送模板集合查询。
pub trait SupplierDeliveryTemplateRepositoryExt {
    /// 批量读取多个供应商的全部未删除模板。
    ///
    /// # 参数
    /// * `supplier_ids` - 供应商集合
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 按供应商、名称排序的模板；空输入不查询。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    async fn list_by_suppliers(
        &self,
        supplier_ids: &[SupplierAccountId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierDeliveryTemplate>>;

    /// 读取供应商当前默认模板。
    ///
    /// # 参数
    /// * `supplier_id` - 供应商
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 存在默认模板时返回该模板。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    async fn find_default(
        &self,
        supplier_id: &SupplierAccountId,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierDeliveryTemplate>>;

    /// 按供应商与名称读取未删除模板。
    ///
    /// # 参数
    /// * `supplier_id` - 供应商
    /// * `name` - 已规范化名称
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 同名模板存在时返回该模板。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    async fn find_by_name(
        &self,
        supplier_id: &SupplierAccountId,
        name: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierDeliveryTemplate>>;
}

impl SupplierDeliveryTemplateRepositoryExt for SupplierDeliveryTemplateRepository<'_> {
    async fn list_by_suppliers(
        &self,
        supplier_ids: &[SupplierAccountId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierDeliveryTemplate>> {
        if supplier_ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids = supplier_ids.iter().map(ToString::to_string).collect::<Vec<_>>();
        self.find_many_sorted(
            doc! { "supplier_id": { "$in": ids } },
            doc! { "supplier_id": 1, "name": 1 },
            executor,
        )
        .await
    }

    async fn find_default(
        &self,
        supplier_id: &SupplierAccountId,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierDeliveryTemplate>> {
        self.find_one(doc! { "supplier_id": supplier_id.to_string(), "is_default": true }, executor).await
    }

    async fn find_by_name(
        &self,
        supplier_id: &SupplierAccountId,
        name: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierDeliveryTemplate>> {
        self.find_one(doc! { "supplier_id": supplier_id.to_string(), "name": name }, executor).await
    }
}

/// 供给对配送模板的引用查询。
pub trait OfferingDeliveryTemplateRepositoryExt {
    /// 判断是否仍有未删除供给指定该模板。
    ///
    /// # 参数
    /// * `template_id` - 模板主键
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 存在引用时返回 `true`。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    async fn references_delivery_template(
        &self,
        template_id: &SupplierDeliveryTemplateId,
        executor: &mut dyn Executor,
    ) -> Result<bool>;
}

impl OfferingDeliveryTemplateRepositoryExt for SupplierOfferingRepository<'_> {
    async fn references_delivery_template(
        &self,
        template_id: &SupplierDeliveryTemplateId,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        self.exists(doc! { "delivery_template_id": template_id.to_string() }, executor).await
    }
}
