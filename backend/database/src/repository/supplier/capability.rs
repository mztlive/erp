use entities::ids::SupplierAccountId;
use entities::supplier::{CapabilityCode, CapabilityStatus, SupplierCapability};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};

use super::super::{Pagination, QueryFilter, Repository};
use super::{find_supplier_ids, SupplierRepository, SUPPLIER_CAPABILITIES};
use crate::executor::Executor;
use crate::Result;

/// 供应商能力列表筛选条件。
#[derive(Debug, Clone)]
pub struct SupplierCapabilityFilter {
    /// 供应商角色 ID；`None` 表示不筛选。
    pub supplier_id: Option<SupplierAccountId>,
    /// 能力代码；`None` 表示不筛选。
    pub capability_code: Option<CapabilityCode>,
    /// 启停状态；`None` 表示不筛选。
    pub status: Option<CapabilityStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（仓储白名单，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for SupplierCapabilityFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(supplier_id) = &self.supplier_id {
            filter.insert("supplier_id", supplier_id.to_string());
        }
        if let Some(capability_code) = self.capability_code {
            filter.insert("capability_code", capability_code.as_str());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for SupplierCapabilityFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, SupplierCapability> {
    /// 按「供应商 + 能力代码」查找能力（唯一性由
    /// `uk_supplier_capabilities_supplier_code` 保证）。
    ///
    /// # 参数
    /// * `supplier_id` - 供应商角色 ID
    /// * `capability_code` - 能力代码
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的未删除能力；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_supplier_and_code(
        &self,
        supplier_id: &SupplierAccountId,
        capability_code: CapabilityCode,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierCapability>> {
        self.find_one(
            doc! {
                "supplier_id": supplier_id.to_string(),
                "capability_code": capability_code.as_str(),
            },
            executor,
        )
        .await
    }

    /// 检索启用能力的到期预警列表（§6.2：`capability_code + status + valid_to`
    /// 用于选品和到期预警），按 `valid_to` 升序。
    ///
    /// # 参数
    /// * `capability_code` - 能力代码
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的启用能力（含已到期记录，由调用方按业务日期过滤）。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_active_for_expiry_warning(
        &self,
        capability_code: CapabilityCode,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierCapability>> {
        self.find_many_sorted(
            doc! {
                "capability_code": capability_code.as_str(),
                "status": CapabilityStatus::Active.as_str(),
            },
            doc! { "valid_to": 1 },
            executor,
        )
        .await
    }

    /// 查询命中任一当前有效能力的供应商角色 ID。
    ///
    /// # 参数
    /// * `capability_codes` - 供应能力代码；调用方保证非空
    /// * `as_of` - 当前业务日，格式为 `YYYY-MM-DD`
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回去重、稳定排序后的供应商角色 ID。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_supplier_ids_by_active_capability_codes(
        &self,
        capability_codes: &[CapabilityCode],
        as_of: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierAccountId>> {
        let codes: Vec<&str> = capability_codes.iter().map(CapabilityCode::as_str).collect();
        find_supplier_ids(
            self,
            doc! {
                "capability_code": { "$in": codes },
                "status": CapabilityStatus::Active.as_str(),
                "valid_from": { "$lte": as_of },
                "$or": [
                    { "valid_to": null },
                    { "valid_to": { "$gte": as_of } },
                ],
            },
            executor,
        )
        .await
    }
}

impl<'a> SupplierRepository<'a> {
    /// 查询命中任一当前有效能力的供应商角色 ID。
    ///
    /// # 参数
    /// * `capability_codes` - 供应能力代码
    /// * `as_of` - 当前业务日，格式为 `YYYY-MM-DD`
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回去重、稳定排序后的供应商角色 ID。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_supplier_ids_by_active_capability_codes(
        &self,
        capability_codes: &[CapabilityCode],
        as_of: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierAccountId>> {
        Repository::new(self.db, SUPPLIER_CAPABILITIES)
            .list_supplier_ids_by_active_capability_codes(capability_codes, as_of, executor)
            .await
    }

    /// 按创建时间升序读取供应商全部能力。
    ///
    /// # 参数
    /// * `supplier_id` - 供应商角色 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回该供应商的全部未删除能力。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_capabilities(
        &self,
        supplier_id: &SupplierAccountId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierCapability>> {
        Repository::new(self.db, SUPPLIER_CAPABILITIES)
            .find_many_sorted(
                doc! { "supplier_id": supplier_id.to_string() },
                doc! { "created_at": 1 },
                executor,
            )
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::{QueryFilter, SupplierCapabilityFilter};
    use entities::supplier::CapabilityStatus;

    #[test]
    fn capability_filter_applies_supplier_code_and_status() {
        let filter = SupplierCapabilityFilter {
            supplier_id: Some(entities::ids::SupplierAccountId::new("supplier-1")),
            capability_code: Some(entities::supplier::CapabilityCode::Physical),
            status: Some(CapabilityStatus::Active),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_str("supplier_id").unwrap(), "supplier-1");
        assert_eq!(document.get_str("capability_code").unwrap(), "physical");
        assert_eq!(document.get_str("status").unwrap(), "active");
    }
}
