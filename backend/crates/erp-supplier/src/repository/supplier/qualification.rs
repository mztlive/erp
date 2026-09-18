use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::ids::{SupplierAccountId, SupplierQualificationId};
use mongodb::bson::{Document, doc};
use persistence_core::{Executor, Pagination, QueryFilter, Result};

use super::{SUPPLIER_QUALIFICATIONS, SupplierRepository};
use crate::entity::supplier::{
    QualificationStatus, QualificationType, SupplierQualification, SupplierQualificationCapability,
};
use crate::repository::owned::{SupplierQualificationCapabilityRepository, SupplierQualificationRepository};

/// 供应商资质列表筛选条件。
#[derive(Debug, Clone)]
pub struct SupplierQualificationFilter {
    /// 供应商角色 ID；`None` 表示不筛选。
    pub supplier_id: Option<SupplierAccountId>,
    /// 资质类型；`None` 表示不筛选。
    pub qualification_type: Option<QualificationType>,
    /// 资质状态；`None` 表示不筛选。
    pub status: Option<QualificationStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（仓储白名单，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl Default for SupplierQualificationFilter {
    fn default() -> Self {
        Self {
            supplier_id: None,
            qualification_type: None,
            status: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        }
    }
}

/// 可核实日期的资质；合同不能把未知截止日期当作长期有效。
fn verified_window_filter() -> Document {
    doc! { "valid_from": { "$type": "string" }, "$or": [
        { "qualification_type": { "$ne": "contract" } }, { "valid_to": { "$type": "string" } }
    ] }
}

impl QueryFilter for SupplierQualificationFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(supplier_id) = &self.supplier_id {
            filter.insert("supplier_id", supplier_id.to_string());
        }
        if let Some(qualification_type) = self.qualification_type {
            filter.insert("qualification_type", qualification_type.as_str());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for SupplierQualificationFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> SupplierQualificationRepository<'a> {
    /// 检索资质的到期预警列表（§6.2：`valid_to + status` 到期预警索引），
    /// 按 `valid_to` 升序。
    ///
    /// # 参数
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部有效资质（含已到期记录，由调用方按业务日期过滤）。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_active_for_expiry_warning(
        &self,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierQualification>> {
        self.find_many_sorted(
            doc! { "status": QualificationStatus::Active.as_str() },
            doc! { "valid_to": 1 },
            executor,
        )
        .await
    }

    /// 查询已登记任一指定资质类型的供应商角色 ID。
    ///
    /// # 参数
    /// * `qualification_types` - 资质类型；空集合表示不限制类型
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回去重、稳定排序后的供应商角色 ID。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_supplier_ids_by_qualification_types(
        &self,
        qualification_types: &[QualificationType],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierAccountId>> {
        super::find_supplier_ids(
            self.collection().clone_with_type(),
            qualification_type_filter(qualification_types),
            executor,
        )
        .await
    }

    /// 查询当前有效的供应商资质对应的供应商角色 ID。
    ///
    /// # 参数
    /// * `qualification_types` - 资质类型；空集合表示不限制类型
    /// * `as_of` - 当前业务日，格式为 `YYYY-MM-DD`
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回去重、稳定排序后的供应商角色 ID。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_supplier_ids_by_valid_qualifications(
        &self,
        qualification_types: &[QualificationType],
        as_of: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierAccountId>> {
        let mut filter = active_window_filter(qualification_types, as_of);
        filter.insert("$or", vec![doc! { "valid_to": null }, doc! { "valid_to": { "$gte": as_of } }]);
        super::find_supplier_ids(self.collection().clone_with_type(), filter, executor).await
    }

    /// 查询将在指定日期前到期且当前仍有效的供应商资质对应的供应商角色 ID。
    ///
    /// # 参数
    /// * `qualification_types` - 资质类型；空集合表示不限制类型
    /// * `as_of` - 当前业务日，格式为 `YYYY-MM-DD`
    /// * `expires_by` - 到期窗口的结束业务日，格式为 `YYYY-MM-DD`
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回去重、稳定排序后的供应商角色 ID。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_supplier_ids_by_expiring_qualifications(
        &self,
        qualification_types: &[QualificationType],
        as_of: &str,
        expires_by: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierAccountId>> {
        let mut filter = active_window_filter(qualification_types, as_of);
        filter.insert("valid_to", doc! { "$gte": as_of, "$lte": expires_by });
        super::find_supplier_ids(self.collection().clone_with_type(), filter, executor).await
    }

    /// 查询已失效供应商资质对应的供应商角色 ID。
    ///
    /// # 参数
    /// * `qualification_types` - 资质类型；空集合表示不限制类型
    /// * `as_of` - 当前业务日，格式为 `YYYY-MM-DD`
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回去重、稳定排序后的供应商角色 ID。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_supplier_ids_by_expired_qualifications(
        &self,
        qualification_types: &[QualificationType],
        as_of: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierAccountId>> {
        let mut filter = qualification_type_filter(qualification_types);
        filter.insert("$and", vec![verified_window_filter()]);
        filter.insert(
            "$or",
            vec![
                doc! { "status": QualificationStatus::Expired.as_str() },
                doc! {
                    "status": QualificationStatus::Active.as_str(),
                    "valid_to": { "$lt": as_of },
                },
            ],
        );
        super::find_supplier_ids(self.collection().clone_with_type(), filter, executor).await
    }
}

impl SupplierQualificationRepository<'_> {
    /// 读取指定供应商能力已经关联的合同，沿用调用方执行器。
    ///
    /// # Errors
    /// 关联或资质读取失败时返回错误，不将读取失败当作没有合同。
    pub async fn linked_contracts(
        &self,
        supplier_id: &SupplierAccountId,
        capability_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierQualification>> {
        use crate::repository::SupplierExt;
        let links = self
            .database()
            .supplier_qualification_capabilities()
            .find_many(doc! { "capability_id": capability_id }, executor)
            .await?;
        let ids: Vec<String> = links.into_iter().map(|link| link.qualification_id.to_string()).collect();
        if ids.is_empty() {
            return Ok(vec![]);
        }
        self.find_many(doc! { "id": { "$in": ids }, "supplier_id": supplier_id.to_string(), "qualification_type": "contract" }, executor).await
    }
}

impl<'a> SupplierQualificationCapabilityRepository<'a> {
    /// 批量读取指定资质的适用能力关联。
    ///
    /// # Errors
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_by_qualification_ids(
        &self,
        qualification_ids: &[SupplierQualificationId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierQualificationCapability>> {
        if qualification_ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<String> = qualification_ids.iter().map(ToString::to_string).collect();
        self.find_many(doc! { "qualification_id": { "$in": ids } }, executor).await
    }
}

/// 构建有效资质窗口的公共前缀条件。
///
/// 资质类型范围、启用状态、日期核实与生效日上限在有效/窗口内到期两条
/// 查询中共用；调用方只追加各自的失效日窗口。
///
/// # 参数
/// * `qualification_types` - 允许命中的资质类型集合
/// * `as_of` - 当前业务日，格式为 `YYYY-MM-DD`
///
/// # 返回
/// 返回已含类型、状态、核实与生效日条件的查询文档。
fn active_window_filter(qualification_types: &[QualificationType], as_of: &str) -> Document {
    let mut filter = qualification_type_filter(qualification_types);
    filter.insert("status", QualificationStatus::Active.as_str());
    filter.insert("$and", vec![verified_window_filter()]);
    filter.insert("valid_from", doc! { "$lte": as_of });
    filter
}

/// 构建资质类型范围条件；空集合表示不限制类型。
///
/// # 参数
/// * `qualification_types` - 允许命中的资质类型集合
///
/// # 返回
/// 非空时返回 `$in` 条件，空集合时返回空文档。
fn qualification_type_filter(qualification_types: &[QualificationType]) -> Document {
    if qualification_types.is_empty() {
        return Document::new();
    }
    let types: Vec<&str> = qualification_types.iter().map(QualificationType::as_str).collect();
    doc! { "qualification_type": { "$in": types } }
}

impl<'a> SupplierRepository<'a> {
    /// 查询已登记但日期不完整的合同所属供应商。
    ///
    /// # Errors
    /// 数据库读取失败时返回错误；不与未登记资质混淆。
    pub async fn list_supplier_ids_by_unverified_qualifications(
        &self,
        qualification_types: &[QualificationType],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierAccountId>> {
        if !qualification_types.is_empty() && !qualification_types.contains(&QualificationType::Contract) {
            return Ok(vec![]);
        }
        let filter =
            doc! { "qualification_type": "contract", "$or": [{ "valid_from": null }, { "valid_to": null }] };
        super::find_supplier_ids(self.db.collection(SUPPLIER_QUALIFICATIONS), filter, executor).await
    }

    /// 查询已登记任一指定资质类型的供应商角色 ID。
    ///
    /// # 参数
    /// * `qualification_types` - 资质类型；空集合表示不限制类型
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回去重、稳定排序后的供应商角色 ID。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_supplier_ids_by_qualification_types(
        &self,
        qualification_types: &[QualificationType],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierAccountId>> {
        SupplierQualificationRepository::new(self.db, SUPPLIER_QUALIFICATIONS)
            .list_supplier_ids_by_qualification_types(qualification_types, executor)
            .await
    }

    /// 查询当前有效资质对应的供应商角色 ID。
    ///
    /// # 参数
    /// * `qualification_types` - 资质类型；空集合表示不限制类型
    /// * `as_of` - 当前业务日，格式为 `YYYY-MM-DD`
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回去重、稳定排序后的供应商角色 ID。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_supplier_ids_by_valid_qualifications(
        &self,
        qualification_types: &[QualificationType],
        as_of: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierAccountId>> {
        SupplierQualificationRepository::new(self.db, SUPPLIER_QUALIFICATIONS)
            .list_supplier_ids_by_valid_qualifications(qualification_types, as_of, executor)
            .await
    }

    /// 查询将在窗口内到期且当前仍有效的资质对应的供应商角色 ID。
    ///
    /// # 参数
    /// * `qualification_types` - 资质类型；空集合表示不限制类型
    /// * `as_of` - 当前业务日，格式为 `YYYY-MM-DD`
    /// * `expires_by` - 到期窗口结束业务日
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回去重、稳定排序后的供应商角色 ID。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_supplier_ids_by_expiring_qualifications(
        &self,
        qualification_types: &[QualificationType],
        as_of: &str,
        expires_by: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierAccountId>> {
        SupplierQualificationRepository::new(self.db, SUPPLIER_QUALIFICATIONS)
            .list_supplier_ids_by_expiring_qualifications(qualification_types, as_of, expires_by, executor)
            .await
    }

    /// 查询已失效资质对应的供应商角色 ID。
    ///
    /// # 参数
    /// * `qualification_types` - 资质类型；空集合表示不限制类型
    /// * `as_of` - 当前业务日，格式为 `YYYY-MM-DD`
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回去重、稳定排序后的供应商角色 ID。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_supplier_ids_by_expired_qualifications(
        &self,
        qualification_types: &[QualificationType],
        as_of: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierAccountId>> {
        SupplierQualificationRepository::new(self.db, SUPPLIER_QUALIFICATIONS)
            .list_supplier_ids_by_expired_qualifications(qualification_types, as_of, executor)
            .await
    }

    /// 按当前页供应商角色 ID 批量读取资质。
    ///
    /// # 参数
    /// * `supplier_ids` - 当前页供应商角色 ID；空集合时不访问数据库
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回这些供应商的全部未删除资质，不按页大小之外的供应商扩展。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_qualifications_by_supplier_ids(
        &self,
        supplier_ids: &[SupplierAccountId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierQualification>> {
        if supplier_ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<String> = supplier_ids.iter().map(ToString::to_string).collect();
        SupplierQualificationRepository::new(self.db, SUPPLIER_QUALIFICATIONS)
            .find_many(doc! { "supplier_id": { "$in": ids } }, executor)
            .await
    }

    /// 按创建时间倒序读取供应商全部资质。
    ///
    /// # 参数
    /// * `supplier_id` - 供应商角色 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回该供应商的全部未删除资质。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_qualifications(
        &self,
        supplier_id: &SupplierAccountId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierQualification>> {
        SupplierQualificationRepository::new(self.db, SUPPLIER_QUALIFICATIONS)
            .find_many_sorted(
                doc! { "supplier_id": supplier_id.to_string() },
                doc! { "created_at": -1 },
                executor,
            )
            .await
    }
}

#[cfg(test)]
mod tests {
    use persistence_core::QueryFilter;

    use super::SupplierQualificationFilter;
    use crate::entity::supplier::QualificationStatus;

    #[test]
    fn qualification_filter_applies_type_and_status() {
        let filter = SupplierQualificationFilter {
            supplier_id: None,
            qualification_type: Some(crate::entity::supplier::QualificationType::FoodLicense),
            status: Some(QualificationStatus::Active),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_str("qualification_type").unwrap(), "food_license");
        assert_eq!(document.get_str("status").unwrap(), "active");
    }
}
