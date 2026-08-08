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

use entities::ids::{PartyId, SupplierAccountId, SupplierCapabilityId, SupplierQualificationId};
use entities::supplier::{
    CapabilityCode, CapabilityStatus, QualificationStatus, QualificationType, SupplierAccount,
    SupplierAccountStatus, SupplierCapability, SupplierCommercialProfileRevision, SupplierProfileCommand,
    SupplierQualification, SupplierQualificationCapability,
};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use mongodb::Database;
use serde::{Deserialize, Serialize};

use super::extensions::SupplierExt;
use super::regex_filter::insert_literal_regex_filter;
use super::{PageResult, Pagination, QueryFilter, Repository};
use crate::executor::Executor;
use crate::{mongo_ops, Result};

/// `supplier_account` 集合名（单一来源：`SupplierExt` 关联常量）。
const SUPPLIER_ACCOUNTS: &str = <mongodb::Database as SupplierExt>::SUPPLIER_ACCOUNTS;
/// `supplier_commercial_profile_revision` 集合名（单一来源：`SupplierExt` 关联常量）。
const SUPPLIER_COMMERCIAL_PROFILE_REVISIONS: &str =
    <mongodb::Database as SupplierExt>::SUPPLIER_COMMERCIAL_PROFILE_REVISIONS;
