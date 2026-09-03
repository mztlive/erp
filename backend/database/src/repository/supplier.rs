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
use entities::party::{Party, PartyAddress, PartyBankAccount, PartyContact, PartyRevision, PartyTaxProfile};
use entities::supplier::{
    CapabilityCode, CapabilityStatus, QualificationStatus, QualificationType, SupplierAccount,
    SupplierAccountStatus, SupplierCapability, SupplierCommercialProfileRevision, SupplierProfileCommand,
    SupplierQualification, SupplierQualificationCapability, SupplierRatingRevision,
};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use futures_util::TryStreamExt;
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

/// 供应商账号业务主键重复审计行。
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct SupplierAccountIdDuplicate {
    /// 重复的业务主键。
    pub id: String,
    /// 该主键在集合中的出现次数。
    pub count: i64,
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

    /// 审计 `supplier_accounts` 集合中重复的业务主键 `id`。
    ///
    /// 部署 `uk_supplier_accounts_id` 唯一索引前的门禁查询：按 `id` 分组统计
    /// 出现次数并返回全部出现超过一次的主键，供部署在建索引前失败关闭。
    /// 审计覆盖全部文档（含已软删除）：身份唯一索引是全局的，软删除后仍保留
    /// 身份，删除态重复同样会阻断迁移。
    ///
    /// # 参数
    /// * `executor` - 数据访问执行器，由调用方决定是否位于事务中；本方法只读，
    ///   不开启或提交事务
    ///
    /// # 返回
    /// 按 `id` 字典序排列的重复主键及出现次数；无重复时返回空集合。
    ///
    /// # 错误
    /// 当 MongoDB 聚合或游标读取失败时返回错误。
    ///
    /// # 约束
    /// 只查询本域 `supplier_accounts` 集合；仅返回审计事实，不执行清理或建索引。
    pub async fn duplicate_supplier_account_ids(
        &self,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierAccountIdDuplicate>> {
        aggregate_id_duplicates(&self.db.collection::<Document>(SUPPLIER_ACCOUNTS), executor).await
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

    /// 分页前生效的供应商列表事实束查询（`PROC-R03`）。
    ///
    /// 将关键词主体命中、能力与资质候选约束、分页投影查询与列表水合所需的
    /// 主体/修订/商务资料批量读取收敛为固定上界的仓储调用；关键词、能力与
    /// 资质健康状态均在分页计数前生效，保证总数与分页内容一致。
    ///
    /// # 参数
    /// * `input` - 已规范化的列表业务筛选、分页排序与业务日
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页投影行、总数及水合所需的批量事实。
    ///
    /// # 错误
    /// 业务日期非法或任一批量查询失败时返回错误。
    ///
    /// # 约束
    /// 不开启或提交事务；查询次数与页大小无关且有固定上界；软删除由基查询
    /// 过滤，排序字段经仓储白名单校验。
    pub async fn load_supplier_list_bundle(
        &self,
        input: &SupplierListSearchInput,
        executor: &mut dyn Executor,
    ) -> Result<SupplierListBundle> {
        let party_ids = match input.keyword.as_deref() {
            Some(keyword) => Some(
                self.db
                    .party()
                    .matching_current_party_ids_by_name(keyword, executor)
                    .await?,
            ),
            None => None,
        };
        let (supplier_ids, excluded_supplier_ids) = self
            .list_filter_id_constraints(
                &input.capability_codes,
                &input.qualification_types,
                input.qualification_health,
                &input.as_of,
                executor,
            )
            .await?;
        let filter = SupplierAccountFilter {
            keyword: input.keyword.clone(),
            party_id: input.party_id.clone(),
            party_ids,
            status: input.status,
            supplier_ids,
            excluded_supplier_ids,
            page: input.page,
            page_size: input.page_size,
            sort_by: input.sort_by.clone(),
            sort_ascending: input.sort_ascending,
        };
        let page = Repository::new(self.db, SUPPLIER_ACCOUNTS)
            .search_supplier_accounts(&filter, executor)
            .await?;
        let party_ids: Vec<PartyId> = page.items.iter().map(|row| PartyId::new(&row.party_id)).collect();
        let (parties, revisions) = self
            .db
            .party()
            .list_with_current_revisions(&party_ids, executor)
            .await?;
        let profile_ids: Vec<String> = page
            .items
            .iter()
            .filter_map(|row| row.current_commercial_profile_revision_id.clone())
            .collect();
        let profiles = self
            .list_commercial_profiles_by_ids(&profile_ids, executor)
            .await?;
        Ok(SupplierListBundle {
            page,
            parties,
            revisions,
            profiles,
        })
    }

    /// 组装能力与资质条件对应的角色 ID 约束。
    ///
    /// # 参数
    /// * `capability_codes` - 命中的能力代码；空集合表示不限制
    /// * `qualification_types` - 命中的资质类型；空集合表示不限制类型
    /// * `health` - 资质健康状态；`None` 表示不按健康状态过滤
    /// * `as_of` - 当前业务日字符串
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回必须命中的候选 ID 与必须排除的 ID 集合。
    ///
    /// # 错误
    /// 业务日期非法或任一仓储查询失败时返回错误。
    ///
    /// # 约束
    /// 纯仓储侧候选计算，不返回 Service DTO；交集语义与旧 Service 一致。
    async fn list_filter_id_constraints(
        &self,
        capability_codes: &[CapabilityCode],
        qualification_types: &[QualificationType],
        health: Option<SupplierQualificationHealthFilter>,
        as_of: &str,
        executor: &mut dyn Executor,
    ) -> Result<(Option<Vec<SupplierAccountId>>, Option<Vec<SupplierAccountId>>)> {
        let capability_ids = if capability_codes.is_empty() {
            None
        } else {
            Some(
                self.list_supplier_ids_by_active_capability_codes(capability_codes, as_of, executor)
                    .await?,
            )
        };
        let (qualification_ids, excluded_qualification_ids) = self
            .list_filter_qualification_constraints(qualification_types, health, as_of, executor)
            .await?;
        Ok((
            intersect_supplier_ids(capability_ids, qualification_ids),
            excluded_qualification_ids,
        ))
    }

    /// 查询资质类型和健康状态对应的供应商角色 ID 约束。
    ///
    /// # 参数
    /// * `qualification_types` - 资质类型；空集合表示不限制类型
    /// * `health` - 资质健康状态；`None` 表示仅按类型命中
    /// * `as_of` - 当前业务日字符串
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回应命中与应排除的供应商 ID 集合；未筛选资质时均为 `None`。
    ///
    /// # 错误
    /// 到期窗口计算或任一仓储查询失败时返回错误。
    ///
    /// # 约束
    /// `Expiring30` 窗口为起始日后第三十个自然日；`NotRegistered` 仅返回排除集合。
    async fn list_filter_qualification_constraints(
        &self,
        qualification_types: &[QualificationType],
        health: Option<SupplierQualificationHealthFilter>,
        as_of: &str,
        executor: &mut dyn Executor,
    ) -> Result<(Option<Vec<SupplierAccountId>>, Option<Vec<SupplierAccountId>>)> {
        match qualification_constraint_kind(qualification_types, health) {
            QualificationConstraintKind::Unconstrained => Ok((None, None)),
            QualificationConstraintKind::Excluded => Ok((
                None,
                Some(
                    self.list_supplier_ids_by_qualification_types(qualification_types, executor)
                        .await?,
                ),
            )),
            QualificationConstraintKind::Included => match health {
                None => Ok((
                    Some(
                        self.list_supplier_ids_by_qualification_types(qualification_types, executor)
                            .await?,
                    ),
                    None,
                )),
                Some(SupplierQualificationHealthFilter::Valid) => Ok((
                    Some(
                        self.list_supplier_ids_by_valid_qualifications(qualification_types, as_of, executor)
                            .await?,
                    ),
                    None,
                )),
                Some(SupplierQualificationHealthFilter::Expiring30) => {
                    let expires_by = qualification_expiry_cutoff(as_of)?;
                    Ok((
                        Some(
                            self.list_supplier_ids_by_expiring_qualifications(
                                qualification_types,
                                as_of,
                                &expires_by,
                                executor,
                            )
                            .await?,
                        ),
                        None,
                    ))
                }
                Some(SupplierQualificationHealthFilter::Expired) => Ok((
                    Some(
                        self.list_supplier_ids_by_expired_qualifications(
                            qualification_types,
                            as_of,
                            executor,
                        )
                        .await?,
                    ),
                    None,
                )),
                Some(SupplierQualificationHealthFilter::NotRegistered) => Ok((
                    None,
                    Some(
                        self.list_supplier_ids_by_qualification_types(qualification_types, executor)
                            .await?,
                    ),
                )),
            },
        }
    }

    /// 批量加载供应商详情所需的全部事实（`PROC-R04`）。
    ///
    /// 一次调用返回供应商、主体当前指针及联系人、地址、税务、银行、能力、
    /// 资质关联、评级与商务版本历史；资质与能力关联一次批量读取，查询次数
    /// 有固定上界，不随资质或能力数量增长。
    ///
    /// # 参数
    /// * `supplier_id` - 供应商角色 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 供应商不存在时返回 `None`；存在时返回全部事实束，主体缺失、当前指针
    /// 缺失或历史集合为空时以 `None`/空集合表达，由 Service 映射为明确语义。
    ///
    /// # 错误
    /// 任一批量仓储查询失败时返回错误。
    ///
    /// # 约束
    /// 不开启或提交事务；不记录敏感明文日志；不返回 Service DTO 或 View。
    pub async fn load_supplier_detail_bundle(
        &self,
        supplier_id: &SupplierAccountId,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierDetailBundle>> {
        let Some(supplier) = self.account(supplier_id, executor).await? else {
            return Ok(None);
        };
        let party_bundle = self
            .db
            .party()
            .find_with_current_revision(&supplier.party_id, executor)
            .await?;
        let (party, party_revision) = match party_bundle {
            Some((party, revision)) => (Some(party), revision),
            None => (None, None),
        };
        let contacts = self
            .db
            .party_contacts()
            .list_by_party(&supplier.party_id, executor)
            .await?;
        let addresses = self
            .db
            .party_addresses()
            .list_by_party(&supplier.party_id, executor)
            .await?;
        let tax_profiles = self
            .db
            .party_tax_profiles()
            .list_by_party(&supplier.party_id, executor)
            .await?;
        let bank_accounts = self
            .db
            .party_bank_accounts()
            .list_by_party(&supplier.party_id, executor)
            .await?;
        let capabilities = self.list_capabilities(supplier_id, executor).await?;
        let qualifications = self.list_qualifications(supplier_id, executor).await?;
        let qualification_ids: Vec<entities::ids::SupplierQualificationId> = qualifications
            .iter()
            .map(|item| entities::ids::SupplierQualificationId::new(&item.base.id))
            .collect();
        let qualification_links =
            Repository::<SupplierQualificationCapability>::new(self.db, SUPPLIER_QUALIFICATION_CAPABILITIES)
                .list_by_qualification_ids(&qualification_ids, executor)
                .await?;
        let ratings = self.list_ratings_latest_first(supplier_id, executor).await?;
        let commercial_profiles = self
            .list_commercial_profiles_latest_first(supplier_id, executor)
            .await?;
        let commercial_party_names = self
            .commercial_party_names(&commercial_profiles, executor)
            .await?;
        Ok(Some(SupplierDetailBundle {
            supplier,
            party,
            party_revision,
            contacts,
            addresses,
            tax_profiles,
            bank_accounts,
            capabilities,
            qualifications,
            qualification_links,
            ratings,
            commercial_profiles,
            commercial_party_names,
        }))
    }

    /// 批量读取商务版本引用的签约与付款主体当前法定名称。
    ///
    /// # 参数
    /// * `profiles` - 已加载的商务资料历史
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回主体 ID 字符串到当前法定名称的映射；缺少当前修订时无键。
    ///
    /// # 错误
    /// 主体或当前修订批量查询失败时返回错误。
    ///
    /// # 约束
    /// 只经主体域属主访问器组装，不在供应商仓储内直查外域集合。
    async fn commercial_party_names(
        &self,
        profiles: &[SupplierCommercialProfileRevision],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        let party_ids: Vec<PartyId> = profiles
            .iter()
            .flat_map(|profile| {
                [
                    profile.signing_entity_party_id.to_string(),
                    profile.payment_entity_party_id.to_string(),
                ]
            })
            .collect::<HashSet<_>>()
            .into_iter()
            .map(PartyId::new)
            .collect();
        if party_ids.is_empty() {
            return Ok(HashMap::new());
        }
        self.db
            .party()
            .current_legal_names_by_party_ids(&party_ids, executor)
            .await
    }
}

/// 供应商列表仓储侧资质健康状态筛选。
///
/// # 约束
/// 仓储自有类型，避免数据库层依赖 Service DTO；语义与 Service 侧
/// `SupplierQualificationHealth` 一一对应。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupplierQualificationHealthFilter {
    /// 当前有效。
    Valid,
    /// 当前有效且 30 天内到期。
    Expiring30,
    /// 已失效。
    Expired,
    /// 尚未登记对应资质。
    NotRegistered,
}

/// 资质筛选约束的纯分支种类（`PROC-R03`）。
///
/// # 约束
/// 纯内存判定，不触及 I/O；`list_filter_qualification_constraints` 按此 branching
/// 选择命中集合或排除集合查询，分支语义与本枚举一一对应。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QualificationConstraintKind {
    /// 未筛选资质，不产生候选约束。
    Unconstrained,
    /// 按类型或健康状态产生命中集合。
    Included,
    /// `NotRegistered` 产生排除集合，命中集合为空。
    Excluded,
}

/// 判定资质筛选约束的纯分支种类。
///
/// 未传健康状态且资质类型为空时无约束；`NotRegistered` 无论类型是否为空均走
/// 排除分支；其余组合均走命中分支。
///
/// # 参数
/// * `qualification_types` - 资质类型；空集合表示不限制类型
/// * `health` - 资质健康状态；`None` 表示仅按类型命中
///
/// # 返回
/// 返回命中、排除或无约束的分支种类。
///
/// # 错误
/// 无；仅做内存分支判定。
///
/// # 约束
/// 纯内存判定，不触及 I/O；分支结果必须与
/// `list_filter_qualification_constraints` 的查询选择保持一致。
fn qualification_constraint_kind(
    qualification_types: &[QualificationType],
    health: Option<SupplierQualificationHealthFilter>,
) -> QualificationConstraintKind {
    match health {
        None if qualification_types.is_empty() => QualificationConstraintKind::Unconstrained,
        None => QualificationConstraintKind::Included,
        Some(SupplierQualificationHealthFilter::NotRegistered) => QualificationConstraintKind::Excluded,
        Some(_) => QualificationConstraintKind::Included,
    }
}

/// 供应商列表仓储搜索输入。
///
/// # 约束
/// 全部字段均为 Service 已规范化的业务值；分页排序在仓储白名单内校验。
#[derive(Debug, Clone)]
pub struct SupplierListSearchInput {
    /// 供应商编号模糊搜索。
    pub keyword: Option<String>,
    /// 共用企业主体精确匹配。
    pub party_id: Option<PartyId>,
    /// 启停状态。
    pub status: Option<SupplierAccountStatus>,
    /// 能力代码。
    pub capability_codes: Vec<CapabilityCode>,
    /// 资质类型。
    pub qualification_types: Vec<QualificationType>,
    /// 资质健康状态。
    pub qualification_health: Option<SupplierQualificationHealthFilter>,
    /// 当前业务日字符串。
    pub as_of: String,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段。
    pub sort_by: Option<String>,
    /// 是否升序。
    pub sort_ascending: bool,
}

/// 供应商列表事实束。
///
/// # 约束
/// 仅承载持久化事实与投影行，不含 View 映射与授权结论。
#[derive(Debug)]
pub struct SupplierListBundle {
    /// 当前页投影行与总数。
    pub page: PageResult<SupplierAccountRow>,
    /// 命中的主体集合。
    pub parties: Vec<Party>,
    /// 命中的主体当前修订集合。
    pub revisions: Vec<PartyRevision>,
    /// 命中的商务资料版本集合。
    pub profiles: Vec<SupplierCommercialProfileRevision>,
}

/// 供应商详情事实束。
///
/// # 约束
/// 仅承载持久化事实；敏感令牌签发与 View 映射保留在 Service。
#[derive(Debug)]
pub struct SupplierDetailBundle {
    /// 供应商角色。
    pub supplier: SupplierAccount,
    /// 关联主体；缺失时为 `None`。
    pub party: Option<Party>,
    /// 主体当前修订；指针缺失或目标缺失时为 `None`。
    pub party_revision: Option<PartyRevision>,
    /// 联系人历史集合。
    pub contacts: Vec<PartyContact>,
    /// 地址历史集合。
    pub addresses: Vec<PartyAddress>,
    /// 税务资料历史集合。
    pub tax_profiles: Vec<PartyTaxProfile>,
    /// 银行账户历史集合。
    pub bank_accounts: Vec<PartyBankAccount>,
    /// 能力集合。
    pub capabilities: Vec<SupplierCapability>,
    /// 资质集合。
    pub qualifications: Vec<SupplierQualification>,
    /// 资质适用能力关联集合。
    pub qualification_links: Vec<SupplierQualificationCapability>,
    /// 评级历史（最新优先）。
    pub ratings: Vec<SupplierRatingRevision>,
    /// 商务资料历史（最新优先）。
    pub commercial_profiles: Vec<SupplierCommercialProfileRevision>,
    /// 商务版本引用的签约/付款主体当前法定名称。
    pub commercial_party_names: HashMap<String, String>,
}

/// 合并两个供应商角色候选集合；两个条件同时存在时取交集。
///
/// # 参数
/// * `current` - 已有筛选条件命中的候选集合
/// * `matched` - 新筛选条件命中的候选集合
///
/// # 返回
/// 两者均存在时返回交集，仅一者存在时原样返回，均不存在时返回 `None`。
///
/// # 错误
/// 无。
///
/// # 约束
/// 纯内存集合运算，不触及 I/O；输入顺序按 `current` 保留，交集判定经哈希集合完成。
fn intersect_supplier_ids(
    current: Option<Vec<SupplierAccountId>>,
    matched: Option<Vec<SupplierAccountId>>,
) -> Option<Vec<SupplierAccountId>> {
    let (current, matched) = match (current, matched) {
        (Some(current), Some(matched)) => (current, matched),
        (Some(current), None) => return Some(current),
        (None, Some(matched)) => return Some(matched),
        (None, None) => return None,
    };
    let matched: HashSet<String> = matched.into_iter().map(|id| id.to_string()).collect();
    Some(
        current
            .into_iter()
            .filter(|id| matched.contains(&id.to_string()))
            .collect(),
    )
}

/// 计算“30 天内到期”筛选窗口的结束业务日。
///
/// # 参数
/// * `as_of` - 窗口起始业务日字符串
///
/// # 返回
/// 返回起始日后第三十个自然日的稳定日期字符串。
///
/// # 错误
/// 日期格式非法或计算溢出时返回错误。
///
/// # 约束
/// 纯日期计算，不触及 I/O；窗口长度固定为 30 天。
fn qualification_expiry_cutoff(as_of: &str) -> Result<String> {
    let as_of = as_of
        .parse::<entities::common::time::BusinessDate>()
        .map_err(|_| Error::EntityMetadataOutOfRange("supplier business date"))?;
    Ok(SupplierQualification::expiry_cutoff(as_of, 30)
        .map_err(|_| Error::EntityMetadataOutOfRange("supplier expiry cutoff"))?
        .to_string())
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

/// 执行业务主键重复审计聚合，复用调用方执行器的会话语义。
///
/// # 参数
/// * `collection` - 供应商账号集合句柄
/// * `executor` - 数据访问执行器，由调用方决定是否位于事务中
///
/// # 返回
/// 按 `id` 字典序排列的重复主键及出现次数。
///
/// # 错误
/// 当 MongoDB 聚合或游标读取失败时返回错误。
async fn aggregate_id_duplicates(
    collection: &mongodb::Collection<Document>,
    executor: &mut dyn Executor,
) -> Result<Vec<SupplierAccountIdDuplicate>> {
    let pipeline = supplier_account_id_duplicate_pipeline();
    let rows = match executor.session() {
        Some(session) => {
            collection
                .aggregate(pipeline)
                .with_type::<SupplierAccountIdDuplicate>()
                .session(&mut *session)
                .await?
                .stream(session)
                .try_collect::<Vec<_>>()
                .await
        }
        None => {
            collection
                .aggregate(pipeline)
                .with_type::<SupplierAccountIdDuplicate>()
                .await?
                .try_collect::<Vec<_>>()
                .await
        }
    }
    .map_err(crate::Error::from)?;
    Ok(rows)
}

/// 返回业务主键重复审计的聚合管道，供审计执行与测试共用。
///
/// 按 `id` 分组计数，仅保留出现超过一次的主键并按 `id` 字典序稳定排列；
/// 不附加软删除过滤，全局唯一索引同样约束已软删除文档。
///
/// # 返回
/// 与 [`SupplierRepository::duplicate_supplier_account_ids`] 相同的聚合管道。
fn supplier_account_id_duplicate_pipeline() -> Vec<Document> {
    vec![
        doc! { "$group": { "_id": "$id", "count": { "$sum": 1 } } },
        doc! { "$match": { "count": { "$gt": 1 } } },
        doc! { "$sort": { "_id": 1 } },
        doc! { "$project": { "_id": 0, "id": "$_id", "count": 1 } },
    ]
}

#[cfg(test)]
mod tests {
    use super::{
        intersect_supplier_ids, qualification_constraint_kind, qualification_expiry_cutoff, sort_doc,
        QualificationConstraintKind, QueryFilter, SupplierAccountFilter, SupplierCapabilityFilter,
        SupplierQualificationFilter, SupplierQualificationHealthFilter,
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

    #[test]
    fn supplier_list_candidate_intersection_preserves_order_and_empty() {
        use entities::ids::SupplierAccountId;
        // 能力与资质双维度同时命中时取交集，且保留能力侧顺序。
        let capability_ids = Some(vec![
            SupplierAccountId::new("s-1"),
            SupplierAccountId::new("s-2"),
            SupplierAccountId::new("s-3"),
        ]);
        let qualification_ids = Some(vec![
            SupplierAccountId::new("s-2"),
            SupplierAccountId::new("s-3"),
            SupplierAccountId::new("s-4"),
        ]);
        assert_eq!(
            intersect_supplier_ids(capability_ids, qualification_ids)
                .unwrap()
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            vec!["s-2".to_string(), "s-3".to_string()]
        );
        // 仅一侧约束时原样透传。
        assert_eq!(
            intersect_supplier_ids(Some(vec![SupplierAccountId::new("s-1")]), None)
                .unwrap()
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            vec!["s-1".to_string()]
        );
        assert_eq!(
            intersect_supplier_ids(None, Some(vec![SupplierAccountId::new("s-9")]))
                .unwrap()
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            vec!["s-9".to_string()]
        );
        assert!(intersect_supplier_ids(None, None).is_none());
        assert_eq!(
            intersect_supplier_ids(
                Some(vec![SupplierAccountId::new("s-1")]),
                Some(vec![SupplierAccountId::new("s-9")])
            )
            .unwrap()
            .len(),
            0
        );
    }

    /// 资质约束分支覆盖类型、健康状态与未登记排除路径。
    #[test]
    fn qualification_constraint_kind_covers_all_branches() {
        use entities::supplier::QualificationType;
        use QualificationConstraintKind::{Excluded, Included, Unconstrained};
        // 未筛选资质时无约束。
        assert_eq!(qualification_constraint_kind(&[], None), Unconstrained);
        // 仅按类型命中。
        assert_eq!(
            qualification_constraint_kind(&[QualificationType::FoodLicense], None),
            Included
        );
        // 各健康状态均走命中分支。
        for health in [
            SupplierQualificationHealthFilter::Valid,
            SupplierQualificationHealthFilter::Expiring30,
            SupplierQualificationHealthFilter::Expired,
        ] {
            assert_eq!(
                qualification_constraint_kind(&[QualificationType::FoodLicense], Some(health)),
                Included,
                "健康状态 {health:?} 应走命中分支"
            );
            assert_eq!(
                qualification_constraint_kind(&[], Some(health)),
                Included,
                "空类型下健康状态 {health:?} 仍应走命中分支"
            );
        }
        // 未登记资质走排除分支，命中集合为空。
        assert_eq!(
            qualification_constraint_kind(
                &[QualificationType::FoodLicense],
                Some(SupplierQualificationHealthFilter::NotRegistered)
            ),
            Excluded
        );
        assert_eq!(
            qualification_constraint_kind(&[], Some(SupplierQualificationHealthFilter::NotRegistered)),
            Excluded
        );
    }

    /// 到期窗口固定为起始日后第三十个自然日。
    #[test]
    fn supplier_list_expiry_cutoff_is_thirty_days_after_as_of() {
        assert_eq!(qualification_expiry_cutoff("2026-08-31").unwrap(), "2026-09-30");
        assert_eq!(qualification_expiry_cutoff("2026-01-31").unwrap(), "2026-03-02");
        assert_eq!(qualification_expiry_cutoff("2026-02-01").unwrap(), "2026-03-03");
        assert!(qualification_expiry_cutoff("not-a-date").is_err());
        assert!(qualification_expiry_cutoff("2026-13-01").is_err());
    }

    #[test]
    fn duplicate_audit_pipeline_groups_and_reports_repeated_ids() {
        let pipeline = super::supplier_account_id_duplicate_pipeline();

        assert_eq!(pipeline.len(), 4);
        let rendered = format!("{pipeline:?}");
        assert!(rendered.contains("$group"));
        assert!(rendered.contains("$match"));
        assert!(rendered.contains("$sort"));
        assert!(rendered.contains("$project"));
        assert_eq!(
            pipeline[1]
                .get_document("$match")
                .unwrap()
                .get_document("count")
                .unwrap(),
            &doc! { "$gt": 1 }
        );
    }
}

/// PROC-R10 供应商业务主键索引的真实 MongoDB 验收（隔离库，Quality 单独执行）。
#[cfg(test)]
mod proc_r10_mongo_tests {
    use mongodb::bson::{doc, Document};
    use test_support::{require_mongo, TestDb};

    use super::{SupplierAccountIdDuplicate, SUPPLIER_ACCOUNTS};
    use crate::{ensure_indexes, NoTransaction, SupplierExt};

    /// 插入仅携带索引相关字段的供应商账号原始文档。
    ///
    /// # 参数
    /// * `db` - 隔离测试库
    /// * `id` - 业务主键（可故意重复）
    /// * `party_id` - 所属主体（保持唯一，避免干扰其他唯一索引）
    /// * `supplier_no` - 供应商编号（保持唯一，避免干扰其他唯一索引）
    ///
    /// # 错误
    /// 写入失败时 panic。
    async fn insert_raw_supplier_account(
        db: &mongodb::Database,
        id: &str,
        party_id: &str,
        supplier_no: &str,
    ) {
        db.collection::<Document>(SUPPLIER_ACCOUNTS)
            .insert_one(doc! {
                "id": id,
                "party_id": party_id,
                "supplier_no": supplier_no,
                "deleted_at": 0_i64,
            })
            .await
            .expect("原始供应商账号写入失败");
    }

    /// 重复 `id` 必须先被审计报出，再拒绝索引迁移并输出冲突索引诊断。
    ///
    /// # 参数
    /// 无，内部创建隔离库。
    ///
    /// # 返回
    /// 审计命中重复主键且迁移失败关闭时通过。
    ///
    /// # 错误
    /// 审计漏报或迁移未拒绝时测试失败。
    ///
    /// # 约束
    /// 仅验证 `supplier_accounts` 集合；`#[ignore]` 由 Quality 在隔离副本集执行。
    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn duplicate_supplier_account_ids_are_audited_and_refuse_migration() {
        require_mongo!(async {
            let fixture = TestDb::new("proc_r10_supplier_id_dup")
                .await
                .expect("测试数据库创建失败");
            insert_raw_supplier_account(fixture.db(), "dup-1", "pty-a", "SUP-A").await;
            insert_raw_supplier_account(fixture.db(), "dup-1", "pty-b", "SUP-B").await;
            insert_raw_supplier_account(fixture.db(), "sup-ok", "pty-c", "SUP-C").await;

            let duplicates = fixture
                .db()
                .supplier()
                .duplicate_supplier_account_ids(&mut NoTransaction)
                .await
                .expect("重复审计查询失败");
            assert_eq!(
                duplicates,
                vec![SupplierAccountIdDuplicate {
                    id: "dup-1".to_string(),
                    count: 2
                }],
                "部署前审计必须报出重复 id"
            );

            let err = ensure_indexes(fixture.db())
                .await
                .expect_err("重复 id 必须拒绝建索引");
            let rendered = format!("{err:?}");
            assert!(
                rendered.contains("uk_supplier_accounts_id"),
                "诊断必须包含冲突索引名：{rendered}"
            );
        });
    }

    /// `id $in` 批量查询的执行计划必须命中唯一索引且无集合扫描。
    ///
    /// # 参数
    /// 无，内部创建隔离库。
    ///
    /// # 返回
    /// `explain` 命中 `uk_supplier_accounts_id` 的 `IXSCAN` 且无 `COLLSCAN` 时通过。
    ///
    /// # 错误
    /// 索引未命中或出现集合扫描时测试失败。
    ///
    /// # 约束
    /// 不使用 `hint`；`#[ignore]` 由 Quality 在隔离副本集执行。
    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn supplier_id_in_queries_use_unique_id_index() {
        require_mongo!(async {
            let fixture = TestDb::new("proc_r10_supplier_id_explain")
                .await
                .expect("测试数据库创建失败");
            ensure_indexes(fixture.db()).await.expect("索引创建失败");
            insert_raw_supplier_account(fixture.db(), "sup-1", "pty-1", "SUP-1").await;

            let explain = fixture
                .db()
                .run_command(doc! {
                    "explain": {
                        "find": SUPPLIER_ACCOUNTS,
                        "filter": {
                            "id": { "$in": ["sup-1", "sup-missing"] },
                            "deleted_at": 0_i64,
                        },
                    },
                    "verbosity": "executionStats",
                })
                .await
                .expect("供应商 id 查询 explain 失败");
            let rendered = format!("{explain:?}");
            assert!(rendered.contains("IXSCAN"), "explain 未使用 IXSCAN：{rendered}");
            assert!(
                rendered.contains("uk_supplier_accounts_id"),
                "explain 未命中 uk_supplier_accounts_id：{rendered}"
            );
            assert!(
                !rendered.contains("COLLSCAN"),
                "explain 出现 COLLSCAN：{rendered}"
            );
        });
    }
}

/// `PROC-R03`/`PROC-R04` 列表与详情事实束的真实 MongoDB 验收（隔离库，Quality 单独执行）。
#[cfg(test)]
mod proc_supplier_io_mongo_tests {
    use std::str::FromStr;

    use entities::common::time::BusinessDate;
    use entities::ids::{
        PartyId, PartyRevisionId, SupplierAccountId, SupplierCapabilityId,
        SupplierCommercialProfileRevisionId, SupplierQualificationCapabilityId, SupplierQualificationId,
        SupplierRatingRevisionId,
    };
    use entities::money::Rate;
    use entities::party::{Party, PartyData, PartyKind, PartyRevision, PartyRevisionData, PartyStatus};
    use entities::supplier::{
        CapabilityCode, CapabilityStatus, InvoiceType, QualificationStatus, QualificationType,
        ReconciliationCycle, SettlementMode, SupplierAccount, SupplierAccountData, SupplierAccountStatus,
        SupplierCapability, SupplierCapabilityData, SupplierCommercialProfileRevision,
        SupplierCommercialProfileRevisionData, SupplierQualification, SupplierQualificationCapability,
        SupplierQualificationCapabilityData, SupplierQualificationData, SupplierRating,
        SupplierRatingRevision, SupplierRatingRevisionData,
    };
    use mongodb::bson::doc;
    use test_support::{require_mongo, TestDb};

    use super::{
        SupplierListSearchInput, SupplierQualificationHealthFilter, SUPPLIER_ACCOUNTS, SUPPLIER_CAPABILITIES,
    };
    use crate::{ensure_indexes, NoTransaction, PartyExt, SupplierExt, Transactional};

    /// 列表与详情验收的业务日。
    const AS_OF: &str = "2026-08-31";

    /// 构造带当前修订的主体。
    ///
    /// # 参数
    /// * `id` - 主体稳定 ID
    /// * `revision_id` - 当前修订 ID
    /// * `legal_name` - 法定名称
    ///
    /// # 返回
    /// 返回主体与其当前修订。
    fn party_with_revision(id: &str, revision_id: &str, legal_name: &str) -> (Party, PartyRevision) {
        let mut party = Party::new(
            PartyId::new(id),
            PartyData {
                party_no: format!("P-{id}"),
                party_kind: PartyKind::Enterprise,
                unified_credit_code: None,
                status: PartyStatus::Active,
            },
            "test",
        )
        .expect("主体构造失败");
        let revision = PartyRevision::new(
            PartyRevisionId::new(revision_id),
            PartyRevisionData {
                party_id: PartyId::new(id),
                revision_no: 1,
                legal_name: legal_name.to_string(),
                short_name: None,
                change_reason: "初始登记".to_string(),
            },
        )
        .expect("主体修订构造失败");
        party.stable.current_revision_id = Some(revision_id.to_string());
        (party, revision)
    }

    /// 构造供应商角色。
    ///
    /// # 参数
    /// * `id` - 供应商稳定 ID
    /// * `party_id` - 所属主体 ID
    /// * `supplier_no` - 供应商编号
    /// * `profile_id` - 当前商务资料 ID；`None` 表示无当前指针
    ///
    /// # 返回
    /// 返回未删除的启用供应商角色。
    fn supplier_account(
        id: &str,
        party_id: &str,
        supplier_no: &str,
        profile_id: Option<&str>,
    ) -> SupplierAccount {
        SupplierAccount::new(
            SupplierAccountId::new(id),
            SupplierAccountData {
                party_id: PartyId::new(party_id),
                supplier_no: supplier_no.to_string(),
                default_payment_term_id: None,
                current_commercial_profile_revision_id: profile_id
                    .map(SupplierCommercialProfileRevisionId::new),
                status: SupplierAccountStatus::Active,
            },
            "test",
        )
        .expect("供应商角色构造失败")
    }

    /// 构造首版商务资料。
    ///
    /// # 参数
    /// * `id` - 修订 ID
    /// * `supplier_id` - 所属供应商 ID
    /// * `party_id` - 签约与付款主体 ID
    ///
    /// # 返回
    /// 返回修订号为 1 的商务资料。
    fn commercial_profile(id: &str, supplier_id: &str, party_id: &str) -> SupplierCommercialProfileRevision {
        SupplierCommercialProfileRevision::new(
            SupplierCommercialProfileRevisionId::new(id),
            SupplierCommercialProfileRevisionData {
                supplier_id: SupplierAccountId::new(supplier_id),
                revision_no: 1,
                settlement_mode: SettlementMode::PayAfterUse,
                reconciliation_cycle: ReconciliationCycle::Monthly,
                payment_term_snapshot: "NET-30".to_string(),
                business_category: Some("经营类目".to_string()),
                invoice_type: InvoiceType::VatSpecial,
                invoice_tax_rate: Rate::from_str("0.13").unwrap(),
                signing_entity_party_id: PartyId::new(party_id),
                payment_entity_party_id: PartyId::new(party_id),
                change_reason: "初始登记".to_string(),
            },
        )
        .expect("商务资料构造失败")
    }

    /// 构造长期有效的启用能力。
    ///
    /// # 参数
    /// * `id` - 能力稳定 ID
    /// * `supplier_id` - 所属供应商 ID
    /// * `code` - 能力代码
    ///
    /// # 返回
    /// 返回自 2026-01-01 生效的启用能力。
    fn capability(id: &str, supplier_id: &str, code: CapabilityCode) -> SupplierCapability {
        SupplierCapability::new(
            SupplierCapabilityId::new(id),
            SupplierCapabilityData {
                supplier_id: SupplierAccountId::new(supplier_id),
                capability_code: code,
                service_region: None,
                owner_user_id: "test".to_string(),
                fulfillment_note: None,
                valid_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
                valid_to: None,
                status: CapabilityStatus::Active,
            },
            "test",
        )
        .expect("供应商能力构造失败")
    }

    /// 构造资质。
    ///
    /// # 参数
    /// * `id` - 资质稳定 ID
    /// * `supplier_id` - 所属供应商 ID
    /// * `qualification_type` - 资质类型
    /// * `certificate_no` - 证书编号
    /// * `valid_to` - 失效日；`None` 表示长期有效
    ///
    /// # 返回
    /// 返回自 2026-01-01 生效的启用资质。
    fn qualification(
        id: &str,
        supplier_id: &str,
        qualification_type: QualificationType,
        certificate_no: &str,
        valid_to: Option<BusinessDate>,
    ) -> SupplierQualification {
        SupplierQualification::new(
            SupplierQualificationId::new(id),
            SupplierQualificationData {
                supplier_id: SupplierAccountId::new(supplier_id),
                qualification_type,
                certificate_no: certificate_no.to_string(),
                issuer: None,
                valid_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
                valid_to,
                attachment_id: None,
                status: QualificationStatus::Active,
            },
            "test",
        )
        .expect("供应商资质构造失败")
    }

    /// 写入列表与详情验收夹具。
    ///
    /// sup-a（Physical 能力 + 10 天内到期的有效食品资质 + 第二份合同资质 +
    /// 评级 + 商务资料）；sup-b（Physical 能力，无资质）；sup-c（Api 能力 +
    /// 已过期食品资质）；sup-orphan（主体缺失）。
    ///
    /// # 参数
    /// * `db` - 隔离测试库
    ///
    /// # 错误
    /// 任一夹具写入失败时 panic。
    async fn seed_supplier_io_fixture(db: &mongodb::Database) {
        for (party_id, revision_id, legal_name) in [
            ("party-a", "partyrev-a", "供应商甲"),
            ("party-b", "partyrev-b", "供应商乙"),
            ("party-c", "partyrev-c", "供应商丙"),
        ] {
            let (party, revision) = party_with_revision(party_id, revision_id, legal_name);
            db.parties()
                .create(&party, &mut NoTransaction)
                .await
                .expect("主体写入失败");
            db.party_revisions()
                .create(&revision, &mut NoTransaction)
                .await
                .expect("主体修订写入失败");
        }
        for (id, party_id, supplier_no, profile_id) in [
            ("sup-a", "party-a", "SUP-A", Some("profile-a")),
            ("sup-b", "party-b", "SUP-B", None),
            ("sup-c", "party-c", "SUP-C", None),
            ("sup-orphan", "party-missing", "SUP-ORPHAN", None),
        ] {
            db.supplier_accounts()
                .create(
                    &supplier_account(id, party_id, supplier_no, profile_id),
                    &mut NoTransaction,
                )
                .await
                .expect("供应商角色写入失败");
        }
        db.supplier_commercial_profile_revisions()
            .create(
                &commercial_profile("profile-a", "sup-a", "party-a"),
                &mut NoTransaction,
            )
            .await
            .expect("商务资料写入失败");
        for (id, supplier_id, code) in [
            ("cap-a1", "sup-a", CapabilityCode::Physical),
            ("cap-b1", "sup-b", CapabilityCode::Physical),
            ("cap-c1", "sup-c", CapabilityCode::Api),
        ] {
            db.supplier_capabilities()
                .create(&capability(id, supplier_id, code), &mut NoTransaction)
                .await
                .expect("供应商能力写入失败");
        }
        let expiring = BusinessDate::from_ymd(2026, 9, 10).unwrap();
        let expired = BusinessDate::from_ymd(2026, 6, 1).unwrap();
        for (id, supplier_id, qualification_type, certificate_no, valid_to) in [
            (
                "qual-a",
                "sup-a",
                QualificationType::FoodLicense,
                "FOOD-A",
                Some(expiring),
            ),
            ("qual-a2", "sup-a", QualificationType::Contract, "HT-A", None),
            (
                "qual-c",
                "sup-c",
                QualificationType::FoodLicense,
                "FOOD-C",
                Some(expired),
            ),
        ] {
            db.supplier_qualifications()
                .create(
                    &qualification(id, supplier_id, qualification_type, certificate_no, valid_to),
                    &mut NoTransaction,
                )
                .await
                .expect("供应商资质写入失败");
        }
        for (id, qualification_id, capability_id) in
            [("link-a1", "qual-a", "cap-a1"), ("link-a2", "qual-a2", "cap-a1")]
        {
            db.supplier_qualification_capabilities()
                .create(
                    &SupplierQualificationCapability::new(
                        SupplierQualificationCapabilityId::new(id),
                        SupplierQualificationCapabilityData {
                            qualification_id: SupplierQualificationId::new(qualification_id),
                            capability_id: SupplierCapabilityId::new(capability_id),
                        },
                    )
                    .expect("资质关联构造失败"),
                    &mut NoTransaction,
                )
                .await
                .expect("资质关联写入失败");
        }
        db.supplier_rating_revisions()
            .create(
                &SupplierRatingRevision::new(
                    SupplierRatingRevisionId::new("rating-a"),
                    SupplierRatingRevisionData {
                        supplier_id: SupplierAccountId::new("sup-a"),
                        revision_no: 1,
                        initial_score: Some(80),
                        rating: SupplierRating::A,
                        current_score: 85,
                        valid_from: BusinessDate::from_ymd(2026, 8, 1).unwrap(),
                        valid_to: None,
                        change_reason: "初始评级".to_string(),
                    },
                )
                .expect("供应商评级构造失败"),
                &mut NoTransaction,
            )
            .await
            .expect("供应商评级写入失败");
    }

    /// 构造列表搜索输入。
    ///
    /// # 参数
    /// * `build` - 输入调整闭包，由调用方设置筛选维度
    ///
    /// # 返回
    /// 返回业务日固定为验收日的搜索输入。
    fn list_input(build: impl FnOnce(&mut SupplierListSearchInput)) -> SupplierListSearchInput {
        let mut input = SupplierListSearchInput {
            keyword: None,
            party_id: None,
            status: None,
            capability_codes: Vec::new(),
            qualification_types: Vec::new(),
            qualification_health: None,
            as_of: AS_OF.to_string(),
            page: 1,
            page_size: 20,
            sort_by: Some("created_at".to_string()),
            sort_ascending: false,
        };
        build(&mut input);
        input
    }

    /// 返回事实束页中的供应商 ID 集合（字典序）。
    ///
    /// # 参数
    /// * `bundle` - 列表事实束
    ///
    /// # 返回
    /// 返回当前页供应商 ID 的稳定排序集合。
    fn bundle_ids(bundle: &super::SupplierListBundle) -> Vec<String> {
        let mut ids: Vec<String> = bundle.page.items.iter().map(|row| row.id.clone()).collect();
        ids.sort();
        ids
    }

    /// 关键词、能力、资质健康状态均在分页计数前生效，且总数不受页大小影响。
    ///
    /// # 参数
    /// 无，内部创建隔离库。
    ///
    /// # 返回
    /// 组合筛选、`NotRegistered` 排除集、能力与资质交集及分页总数全部符合预期时通过。
    ///
    /// # 错误
    /// 任一筛选总数、分页总数或水合事实与预期不一致时测试失败。
    ///
    /// # 约束
    /// `#[ignore]` 由 Quality 在隔离副本集执行；总数断言覆盖分页前生效语义。
    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn supplier_list_bundle_applies_all_prefilters_before_paging() {
        require_mongo!(async {
            let fixture = TestDb::new("proc_supplier_io_list")
                .await
                .expect("测试数据库创建失败");
            ensure_indexes(fixture.db()).await.expect("索引创建失败");
            seed_supplier_io_fixture(fixture.db()).await;

            let bundle = fixture
                .db()
                .supplier()
                .load_supplier_list_bundle(&list_input(|_| {}), &mut NoTransaction)
                .await
                .expect("列表事实束加载失败");
            assert_eq!(bundle.page.total, 4, "无筛选时总数应覆盖全部供应商");
            assert_eq!(bundle_ids(&bundle), vec!["sup-a", "sup-b", "sup-c", "sup-orphan"]);

            let paged = fixture
                .db()
                .supplier()
                .load_supplier_list_bundle(
                    &list_input(|input| {
                        input.page_size = 1;
                    }),
                    &mut NoTransaction,
                )
                .await
                .expect("分页事实束加载失败");
            assert_eq!(paged.page.total, 4, "总数不得随页大小变化");
            assert_eq!(paged.page.items.len(), 1);

            let keyword = fixture
                .db()
                .supplier()
                .load_supplier_list_bundle(
                    &list_input(|input| {
                        input.keyword = Some("供应商甲".to_string());
                    }),
                    &mut NoTransaction,
                )
                .await
                .expect("关键词事实束加载失败");
            assert_eq!(bundle_ids(&keyword), vec!["sup-a"], "主体名称命中应在分页前生效");

            let capability = fixture
                .db()
                .supplier()
                .load_supplier_list_bundle(
                    &list_input(|input| {
                        input.capability_codes = vec![CapabilityCode::Physical];
                    }),
                    &mut NoTransaction,
                )
                .await
                .expect("能力筛选事实束加载失败");
            assert_eq!(bundle_ids(&capability), vec!["sup-a", "sup-b"]);

            let valid = fixture
                .db()
                .supplier()
                .load_supplier_list_bundle(
                    &list_input(|input| {
                        input.qualification_types = vec![QualificationType::FoodLicense];
                        input.qualification_health = Some(SupplierQualificationHealthFilter::Valid);
                    }),
                    &mut NoTransaction,
                )
                .await
                .expect("有效资质事实束加载失败");
            assert_eq!(bundle_ids(&valid), vec!["sup-a"]);

            let expiring = fixture
                .db()
                .supplier()
                .load_supplier_list_bundle(
                    &list_input(|input| {
                        input.qualification_types = vec![QualificationType::FoodLicense];
                        input.qualification_health = Some(SupplierQualificationHealthFilter::Expiring30);
                    }),
                    &mut NoTransaction,
                )
                .await
                .expect("临期资质事实束加载失败");
            assert_eq!(bundle_ids(&expiring), vec!["sup-a"]);

            let expired = fixture
                .db()
                .supplier()
                .load_supplier_list_bundle(
                    &list_input(|input| {
                        input.qualification_types = vec![QualificationType::FoodLicense];
                        input.qualification_health = Some(SupplierQualificationHealthFilter::Expired);
                    }),
                    &mut NoTransaction,
                )
                .await
                .expect("失效资质事实束加载失败");
            assert_eq!(bundle_ids(&expired), vec!["sup-c"]);

            let not_registered = fixture
                .db()
                .supplier()
                .load_supplier_list_bundle(
                    &list_input(|input| {
                        input.qualification_types = vec![QualificationType::FoodLicense];
                        input.qualification_health = Some(SupplierQualificationHealthFilter::NotRegistered);
                    }),
                    &mut NoTransaction,
                )
                .await
                .expect("未登记资质事实束加载失败");
            assert_eq!(
                bundle_ids(&not_registered),
                vec!["sup-b", "sup-orphan"],
                "未登记分支应返回排除集，命中集合为空"
            );

            let intersection = fixture
                .db()
                .supplier()
                .load_supplier_list_bundle(
                    &list_input(|input| {
                        input.capability_codes = vec![CapabilityCode::Physical];
                        input.qualification_types = vec![QualificationType::FoodLicense];
                        input.qualification_health = Some(SupplierQualificationHealthFilter::Valid);
                    }),
                    &mut NoTransaction,
                )
                .await
                .expect("交集筛选事实束加载失败");
            assert_eq!(bundle_ids(&intersection), vec!["sup-a"]);

            assert!(
                bundle.parties.iter().any(|party| party.base.id == "party-a"),
                "水合事实应包含命中主体"
            );
            assert!(
                bundle
                    .revisions
                    .iter()
                    .any(|revision| revision.base.id == "partyrev-a"),
                "水合事实应包含主体当前修订"
            );
            assert!(
                bundle
                    .profiles
                    .iter()
                    .any(|profile| profile.base.id == "profile-a"),
                "水合事实应包含当前商务资料"
            );
        });
    }

    /// 详情事实束一次批量返回全部历史集合，缺失指针有明确语义。
    ///
    /// # 参数
    /// 无，内部创建隔离库。
    ///
    /// # 返回
    /// 资质与关联批量齐套、商务名称齐套、主体缺失与供应商缺失语义正确时通过。
    ///
    /// # 错误
    /// 任一集合缺失、关联不齐或缺失语义不符时测试失败。
    ///
    /// # 约束
    /// `#[ignore]` 由 Quality 在隔离副本集执行；关联一次批量读取，不断言查询次数。
    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn supplier_detail_bundle_returns_batch_facts_and_missing_pointer_semantics() {
        require_mongo!(async {
            let fixture = TestDb::new("proc_supplier_io_detail")
                .await
                .expect("测试数据库创建失败");
            ensure_indexes(fixture.db()).await.expect("索引创建失败");
            seed_supplier_io_fixture(fixture.db()).await;

            let bundle = fixture
                .db()
                .supplier()
                .load_supplier_detail_bundle(&SupplierAccountId::new("sup-a"), &mut NoTransaction)
                .await
                .expect("详情事实束加载失败")
                .expect("sup-a 事实束缺失");
            assert_eq!(bundle.supplier.base.id, "sup-a");
            assert_eq!(bundle.party.as_ref().expect("主体缺失").base.id, "party-a");
            assert_eq!(
                bundle.party_revision.as_ref().expect("当前修订缺失").base.id,
                "partyrev-a"
            );
            assert_eq!(bundle.capabilities.len(), 1);
            assert_eq!(bundle.qualifications.len(), 2);
            assert_eq!(
                bundle.qualification_links.len(),
                2,
                "两份资质的适用关联应一次批量读回"
            );
            assert_eq!(bundle.ratings.len(), 1);
            assert_eq!(bundle.commercial_profiles.len(), 1);
            assert_eq!(
                bundle.commercial_party_names.get("party-a").map(String::as_str),
                Some("供应商甲")
            );

            let orphan = fixture
                .db()
                .supplier()
                .load_supplier_detail_bundle(&SupplierAccountId::new("sup-orphan"), &mut NoTransaction)
                .await
                .expect("孤儿事实束加载失败")
                .expect("sup-orphan 事实束缺失");
            assert!(orphan.party.is_none(), "主体缺失时应为 None");
            assert!(orphan.party_revision.is_none(), "主体缺失时修订应为 None");
            assert!(orphan.capabilities.is_empty());
            assert!(orphan.qualifications.is_empty());

            let missing = fixture
                .db()
                .supplier()
                .load_supplier_detail_bundle(&SupplierAccountId::new("sup-missing"), &mut NoTransaction)
                .await
                .expect("缺失供应商查询失败");
            assert!(missing.is_none(), "供应商缺失时应返回 None");
        });
    }

    /// 列表候选约束查询的执行计划必须命中唯一索引且无集合扫描。
    ///
    /// # 参数
    /// 无，内部创建隔离库。
    ///
    /// # 返回
    /// `explain` 命中 `uk_supplier_accounts_id` 的 `IXSCAN` 且无 `COLLSCAN` 时通过。
    ///
    /// # 错误
    /// 索引未命中或出现集合扫描时测试失败。
    ///
    /// # 约束
    /// 不使用 `hint`；`#[ignore]` 由 Quality 在隔离副本集执行。
    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn supplier_list_candidate_query_uses_index_without_collscan() {
        require_mongo!(async {
            let fixture = TestDb::new("proc_supplier_io_explain")
                .await
                .expect("测试数据库创建失败");
            ensure_indexes(fixture.db()).await.expect("索引创建失败");
            seed_supplier_io_fixture(fixture.db()).await;

            let explain = fixture
                .db()
                .run_command(doc! {
                    "explain": {
                        "find": SUPPLIER_ACCOUNTS,
                        "filter": {
                            "id": { "$in": ["sup-a", "sup-b"] },
                            "deleted_at": 0_i64,
                        },
                    },
                    "verbosity": "executionStats",
                })
                .await
                .expect("候选约束查询 explain 失败");
            let rendered = format!("{explain:?}");
            assert!(rendered.contains("IXSCAN"), "explain 未使用 IXSCAN：{rendered}");
            assert!(
                rendered.contains("uk_supplier_accounts_id"),
                "explain 未命中 uk_supplier_accounts_id：{rendered}"
            );
            assert!(
                !rendered.contains("COLLSCAN"),
                "explain 出现 COLLSCAN：{rendered}"
            );

            let capability_explain = fixture
                .db()
                .run_command(doc! {
                    "explain": {
                        "find": SUPPLIER_CAPABILITIES,
                        "filter": {
                            "supplier_id": "sup-a",
                            "deleted_at": 0_i64,
                        },
                    },
                    "verbosity": "executionStats",
                })
                .await
                .expect("能力查询 explain 失败");
            let capability_rendered = format!("{capability_explain:?}");
            assert!(
                capability_rendered.contains("IXSCAN"),
                "能力查询 explain 未使用 IXSCAN：{capability_rendered}"
            );
            assert!(
                !capability_rendered.contains("COLLSCAN"),
                "能力查询 explain 出现 COLLSCAN：{capability_rendered}"
            );
        });
    }

    /// 事实束查询复用调用方执行器，事务内可见同一会话写入。
    ///
    /// # 参数
    /// 无，内部创建隔离库。
    ///
    /// # 返回
    /// 事务内写入的供应商在同一会话的列表与详情事实束中均可见时通过。
    ///
    /// # 错误
    /// 事务内重验不可见或提交失败时测试失败。
    ///
    /// # 约束
    /// 事务内重验必须复用调用方 executor，不得另开连接或独立事务；
    /// `#[ignore]` 由 Quality 在隔离副本集执行。
    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn supplier_bundles_see_same_session_writes() {
        require_mongo!(async {
            let fixture = TestDb::new("proc_supplier_io_txn")
                .await
                .expect("测试数据库创建失败");
            ensure_indexes(fixture.db()).await.expect("索引创建失败");
            seed_supplier_io_fixture(fixture.db()).await;

            let (party, revision) = party_with_revision("party-txn", "partyrev-txn", "供应商丁");
            let supplier = supplier_account("sup-txn", "party-txn", "SUP-TXN-1", None);
            let trans_capability = capability("cap-txn", "sup-txn", CapabilityCode::Physical);

            let db = fixture.db().clone();
            let client = db.client().clone();
            client
                .with_transaction::<_, (), crate::errors::Error>(move |session| {
                    let db = db.clone();
                    let party = party.clone();
                    let revision = revision.clone();
                    let supplier = supplier.clone();
                    let trans_capability = trans_capability.clone();
                    Box::pin(async move {
                        db.parties().create(&party, session).await?;
                        db.party_revisions().create(&revision, session).await?;
                        db.supplier_accounts().create(&supplier, session).await?;
                        db.supplier_capabilities()
                            .create(&trans_capability, session)
                            .await?;
                        let bundle = db
                            .supplier()
                            .load_supplier_list_bundle(
                                &SupplierListSearchInput {
                                    keyword: Some("SUP-TXN-1".to_string()),
                                    party_id: None,
                                    status: None,
                                    capability_codes: Vec::new(),
                                    qualification_types: Vec::new(),
                                    qualification_health: None,
                                    as_of: AS_OF.to_string(),
                                    page: 1,
                                    page_size: 20,
                                    sort_by: Some("created_at".to_string()),
                                    sort_ascending: false,
                                },
                                session,
                            )
                            .await?;
                        assert_eq!(bundle.page.total, 1, "事务内应能 read-your-writes");
                        let detail = db
                            .supplier()
                            .load_supplier_detail_bundle(&SupplierAccountId::new("sup-txn"), session)
                            .await?;
                        let detail = detail.expect("事务内详情事实束缺失");
                        assert_eq!(detail.capabilities.len(), 1);
                        Ok(())
                    })
                })
                .await
                .expect("同一会话事务读写失败");
        });
    }
}
