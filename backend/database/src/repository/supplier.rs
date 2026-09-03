//! 域 D09 `supplier` 仓储：supplier_account、supplier_commercial_profile_revision、
//! supplier_capability(+_revision)、supplier_qualification(+_revision)、
//! supplier_qualification_capability、supplier_rating_revision（数据模型 §6.2）。
//!
//! 单一集合 CRUD 与乐观锁直接复用 [`Repository`] 基类（base.rs：
//! `update`/`soft_delete`/`restore` 比较 `id + version` 做 CAS）；本文件只补充
//! 域特有查询与跨集合多步骤写入入口。`supplier_account`/`supplier_capability`/
//! `supplier_qualification` 是稳定基础资料（可软删除，身份字段全局唯一）；
//! 四个 `*_revision` 集合是不可变修订（追加式、**不提供**软删除）；
//! `supplier_qualification_capability` 是资质 ↔ 能力的纯关联行。
//!
//! 集合名常量统一从 `SupplierExt` 关联常量导入（唯一权威来源）；筛选/行类型
//! 定义在本文件，经 `SupplierExt` 的关联类型对外暴露。

use std::collections::{HashMap, HashSet};

use entities::file_asset::FileAsset;
use entities::ids::{FileAssetId, PartyId, SupplierAccountId, SupplierQualificationId};
use entities::supplier::{
    CapabilityCode, CapabilityStatus, QualificationStatus, QualificationType, SupplierAccount,
    SupplierAccountStatus, SupplierCapability, SupplierCommercialProfileRevision, SupplierProfileCommand,
    SupplierQualification, SupplierQualificationCapability, SupplierRatingRevision,
};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use mongodb::Database;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use super::extensions::{FileAssetExt, PartyExt, SupplierExt};
use super::regex_filter::insert_literal_regex_filter;
use super::{PageResult, Pagination, QueryFilter, Repository};
use crate::executor::Executor;
use crate::{mongo_ops, Error, Result};

/// `supplier_account` 集合名（单一来源：`SupplierExt` 关联常量）。
const SUPPLIER_ACCOUNTS: &str = <mongodb::Database as SupplierExt>::SUPPLIER_ACCOUNTS;
/// `supplier_commercial_profile_revision` 集合名（单一来源：`SupplierExt` 关联常量）。
const SUPPLIER_COMMERCIAL_PROFILE_REVISIONS: &str =
    <mongodb::Database as SupplierExt>::SUPPLIER_COMMERCIAL_PROFILE_REVISIONS;
/// 供应商能力集合名。
const SUPPLIER_CAPABILITIES: &str = <mongodb::Database as SupplierExt>::SUPPLIER_CAPABILITIES;
/// 供应商能力修订集合名。
const SUPPLIER_CAPABILITY_REVISIONS: &str = <mongodb::Database as SupplierExt>::SUPPLIER_CAPABILITY_REVISIONS;
/// 供应商资质集合名。
const SUPPLIER_QUALIFICATIONS: &str = <mongodb::Database as SupplierExt>::SUPPLIER_QUALIFICATIONS;
/// 供应商资质修订集合名。
const SUPPLIER_QUALIFICATION_REVISIONS: &str =
    <mongodb::Database as SupplierExt>::SUPPLIER_QUALIFICATION_REVISIONS;
/// 资质适用能力关联集合名。
const SUPPLIER_QUALIFICATION_CAPABILITIES: &str =
    <mongodb::Database as SupplierExt>::SUPPLIER_QUALIFICATION_CAPABILITIES;
/// 供应商评级修订集合名。
const SUPPLIER_RATING_REVISIONS: &str = <mongodb::Database as SupplierExt>::SUPPLIER_RATING_REVISIONS;
/// 供应商资料幂等命令集合名。
const SUPPLIER_PROFILE_COMMANDS: &str = <mongodb::Database as SupplierExt>::SUPPLIER_PROFILE_COMMANDS;