/// 资质适用能力关联集合名。
const SUPPLIER_QUALIFICATION_CAPABILITIES: &str =
    <mongodb::Database as SupplierExt>::SUPPLIER_QUALIFICATION_CAPABILITIES;

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
        filter
    }
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

    /// 按供应商编号查找供应商角色（编号全局唯一，由 `uk_supplier_accounts_supplier_no`
    /// 保证）。
    ///
    /// # 参数
    /// * `supplier_no` - 供应商编号
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的未删除供应商角色；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_supplier_no(
        &self,
        supplier_no: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierAccount>> {
        self.find_one(doc! { "supplier_no": supplier_no }, executor).await
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

/// 商务结算版本列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierCommercialProfileRow {
    /// 实体主键。
    pub id: String,
    /// 供应商角色 ID。
    pub supplier_id: SupplierAccountId,
    /// 商务版本号。
    pub revision_no: u32,
    /// 结算方式。
    pub settlement_mode: String,
    /// 对账周期。
    pub reconciliation_cycle: String,
    /// 付款条件快照。
    pub payment_term_snapshot: String,
    /// 发票类型。
    pub invoice_type: String,
    /// 变更原因。
    pub change_reason: String,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
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
    /// 分页检索商务结算版本列表（投影查询，§6.2 历史查询）。
    ///
    /// 只返回 [`SupplierCommercialProfileRow`] 所需的列表字段；排序字段经仓储
    /// 白名单校验（`created_at`/`revision_no`），非法字段回落默认 `created_at`。
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
    pub async fn search_commercial_profiles(
        &self,
        filter: &SupplierCommercialProfileFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SupplierCommercialProfileRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
                &["created_at", "revision_no"],
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(commercial_profile_projection())
            .build();
        let collection = self
            .collection()
            .clone_with_type::<SupplierCommercialProfileRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 按「供应商 + 修订序号」查找商务结算版本。
    ///
    /// 唯一性由 `uk_supplier_commercial_profile_revisions_supplier_revision`
    /// 唯一索引保证；本方法用于版本定位与历史查询。
    ///
    /// # 参数
    /// * `supplier_id` - 供应商角色 ID
    /// * `revision_no` - 修订序号
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的版本；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_supplier_and_revision(
        &self,
        supplier_id: &SupplierAccountId,
        revision_no: u32,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierCommercialProfileRevision>> {
        self.find_one(
            doc! {
                "supplier_id": supplier_id.to_string(),
                "revision_no": revision_no as i32,
            },
            executor,
        )
        .await
    }

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

/// 供应商能力列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierCapabilityRow {
    /// 实体主键。
    pub id: String,
    /// 供应商角色 ID。
    pub supplier_id: SupplierAccountId,
    /// 能力代码。
    pub capability_code: CapabilityCode,
    /// 服务区域。
    pub service_region: Option<String>,
    /// 负责人。
    pub owner_user_id: String,
    /// 履约说明。
    pub fulfillment_note: Option<String>,
    /// 生效开始日期。
    pub valid_from: String,
    /// 生效结束日期。
    pub valid_to: Option<String>,
    /// 启停状态。
    pub status: CapabilityStatus,
    /// 当前能力修订。
    pub current_revision_id: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
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
    /// 分页检索供应商能力列表（投影查询）。
    ///
    /// 排序字段经仓储白名单校验（`created_at`/`capability_code`/`valid_to`），
    /// 非法字段回落默认 `created_at`。
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
    pub async fn search_supplier_capabilities(
        &self,
        filter: &SupplierCapabilityFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SupplierCapabilityRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
                &["created_at", "capability_code", "valid_to"],
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(supplier_capability_projection())
            .build();
        let collection = self.collection().clone_with_type::<SupplierCapabilityRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

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
}

/// 供应商资质列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierQualificationRow {
    /// 实体主键。
    pub id: String,
    /// 供应商角色 ID。
    pub supplier_id: SupplierAccountId,
    /// 资质类型。
    pub qualification_type: QualificationType,
    /// 证书编号。
    pub certificate_no: String,
    /// 发证机构。
    pub issuer: Option<String>,
    /// 生效开始日期。
    pub valid_from: String,
    /// 生效结束日期。
    pub valid_to: Option<String>,
    /// 资质附件 ID。
    pub attachment_id: Option<String>,
    /// 资质状态。
    pub status: QualificationStatus,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
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
    /// 分页检索供应商资质列表（投影查询）。
    ///
    /// 排序字段经仓储白名单校验（`created_at`/`valid_to`/`status`），非法
    /// 字段回落默认 `created_at`。
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
    pub async fn search_supplier_qualifications(
        &self,
        filter: &SupplierQualificationFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SupplierQualificationRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
                &["created_at", "valid_to", "status"],
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(supplier_qualification_projection())
            .build();
        let collection = self.collection().clone_with_type::<SupplierQualificationRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

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

    /// 读取一项能力关联的全部资质关系。
    ///
    /// # Errors
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_by_capability_id(
        &self,
        capability_id: &SupplierCapabilityId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierQualificationCapability>> {
        self.find_many(doc! { "capability_id": capability_id.to_string() }, executor)
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

/// D09 域专用仓储：跨集合、多步骤且必须位于事务内的聚合写入。
///
/// 单一集合 CRUD 使用 [`Repository`] 基类；本类型只承载依赖事务的
/// 跨集合原子写入入口，由 `SupplierExt::supplier()` 访问。
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

/// 商务结算版本列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn commercial_profile_projection() -> Document {
    doc! {
        "id": 1,
        "supplier_id": 1,
        "revision_no": 1,
        "settlement_mode": 1,
        "reconciliation_cycle": 1,
        "payment_term_snapshot": 1,
        "invoice_type": 1,
        "change_reason": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 供应商能力列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn supplier_capability_projection() -> Document {
    doc! {
        "id": 1,
        "supplier_id": 1,
        "capability_code": 1,
        "service_region": 1,
        "owner_user_id": 1,
        "fulfillment_note": 1,
        "valid_from": 1,
        "valid_to": 1,
        "status": 1,
        "current_revision_id": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 供应商资质列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn supplier_qualification_projection() -> Document {
    doc! {
        "id": 1,
        "supplier_id": 1,
        "qualification_type": 1,
        "certificate_no": 1,
        "issuer": 1,
        "valid_from": 1,
        "valid_to": 1,
        "attachment_id": 1,
        "status": 1,
        "version": 1,
        "created_at": 1,
    }
}

#[cfg(test)]
mod tests {
    use super::{sort_doc, QueryFilter, SupplierCapabilityFilter, SupplierQualificationFilter};
    use entities::supplier::{CapabilityStatus, QualificationStatus};
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