/// 供应商角色列表投影行（列表接口只取必要字段，禁止返回整文档）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierAccountRow {
    /// 实体主键。
    pub id: String,
    /// 共用企业主体 ID。
    pub party_id: String,
    /// 供应商编号。
    pub supplier_no: String,
    /// 默认结算条件引用。
    pub default_payment_term_id: Option<String>,
    /// 当前商务结算版本 ID。
    pub current_commercial_profile_revision_id: Option<String>,
    /// 启停状态。
    pub status: SupplierAccountStatus,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 供应商编号窄投影行。
#[derive(Debug, Clone, Deserialize)]
struct SupplierNumberRow {
    /// 供应商稳定 ID。
    id: String,
    /// 供应商编号。
    supplier_no: String,
}

/// 供应商账号到企业主体的最小关联行。
#[derive(Debug, Deserialize, PartialEq, Eq)]
struct SupplierPartyRefRow {
    /// 供应商账号稳定 ID。
    id: String,
    /// 供应商账号所属企业主体 ID。
    party_id: String,
}

/// 供应商角色列表筛选条件。
#[derive(Debug, Clone)]
pub struct SupplierAccountFilter {
    /// 供应商编号模糊匹配（字面量正则，忽略大小写）；`None` 表示不筛选。
    pub keyword: Option<String>,
    /// 共用企业主体 ID（精确匹配）；`None` 表示不筛选。
    pub party_id: Option<PartyId>,
    /// 共用主体 ID 集合；用于将主体名称命中并入供应商编号搜索。
    pub party_ids: Option<Vec<PartyId>>,
    /// 启停状态；`None` 表示不筛选。
    pub status: Option<SupplierAccountStatus>,
    /// 必须命中的供应商角色 ID 集合；空集合表示无匹配结果。
    pub supplier_ids: Option<Vec<SupplierAccountId>>,
    /// 必须排除的供应商角色 ID 集合。
    pub excluded_supplier_ids: Option<Vec<SupplierAccountId>>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（仓储白名单，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for SupplierAccountFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(party_id) = &self.party_id {
            filter.insert("party_id", party_id.to_string());
        }
        match (self.keyword.as_deref(), self.party_ids.as_ref()) {
            (Some(keyword), Some(party_ids)) if !party_ids.is_empty() => {
                let ids: Vec<String> = party_ids.iter().map(ToString::to_string).collect();
                filter.insert(
                    "$or",
                    vec![
                        doc! { "supplier_no": { "$regex": regex::escape(keyword), "$options": "i" } },
                        doc! { "party_id": { "$in": ids } },
                    ],
                );
            }
            (Some(keyword), _) => {
                insert_literal_regex_filter(&mut filter, "supplier_no", Some(keyword));
            }
            (None, _) => {}
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        insert_supplier_id_constraints(
            &mut filter,
            self.supplier_ids.as_deref(),
            self.excluded_supplier_ids.as_deref(),
        );
        filter
    }
}

/// 将供应商候选与排除集合写入账户列表查询条件。
///
/// # 参数
/// * `filter` - 待补充的 MongoDB 查询条件
/// * `supplier_ids` - 必须命中的供应商候选集合；`None` 表示不限制
/// * `excluded_supplier_ids` - 必须排除的供应商集合；`None` 表示不排除
///
/// # 返回
/// 无返回值；查询条件在原地更新。
fn insert_supplier_id_constraints(
    filter: &mut Document,
    supplier_ids: Option<&[SupplierAccountId]>,
    excluded_supplier_ids: Option<&[SupplierAccountId]>,
) {
    let Some(supplier_ids) = supplier_ids else {
        if let Some(excluded_supplier_ids) = excluded_supplier_ids {
            filter.insert("id", doc! { "$nin": supplier_id_strings(excluded_supplier_ids) });
        }
        return;
    };
    let mut id_filter = doc! { "$in": supplier_id_strings(supplier_ids) };
    if let Some(excluded_supplier_ids) = excluded_supplier_ids {
        id_filter.insert("$nin", supplier_id_strings(excluded_supplier_ids));
    }
    filter.insert("id", id_filter);
}

/// 转换供应商角色 ID，供 MongoDB 集合条件使用。
///
/// # 参数
/// * `ids` - 强类型供应商角色 ID 集合
///
/// # 返回
/// 返回保持输入顺序的字符串 ID 集合。
fn supplier_id_strings(ids: &[SupplierAccountId]) -> Vec<String> {
    ids.iter().map(ToString::to_string).collect()
}

impl Pagination for SupplierAccountFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, SupplierAccount> {
    /// 批量读取未删除供应商的稳定 ID 与供应商编号。
    ///
    /// # 参数
    /// * `supplier_ids` - 供应商 ID 集合；空集合不访问数据库
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回实际存在的供应商编号映射；停用但未删除供应商仍保留编号，软删除或缺失不补行。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn supplier_numbers_by_ids(
        &self,
        supplier_ids: &[SupplierAccountId],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        if supplier_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let ids = supplier_ids.iter().map(ToString::to_string).collect::<Vec<_>>();
        let collection = self.collection().clone_with_type::<SupplierNumberRow>();
        let rows = mongo_ops::find_many(
            &collection,
            doc! {
                "id": { "$in": ids },
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            FindOptions::builder()
                .projection(doc! { "id": 1, "supplier_no": 1 })
                .build(),
            executor,
        )
        .await?;
        Ok(rows.into_iter().map(|row| (row.id, row.supplier_no)).collect())
    }

    /// 按供应商角色 ID 集合批量读取活跃账户。
    ///
    /// # 参数
    /// * `supplier_ids` - 供应商角色 ID 集合；空集合直接返回空结果
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配且未删除的供应商角色；返回顺序不承诺与输入一致。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_accounts_by_ids(
        &self,
        supplier_ids: &[SupplierAccountId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierAccount>> {
        if supplier_ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids = supplier_ids.iter().map(ToString::to_string).collect::<Vec<_>>();
        self.find_many(doc! { "id": { "$in": ids } }, executor).await
    }

    /// 分页检索供应商角色列表（投影查询）。
    ///
    /// 只返回 [`SupplierAccountRow`] 所需的列表字段，不加载整文档；排序字段
    /// 经仓储白名单校验（`created_at`/`supplier_no`/`status`），非法字段回落
    /// 默认 `created_at`。
    ///
    /// # 参数
    /// * `filter` - 筛选与分页条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页投影行与满足筛选条件的总数。
    ///
    /// # 错误
    /// 当 MongoDB 查询、游标读取或计数失败时返回错误。
    pub async fn search_supplier_accounts(
        &self,
        filter: &SupplierAccountFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SupplierAccountRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
                &["created_at", "supplier_no", "status"],
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(supplier_account_projection())
            .build();
        let collection = self.collection().clone_with_type::<SupplierAccountRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 按共用企业主体查找供应商角色（一个主体至多一个供应商角色，由
    /// `uk_supplier_accounts_party` 保证）。
    ///
    /// # 参数
    /// * `party_id` - 共用企业主体 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的未删除供应商角色；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_party(
        &self,
        party_id: &PartyId,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierAccount>> {
        self.find_one(doc! { "party_id": party_id.to_string() }, executor)
            .await
    }
}

/// 商务结算版本列表筛选条件。
#[derive(Debug, Clone)]
pub struct SupplierCommercialProfileFilter {
    /// 供应商角色 ID；`None` 表示不筛选。
    pub supplier_id: Option<SupplierAccountId>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（仓储白名单，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for SupplierCommercialProfileFilter {
    /// 转换为 MongoDB 查询条件（修订集合无软删除，过滤条件为空时仍显式
    /// 追加未删除过滤，与基类语义保持一致）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(supplier_id) = &self.supplier_id {
            filter.insert("supplier_id", supplier_id.to_string());
        }
        filter
    }
}

impl Pagination for SupplierCommercialProfileFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, SupplierCommercialProfileRevision> {
    /// 检索某供应商的商务版本历史（按 `revision_no` 升序，§6.2 历史查询）。
    ///
    /// # 参数
    /// * `supplier_id` - 供应商角色 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回该供应商的全部商务版本（修订集合追加式写入，无软删除）。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_revision_history(
        &self,
        supplier_id: &SupplierAccountId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierCommercialProfileRevision>> {
        self.find_many_sorted(
            doc! { "supplier_id": supplier_id.to_string() },
            doc! { "revision_no": 1 },
            executor,
        )
        .await
    }
}

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

impl<'a> Repository<'a, SupplierQualification> {
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
        find_supplier_ids(self, qualification_type_filter(qualification_types), executor).await
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
        let mut filter = qualification_type_filter(qualification_types);
        filter.insert("status", QualificationStatus::Active.as_str());
        filter.insert("valid_from", doc! { "$lte": as_of });
        filter.insert(
            "$or",
            vec![doc! { "valid_to": null }, doc! { "valid_to": { "$gte": as_of } }],
        );
        find_supplier_ids(self, filter, executor).await
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
        let mut filter = qualification_type_filter(qualification_types);
        filter.insert("status", QualificationStatus::Active.as_str());
        filter.insert("valid_from", doc! { "$lte": as_of });
        filter.insert("valid_to", doc! { "$gte": as_of, "$lte": expires_by });
        find_supplier_ids(self, filter, executor).await
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
        find_supplier_ids(self, filter, executor).await
    }
}

impl<'a> Repository<'a, SupplierQualificationCapability> {
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
        self.find_many(doc! { "qualification_id": { "$in": ids } }, executor)
            .await
    }
}

impl<'a> Repository<'a, SupplierProfileCommand> {
    /// 按客户端幂等键读取已成功命令结果。
    ///
    /// # Errors
    /// MongoDB 查询失败时返回错误。
    pub async fn find_by_idempotency_key(
        &self,
        idempotency_key: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierProfileCommand>> {
        self.find_one(doc! { "idempotency_key": idempotency_key }, executor)
            .await
    }
}

/// 仅承载筛选阶段所需的供应商角色 ID。
#[derive(Debug, Deserialize)]
struct SupplierIdRow {
    supplier_id: SupplierAccountId,
}

/// 按供应商子集合条件读取去重后的供应商角色 ID。
///
/// # 参数
/// * `repository` - 供应商能力或资质集合仓储
/// * `filter` - 已按业务语义构造的集合查询条件
/// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
///
/// # 返回
/// 返回按字符串 ID 稳定排序并去重的供应商角色 ID。
///
/// # 错误
/// 当 MongoDB 查询或反序列化失败时返回错误。
async fn find_supplier_ids<T>(
    repository: &Repository<'_, T>,
    mut filter: Document,
    executor: &mut dyn Executor,
) -> Result<Vec<SupplierAccountId>>
where
    T: Serialize + DeserializeOwned + Send + Sync,
{
    filter.insert("deleted_at", NOT_DELETED_TIMESTAMP_BSON);
    let options = FindOptions::builder()
        .projection(doc! { "supplier_id": 1, "_id": 0 })
        .build();
    let collection = repository.collection().clone_with_type::<SupplierIdRow>();
    let rows = mongo_ops::find_many(&collection, filter, options, executor).await?;
    let mut ids: Vec<SupplierAccountId> = rows.into_iter().map(|row| row.supplier_id).collect();
    ids.sort_by_key(ToString::to_string);
    ids.dedup();
    Ok(ids)
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
    let types: Vec<&str> = qualification_types
        .iter()
        .map(QualificationType::as_str)
        .collect();
    doc! { "qualification_type": { "$in": types } }
}

/// 修订号最小投影行。
#[derive(Debug, Deserialize)]
struct RevisionNoRow {
    revision_no: u32,
}

/// 查询当前最大修订号并返回下一号。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库
/// * `collection_name` - 追加式修订集合名称
/// * `filter` - 修订序列的稳定业务身份条件
/// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
///
/// # 返回
/// 无历史时返回 `1`，否则返回当前最大修订号加一。
///
/// # 错误
/// 当 MongoDB 查询失败、反序列化失败或修订号达到 `u32::MAX` 时返回错误。
async fn next_revision_no(
    db: &Database,
    collection_name: &str,
    mut filter: Document,
    executor: &mut dyn Executor,
) -> Result<u32> {
    filter.insert("deleted_at", NOT_DELETED_TIMESTAMP_BSON);
    let options = FindOptions::builder()
        .sort(doc! { "revision_no": -1 })
        .limit(1)
        .projection(doc! { "revision_no": 1, "_id": 0 })
        .build();
    let rows = mongo_ops::find_many(
        &db.collection::<RevisionNoRow>(collection_name),
        filter,
        options,
        executor,
    )
    .await?;
    rows.into_iter()
        .next()
        .map(|row| row.revision_no)
        .unwrap_or(0)
        .checked_add(1)
        .ok_or(Error::EntityMetadataOutOfRange("supplier revision number"))
}

/// D09 域专用仓储：语义化聚合读取与跨集合事务写入。
///
/// 单一集合 CRUD 使用 [`Repository`] 基类；列表筛选、批量资料、当前修订号
/// 和跨集合原子写入由本类型收敛，通过 `SupplierExt::supplier()` 访问。
pub struct SupplierRepository<'a> {
    db: &'a Database,
}

impl<'a> SupplierRepository<'a> {
    /// 创建域专用仓储。
    ///
    /// # 参数
    /// * `db` - 目标 MongoDB 数据库
    ///
    /// # 返回
    /// 返回仓储实例。
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// 按稳定 ID 读取未删除供应商角色账号。
    ///
    /// # 参数
    /// * `supplier_id` - 供应商角色 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配供应商角色；不存在时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
    pub async fn account(
        &self,
        supplier_id: &SupplierAccountId,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierAccount>> {
        Repository::new(self.db, SUPPLIER_ACCOUNTS)
            .find_by_id(supplier_id.as_ref(), executor)
            .await
    }

    /// 按供应商账号 ID 批量读取当前主体修订的法定名称。
    ///
    /// 查询仅返回未删除供应商账号、未删除 Party 及其未删除当前修订形成的
    /// `账号 ID -> 法定名称` 投影。缺失任一关联或缺少当前修订指针时不生成键；
    /// 法定名称按持久化原值返回，不在仓储层执行空白回退等业务决策。
    ///
    /// # 参数
    /// * `supplier_ids` - 供应商账号 ID；允许重复，空集合直接返回空映射
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回供应商账号 ID 到当前法定名称的映射。
    ///
    /// # 错误
    /// 当任一批量查询或投影反序列化失败时返回错误。
    ///
    /// # 约束
    /// 只查询本域 `supplier_accounts` 集合，主体法定名称经主体域属主访问器组装。
    pub async fn current_legal_names_by_account_ids(
        &self,
        supplier_ids: &[SupplierAccountId],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        let supplier_ids = unique_strings(supplier_ids.iter().map(ToString::to_string));
        if supplier_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let suppliers = self.supplier_party_refs(&supplier_ids, executor).await?;
        let party_ids = unique_strings(suppliers.iter().map(|row| row.party_id.clone()))
            .into_iter()
            .map(PartyId::new)
            .collect::<Vec<_>>();
        if party_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let party_names = self
            .db
            .party()
            .current_legal_names_by_party_ids(&party_ids, executor)
            .await?;
        Ok(suppliers
            .into_iter()
            .filter_map(|row| {
                party_names
                    .get(&row.party_id)
                    .map(|legal_name| (row.id, legal_name.clone()))
            })
            .collect())
    }

    /// 批量读取未删除供应商账号到企业主体的关联。
    ///
    /// # 参数
    /// * `supplier_ids` - 已去重的供应商账号 ID
    /// * `executor` - 调用方提供的数据访问执行器
    ///
    /// # 返回
    /// 返回全部命中的账号与主体最小关联行。
    ///
    /// # 错误
    /// 当 MongoDB 查询或投影反序列化失败时返回错误。
    async fn supplier_party_refs(
        &self,
        supplier_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierPartyRefRow>> {
        mongo_ops::find_many(
            &self.db.collection::<SupplierPartyRefRow>(SUPPLIER_ACCOUNTS),
            active_ids_filter(supplier_ids),
            FindOptions::builder()
                .projection(supplier_party_ref_projection())
                .build(),
            executor,
        )
        .await
    }

    /// 按客户端幂等键读取供应商资料命令。
    ///
    /// # 参数
    /// * `idempotency_key` - 客户端幂等键
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回已成功命令；不存在时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
    pub async fn profile_command(
        &self,
        idempotency_key: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierProfileCommand>> {
        Repository::<SupplierProfileCommand>::new(self.db, SUPPLIER_PROFILE_COMMANDS)
            .find_by_idempotency_key(idempotency_key, executor)
            .await
    }

    /// 按文件 ID 读取供应商资质附件（委托文件资产属主仓储，不直查文件集合）。
    ///
    /// 供应商域仅编排调用 [`FileAssetExt::file_assets`] 属主访问器上的通用
    /// [`Repository::find_by_id`]，不构造 `file_assets` 集合句柄、不复刻属主 CRUD。
    ///
    /// # 参数
    /// * `attachment_id` - 资质附件文件 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回未删除文件资产；不存在时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
    ///
    /// # 约束
    /// 只经文件资产属主访问器查询 `file_assets` 集合，不在供应商仓储内直查外域集合。
    pub async fn qualification_attachment(
        &self,
        attachment_id: &FileAssetId,
        executor: &mut dyn Executor,
    ) -> Result<Option<FileAsset>> {
        self.db
            .file_assets()
            .find_by_id(attachment_id.as_ref(), executor)
            .await
    }

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
        Repository::new(self.db, SUPPLIER_QUALIFICATIONS)
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
        Repository::new(self.db, SUPPLIER_QUALIFICATIONS)
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
        Repository::new(self.db, SUPPLIER_QUALIFICATIONS)
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
        Repository::new(self.db, SUPPLIER_QUALIFICATIONS)
            .list_supplier_ids_by_expired_qualifications(qualification_types, as_of, executor)
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
        Repository::new(self.db, SUPPLIER_QUALIFICATIONS)
            .find_many_sorted(
                doc! { "supplier_id": supplier_id.to_string() },
                doc! { "created_at": -1 },
                executor,
            )
            .await
    }

    /// 按修订号倒序读取供应商商务资料历史。
    ///
    /// # 参数
    /// * `supplier_id` - 供应商角色 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回最新版本优先的商务资料修订。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_commercial_profiles_latest_first(
        &self,
        supplier_id: &SupplierAccountId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierCommercialProfileRevision>> {
        Repository::new(self.db, SUPPLIER_COMMERCIAL_PROFILE_REVISIONS)
            .find_many_sorted(
                doc! { "supplier_id": supplier_id.to_string() },
                doc! { "revision_no": -1 },
                executor,
            )
            .await
    }

    /// 按修订 ID 集合批量读取商务资料版本。
    ///
    /// # 参数
    /// * `revision_ids` - 商务资料修订 ID 集合；为空时直接返回空集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配的商务资料版本。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_commercial_profiles_by_ids(
        &self,
        revision_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierCommercialProfileRevision>> {
        if revision_ids.is_empty() {
            return Ok(Vec::new());
        }
        Repository::new(self.db, SUPPLIER_COMMERCIAL_PROFILE_REVISIONS)
            .find_many(doc! { "id": { "$in": revision_ids } }, executor)
            .await
    }

    /// 按修订号倒序读取供应商评级历史。
    ///
    /// # 参数
    /// * `supplier_id` - 供应商角色 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回最新评级优先的修订历史。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_ratings_latest_first(
        &self,
        supplier_id: &SupplierAccountId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierRatingRevision>> {
        Repository::new(self.db, SUPPLIER_RATING_REVISIONS)
            .find_many_sorted(
                doc! { "supplier_id": supplier_id.to_string() },
                doc! { "revision_no": -1 },
                executor,
            )
            .await
    }

    /// 按修订号升序读取供应商评级历史。
    ///
    /// # 参数
    /// * `supplier_id` - 供应商角色 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回最早评级优先的修订历史。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_rating_history(
        &self,
        supplier_id: &SupplierAccountId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierRatingRevision>> {
        Repository::new(self.db, SUPPLIER_RATING_REVISIONS)
            .find_many_sorted(
                doc! { "supplier_id": supplier_id.to_string() },
                doc! { "revision_no": 1 },
                executor,
            )
            .await
    }

    /// 返回下一商务资料修订序号。
    ///
    /// # 参数
    /// * `supplier_id` - 供应商角色 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 无历史时返回 `1`，否则返回当前最大修订号加一。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
    pub async fn next_commercial_profile_revision_no(
        &self,
        supplier_id: &SupplierAccountId,
        executor: &mut dyn Executor,
    ) -> Result<u32> {
        next_revision_no(
            self.db,
            SUPPLIER_COMMERCIAL_PROFILE_REVISIONS,
            doc! { "supplier_id": supplier_id.to_string() },
            executor,
        )
        .await
    }

    /// 返回下一能力修订序号。
    ///
    /// # 参数
    /// * `supplier_id` - 供应商角色 ID
    /// * `capability_code` - 稳定能力代码
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 无历史时返回 `1`，否则返回当前最大修订号加一。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
    pub async fn next_capability_revision_no(
        &self,
        supplier_id: &SupplierAccountId,
        capability_code: CapabilityCode,
        executor: &mut dyn Executor,
    ) -> Result<u32> {
        next_revision_no(
            self.db,
            SUPPLIER_CAPABILITY_REVISIONS,
            doc! {
                "supplier_id": supplier_id.to_string(),
                "capability_code": capability_code.as_str(),
            },
            executor,
        )
        .await
    }

    /// 返回下一资质修订序号。
    ///
    /// # 参数
    /// * `supplier_id` - 供应商角色 ID
    /// * `qualification_type` - 稳定资质类型
    /// * `certificate_no` - 稳定证书编号
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 无历史时返回 `1`，否则返回当前最大修订号加一。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
    pub async fn next_qualification_revision_no(
        &self,
        supplier_id: &SupplierAccountId,
        qualification_type: QualificationType,
        certificate_no: &str,
        executor: &mut dyn Executor,
    ) -> Result<u32> {
        next_revision_no(
            self.db,
            SUPPLIER_QUALIFICATION_REVISIONS,
            doc! {
                "supplier_id": supplier_id.to_string(),
                "qualification_type": qualification_type.as_str(),
                "certificate_no": certificate_no,
            },
            executor,
        )
        .await
    }

    /// 创建供应商角色并写入首个商务结算版本（跨集合多步骤写入）。
    ///
    /// 依次写入 `supplier_commercial_profile_revisions` 与 `supplier_accounts`，
    /// 保证「商务版本 + 供应商角色」原子可见（数据模型 §6.2：供应商角色
    /// 携带 `current_commercial_profile_revision_id` 指向当前版本）。
    /// **必须收到事务执行器**：本方法不构成原子边界，传入 `NoTransaction`
    /// 时两笔写入各自自动提交，中途失败会留下只有版本没有供应商角色的
    /// 半成品；Service 必须通过 `database::Transactional::with_transaction`
    /// 传入事务会话。
    ///
    /// # 参数
    /// * `supplier` - 待写入的供应商角色（`current_commercial_profile_revision_id`
    ///   必须已指向 `revision`）
    /// * `revision` - 待写入的首个商务结算版本
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当供应商编号或主体归属违反唯一索引（透出 [`crate::Error::DuplicateKey`]）
    /// 或 MongoDB 写入失败时返回错误。
    pub async fn create_supplier_with_initial_profile(
        &self,
        supplier: &SupplierAccount,
        revision: &SupplierCommercialProfileRevision,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self
                .db
                .collection::<SupplierCommercialProfileRevision>(SUPPLIER_COMMERCIAL_PROFILE_REVISIONS),
            revision,
            executor,
        )
        .await?;
        mongo_ops::insert_one(
            &self.db.collection::<SupplierAccount>(SUPPLIER_ACCOUNTS),
            supplier,
            executor,
        )
        .await
    }

    /// 在同一事务内整体替换一份资质的适用能力集合。
    ///
    /// 调用方必须先校验能力均属于同一供应商，并传入事务执行器。
    ///
    /// # Errors
    /// 删除旧关联或写入新关联失败时返回错误。
    pub async fn replace_qualification_capabilities(
        &self,
        qualification_id: &SupplierQualificationId,
        links: Vec<SupplierQualificationCapability>,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::delete_many(
            &self
                .db
                .collection::<SupplierQualificationCapability>(SUPPLIER_QUALIFICATION_CAPABILITIES),
            doc! { "qualification_id": qualification_id.to_string() },
            executor,
        )
        .await?;
        mongo_ops::insert_many(
            &self
                .db
                .collection::<SupplierQualificationCapability>(SUPPLIER_QUALIFICATION_CAPABILITIES),
            links,
            executor,
        )
        .await
    }
}

/// 构建排序文档（仓储白名单）。
///
/// `sort_by` 不在 `allowed` 白名单内时回落默认 `created_at`，禁止透传任意
/// 字段名（P2 §2.3）。
///
/// # 参数
/// * `sort_by` - 排序字段；`None` 或不在白名单时默认 `created_at`
/// * `sort_ascending` - 升序为 `true`，降序为 `false`
/// * `allowed` - 允许的排序字段白名单
///
/// # 返回
/// 返回排序条件文档。
fn sort_doc(sort_by: Option<&str>, sort_ascending: bool, allowed: &[&str]) -> Document {
    let direction = if sort_ascending { 1 } else { -1 };
    let field = sort_by
        .filter(|candidate| allowed.contains(candidate))
        .unwrap_or("created_at");
    doc! { field: direction }
}

/// 供应商角色列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn supplier_account_projection() -> Document {
    doc! {
        "id": 1,
        "party_id": 1,
        "supplier_no": 1,
        "default_payment_term_id": 1,
        "current_commercial_profile_revision_id": 1,
        "status": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 按首次出现顺序去重字符串集合。
///
/// # 参数
/// * `values` - 允许包含重复值的字符串迭代器
///
/// # 返回
/// 返回保留首次出现顺序的唯一字符串集合。
fn unique_strings(values: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut unique = Vec::new();
    for value in values {
        if seen.insert(value.clone()) {
            unique.push(value);
        }
    }
    unique
}

/// 构建未删除稳定对象的批量 ID 查询条件。
///
/// # 参数
/// * `ids` - 已去重的稳定对象 ID
///
/// # 返回
/// 返回同时限定 ID 集合与软删除标记的查询文档。
fn active_ids_filter(ids: &[String]) -> Document {
    doc! {
        "id": { "$in": ids },
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
    }
}

/// 返回供应商账号到主体关联的最小字段投影。
///
/// # 返回
/// 返回排除 MongoDB `_id` 且仅保留账号与主体 ID 的投影文档。
fn supplier_party_ref_projection() -> Document {
    doc! { "_id": 0, "id": 1, "party_id": 1 }
}

#[cfg(test)]
mod tests {
    use super::{
        sort_doc, QueryFilter, SupplierAccountFilter, SupplierCapabilityFilter, SupplierQualificationFilter,
    };
    use entities::supplier::{CapabilityStatus, QualificationStatus, SupplierAccountStatus};
    use mongodb::bson::doc;

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

    #[test]
    fn qualification_filter_applies_type_and_status() {
        let filter = SupplierQualificationFilter {
            supplier_id: None,
            qualification_type: Some(entities::supplier::QualificationType::FoodLicense),
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

    #[test]
    fn account_filter_applies_candidate_and_excluded_supplier_ids() {
        let filter = SupplierAccountFilter {
            keyword: None,
            party_id: None,
            party_ids: None,
            status: Some(SupplierAccountStatus::Active),
            supplier_ids: Some(vec![entities::ids::SupplierAccountId::new("supplier-1")]),
            excluded_supplier_ids: Some(vec![entities::ids::SupplierAccountId::new("supplier-2")]),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        let ids = document.get_document("id").unwrap();
        assert_eq!(ids.get_array("$in").unwrap().len(), 1);
        assert_eq!(ids.get_array("$nin").unwrap().len(), 1);
        assert_eq!(document.get_str("status").unwrap(), "active");
    }

    #[test]
    fn sort_doc_falls_back_to_created_at_when_field_is_not_whitelisted() {
        assert_eq!(
            sort_doc(Some("revised_at"), false, &["created_at", "supplier_no"]),
            doc! { "created_at": -1 }
        );
        assert_eq!(
            sort_doc(Some("supplier_no"), true, &["created_at", "supplier_no"]),
            doc! { "supplier_no": 1 }
        );
    }
}
