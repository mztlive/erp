//! 域 D07 `party` 仓储：party、party_revision、party_contact、party_address、
//! party_tax_profile、party_bank_account（数据模型 §6.2）。
//!
//! 单一集合 CRUD 与乐观锁直接复用 [`Repository`] 基类（base.rs：
//! `update`/`soft_delete`/`restore` 比较 `id + version` 做 CAS）；本文件只补充
//! 域特有查询与跨集合多步骤写入入口。`party` 是稳定基础资料（可软删除，
//! 身份类字段全局唯一），`party_revision` 是不可变修订（追加式、**不提供**
//! 软删除），联系人/地址/税务资料/银行账户是支持有效期的从属事实行。
//!
//! 集合名常量统一从 `PartyExt` 关联常量导入（唯一权威来源）；筛选/行类型
//! 定义在本文件，经 `PartyExt` 的关联类型对外暴露。

use entities::ids::PartyId;
use entities::party::{
    AddressType, EffectiveRecordStatus, Party, PartyAddress, PartyBankAccount, PartyContact, PartyKind,
    PartyRevision, PartyStatus, PartyTaxProfile,
};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use mongodb::Database;
use serde::{Deserialize, Serialize};

use super::extensions::PartyExt;
use super::regex_filter::insert_literal_regex_filter;
use super::{PageResult, Pagination, QueryFilter, Repository};
use crate::executor::Executor;
use crate::{mongo_ops, Result};

/// `party` 集合名（单一来源：`PartyExt` 关联常量）。
const PARTIES: &str = <mongodb::Database as PartyExt>::PARTIES;
/// `party_revision` 集合名（单一来源：`PartyExt` 关联常量）。
const PARTY_REVISIONS: &str = <mongodb::Database as PartyExt>::PARTY_REVISIONS;

/// 主体列表投影行（列表接口只取必要字段，禁止返回整文档）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PartyRow {
    /// 实体主键。
    pub id: String,
    /// 主体编号。
    pub party_no: String,
    /// 主体类型。
    pub party_kind: PartyKind,
    /// 统一社会信用代码（非空值规范化后全局唯一）。
    pub unified_credit_code: Option<String>,
    /// 启停状态。
    pub status: PartyStatus,
    /// 当前生效修订 ID。
    pub current_revision_id: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 主体列表筛选条件。
#[derive(Debug, Clone)]
pub struct PartyFilter {
    /// 主体编号模糊匹配（字面量正则，忽略大小写）；`None` 表示不筛选。
    pub keyword: Option<String>,
    /// 主体类型；`None` 表示不筛选。
    pub party_kind: Option<PartyKind>,
    /// 启停状态；`None` 表示不筛选。
    pub status: Option<PartyStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（仓储白名单，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for PartyFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        insert_literal_regex_filter(&mut filter, "party_no", self.keyword.as_deref());
        if let Some(party_kind) = self.party_kind {
            filter.insert("party_kind", party_kind.as_str());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for PartyFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, Party> {
    /// 分页检索主体列表（投影查询）。
    ///
    /// 只返回 [`PartyRow`] 所需的列表字段，不加载整文档；排序字段经仓储
    /// 白名单校验，非法字段回落默认 `created_at`。
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
    pub async fn search_parties(
        &self,
        filter: &PartyFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<PartyRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
                &["created_at", "party_no", "status"],
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(party_projection())
            .build();
        let collection = self.collection().clone_with_type::<PartyRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 按主体编号查找主体（编号全局唯一，由 `uk_parties_party_no` 保证）。
    ///
    /// # 参数
    /// * `party_no` - 主体编号
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的未删除主体；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_party_no(
        &self,
        party_no: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<Party>> {
        self.find_one(doc! { "party_no": party_no }, executor).await
    }

    /// 按主体编号查找主体，包含已软删除记录。
    ///
    /// 全局唯一索引包含软删除记录；编号占用校验必须使用本方法，避免
    /// 仅查未删除记录时误判为可用。
    ///
    /// # 参数
    /// * `party_no` - 主体编号
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的主体（含已软删除）；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_party_no_including_deleted(
        &self,
        party_no: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<Party>> {
        mongo_ops::find_one(&self.collection(), doc! { "party_no": party_no }, executor).await
    }

    /// 按统一社会信用代码查找主体，包含已软删除记录。
    ///
    /// 部分唯一索引 `uk_parties_credit_code` 仅约束非空代码且包含软删除；
    /// 信用代码占用校验必须使用本方法。
    ///
    /// # 参数
    /// * `unified_credit_code` - 已规范化的统一社会信用代码
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的主体（含已软删除）；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_unified_credit_code_including_deleted(
        &self,
        unified_credit_code: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<Party>> {
        mongo_ops::find_one(
            &self.collection(),
            doc! { "unified_credit_code": unified_credit_code },
            executor,
        )
        .await
    }
}

/// 主体修订列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PartyRevisionRow {
    /// 实体主键。
    pub id: String,
    /// 稳定主体 ID。
    pub party_id: PartyId,
    /// 修订序号。
    pub revision_no: u32,
    /// 法定名称。
    pub legal_name: String,
    /// 简称。
    pub short_name: Option<String>,
    /// 变更原因。
    pub change_reason: String,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 主体修订列表筛选条件。
#[derive(Debug, Clone)]
pub struct PartyRevisionFilter {
    /// 稳定主体 ID；`None` 表示不筛选。
    pub party_id: Option<PartyId>,
    /// 法定名称模糊匹配（字面量正则，忽略大小写）；`None` 表示不筛选。
    pub legal_name: Option<String>,
    /// 简称模糊匹配（字面量正则，忽略大小写）；`None` 表示不筛选。
    pub short_name: Option<String>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（仓储白名单，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for PartyRevisionFilter {
    /// 转换为 MongoDB 查询条件（修订集合无软删除，过滤条件为空时仍显式
    /// 追加未删除过滤，与基类语义保持一致）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(party_id) = &self.party_id {
            filter.insert("party_id", party_id.to_string());
        }
        insert_literal_regex_filter(&mut filter, "legal_name", self.legal_name.as_deref());
        insert_literal_regex_filter(&mut filter, "short_name", self.short_name.as_deref());
        filter
    }
}

impl Pagination for PartyRevisionFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, PartyRevision> {
    /// 分页检索主体修订列表（投影查询）。
    ///
    /// 只返回 [`PartyRevisionRow`] 所需的列表字段；排序字段经仓储白名单
    /// 校验（`created_at`/`revision_no`），非法字段回落默认 `created_at`。
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
    pub async fn search_party_revisions(
        &self,
        filter: &PartyRevisionFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<PartyRevisionRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
                &["created_at", "revision_no"],
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(party_revision_projection())
            .build();
        let collection = self.collection().clone_with_type::<PartyRevisionRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 按「稳定主体 + 修订序号」查找修订。
    ///
    /// 唯一性由 `uk_party_revisions_party_revision` 唯一索引保证；本方法用于
    /// 修订幂等写入前的定位与历史查询。
    ///
    /// # 参数
    /// * `party_id` - 稳定主体 ID
    /// * `revision_no` - 修订序号
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的修订；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_party_and_revision(
        &self,
        party_id: &PartyId,
        revision_no: u32,
        executor: &mut dyn Executor,
    ) -> Result<Option<PartyRevision>> {
        self.find_one(
            doc! {
                "party_id": party_id.to_string(),
                "revision_no": revision_no as i32,
            },
            executor,
        )
        .await
    }

    /// 检索某主体的完整修订历史（按 `revision_no` 升序，§6.2 历史查询）。
    ///
    /// # 参数
    /// * `party_id` - 稳定主体 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回该主体的全部修订（修订集合追加式写入，无软删除）。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_revision_history(
        &self,
        party_id: &PartyId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PartyRevision>> {
        self.find_many_sorted(
            doc! { "party_id": party_id.to_string() },
            doc! { "revision_no": 1 },
            executor,
        )
        .await
    }
}

/// 联系人列表投影行。
///
/// 手机号为敏感值（§4.5.5）：投影**不包含** `mobile_ciphertext` 与
/// `mobile_query_hmac`，列表只返回业务展示字段。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PartyContactRow {
    /// 实体主键。
    pub id: String,
    /// 所属企业主体 ID。
    pub party_id: PartyId,
    /// 联系人姓名。
    pub contact_name: String,
    /// 职务/用途。
    pub title: Option<String>,
    /// 电话。
    pub telephone: Option<String>,
    /// 邮箱。
    pub email: Option<String>,
    /// 生效开始日期。
    pub valid_from: String,
    /// 生效结束日期。
    pub valid_to: Option<String>,
    /// 是否当前默认联系人。
    pub is_default: bool,
    /// 启停状态。
    pub status: EffectiveRecordStatus,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 联系人列表筛选条件。
#[derive(Debug, Clone)]
pub struct PartyContactFilter {
    /// 所属企业主体 ID；`None` 表示不筛选。
    pub party_id: Option<PartyId>,
    /// 联系人姓名模糊匹配（字面量正则，忽略大小写）；`None` 表示不筛选。
    pub keyword: Option<String>,
    /// 手机号查询指纹（精确匹配，禁止明文查询）；`None` 表示不筛选。
    pub mobile_query_hmac: Option<String>,
    /// 启停状态；`None` 表示不筛选。
    pub status: Option<EffectiveRecordStatus>,
    /// 默认标记；`None` 表示不筛选。
    pub is_default: Option<bool>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（仓储白名单，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for PartyContactFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(party_id) = &self.party_id {
            filter.insert("party_id", party_id.to_string());
        }
        insert_literal_regex_filter(&mut filter, "contact_name", self.keyword.as_deref());
        if let Some(hmac) = &self.mobile_query_hmac {
            filter.insert("mobile_query_hmac", hmac);
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        if let Some(is_default) = self.is_default {
            filter.insert("is_default", is_default);
        }
        filter
    }
}

impl Pagination for PartyContactFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, PartyContact> {
    /// 分页检索联系人列表（投影查询，敏感字段不进投影）。
    ///
    /// 排序字段经仓储白名单校验（`created_at`/`contact_name`/`valid_from`），
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
    pub async fn search_party_contacts(
        &self,
        filter: &PartyContactFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<PartyContactRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
                &["created_at", "contact_name", "valid_from"],
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(party_contact_projection())
            .build();
        let collection = self.collection().clone_with_type::<PartyContactRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }
}

/// 地址列表投影行。
///
/// 履约地址等地址内容为敏感值（§4.5.5）：投影**不包含** `address_ciphertext`
/// 与 `address_query_hmac`。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PartyAddressRow {
    /// 实体主键。
    pub id: String,
    /// 所属企业主体 ID。
    pub party_id: PartyId,
    /// 地址类型。
    pub address_type: AddressType,
    /// 联系人。
    pub contact_name: Option<String>,
    /// 生效开始日期。
    pub valid_from: String,
    /// 生效结束日期。
    pub valid_to: Option<String>,
    /// 是否当前默认地址。
    pub is_default: bool,
    /// 启停状态。
    pub status: EffectiveRecordStatus,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 地址列表筛选条件。
#[derive(Debug, Clone)]
pub struct PartyAddressFilter {
    /// 所属企业主体 ID；`None` 表示不筛选。
    pub party_id: Option<PartyId>,
    /// 地址类型；`None` 表示不筛选。
    pub address_type: Option<AddressType>,
    /// 启停状态；`None` 表示不筛选。
    pub status: Option<EffectiveRecordStatus>,
    /// 默认标记；`None` 表示不筛选。
    pub is_default: Option<bool>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（仓储白名单，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for PartyAddressFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(party_id) = &self.party_id {
            filter.insert("party_id", party_id.to_string());
        }
        if let Some(address_type) = self.address_type {
            filter.insert("address_type", address_type.as_str());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        if let Some(is_default) = self.is_default {
            filter.insert("is_default", is_default);
        }
        filter
    }
}

impl Pagination for PartyAddressFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, PartyAddress> {
    /// 分页检索地址列表（投影查询，敏感字段不进投影）。
    ///
    /// 排序字段经仓储白名单校验（`created_at`/`address_type`/`valid_from`），
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
    pub async fn search_party_addresses(
        &self,
        filter: &PartyAddressFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<PartyAddressRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
                &["created_at", "address_type", "valid_from"],
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(party_address_projection())
            .build();
        let collection = self.collection().clone_with_type::<PartyAddressRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }
}

/// 税务资料列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PartyTaxProfileRow {
    /// 实体主键。
    pub id: String,
    /// 所属企业主体 ID。
    pub party_id: PartyId,
    /// 纳税人识别号。
    pub tax_no: String,
    /// 生效开始日期。
    pub valid_from: String,
    /// 生效结束日期。
    pub valid_to: Option<String>,
    /// 是否当前默认税务资料。
    pub is_default: bool,
    /// 启停状态。
    pub status: EffectiveRecordStatus,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 税务资料列表筛选条件。
#[derive(Debug, Clone)]
pub struct PartyTaxProfileFilter {
    /// 所属企业主体 ID；`None` 表示不筛选。
    pub party_id: Option<PartyId>,
    /// 启停状态；`None` 表示不筛选。
    pub status: Option<EffectiveRecordStatus>,
    /// 默认标记；`None` 表示不筛选。
    pub is_default: Option<bool>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（仓储白名单，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for PartyTaxProfileFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(party_id) = &self.party_id {
            filter.insert("party_id", party_id.to_string());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        if let Some(is_default) = self.is_default {
            filter.insert("is_default", is_default);
        }
        filter
    }
}

impl Pagination for PartyTaxProfileFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, PartyTaxProfile> {
    /// 分页检索税务资料列表（投影查询）。
    ///
    /// 排序字段经仓储白名单校验（`created_at`/`tax_no`/`valid_from`），
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
    pub async fn search_party_tax_profiles(
        &self,
        filter: &PartyTaxProfileFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<PartyTaxProfileRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
                &["created_at", "tax_no", "valid_from"],
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(party_tax_profile_projection())
            .build();
        let collection = self.collection().clone_with_type::<PartyTaxProfileRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }
}

/// 银行账户列表投影行。
///
/// 账号为敏感值（§4.5.5）：投影**不包含** `account_number_ciphertext` 与
/// `account_number_query_hmac`。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PartyBankAccountRow {
    /// 实体主键。
    pub id: String,
    /// ERP 内部稳定账户编号。
    pub bank_account_no: String,
    /// 所属企业主体 ID。
    pub party_id: PartyId,
    /// 户名。
    pub account_name: String,
    /// 银行。
    pub bank_name: String,
    /// 支行。
    pub bank_branch_name: Option<String>,
    /// 生效开始日期。
    pub valid_from: String,
    /// 生效结束日期。
    pub valid_to: Option<String>,
    /// 是否当前默认账户。
    pub is_default: bool,
    /// 启停状态。
    pub status: EffectiveRecordStatus,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 银行账户列表筛选条件。
#[derive(Debug, Clone)]
pub struct PartyBankAccountFilter {
    /// 所属企业主体 ID；`None` 表示不筛选。
    pub party_id: Option<PartyId>,
    /// 启停状态；`None` 表示不筛选。
    pub status: Option<EffectiveRecordStatus>,
    /// 默认标记；`None` 表示不筛选。
    pub is_default: Option<bool>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（仓储白名单，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for PartyBankAccountFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(party_id) = &self.party_id {
            filter.insert("party_id", party_id.to_string());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        if let Some(is_default) = self.is_default {
            filter.insert("is_default", is_default);
        }
        filter
    }
}

impl Pagination for PartyBankAccountFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, PartyBankAccount> {
    /// 分页检索银行账户列表（投影查询，敏感字段不进投影）。
    ///
    /// 排序字段经仓储白名单校验（`created_at`/`bank_account_no`/`valid_from`），
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
    pub async fn search_party_bank_accounts(
        &self,
        filter: &PartyBankAccountFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<PartyBankAccountRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
                &["created_at", "bank_account_no", "valid_from"],
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(party_bank_account_projection())
            .build();
        let collection = self.collection().clone_with_type::<PartyBankAccountRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 按 ERP 内部稳定账户编号查找账户（编号全局唯一，`uk` 索引保证）。
    ///
    /// # 参数
    /// * `bank_account_no` - 账户编号
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的账户；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_bank_account_no(
        &self,
        bank_account_no: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<PartyBankAccount>> {
        self.find_one(doc! { "bank_account_no": bank_account_no }, executor)
            .await
    }

    /// 按「主体 + 账号查询指纹」查找账户（重复校验只使用 keyed HMAC，
    /// §6.2；唯一性由 `uk_party_bank_accounts_party_hmac` 保证）。
    ///
    /// # 参数
    /// * `party_id` - 所属企业主体 ID
    /// * `account_number_query_hmac` - 规范化账号的查询指纹
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的账户；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_account_hmac(
        &self,
        party_id: &PartyId,
        account_number_query_hmac: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<PartyBankAccount>> {
        self.find_one(
            doc! {
                "party_id": party_id.to_string(),
                "account_number_query_hmac": account_number_query_hmac,
            },
            executor,
        )
        .await
    }
}

/// D07 域专用仓储：跨集合、多步骤且必须位于事务内的聚合写入。
///
/// 单一集合 CRUD 使用 [`Repository`] 基类；本类型只承载依赖事务的
/// 跨集合原子写入入口，由 `PartyExt::party()` 访问。
pub struct PartyRepository<'a> {
    db: &'a Database,
}

impl<'a> PartyRepository<'a> {
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

    /// 追加主体修订并切换当前生效版本（跨集合多步骤写入）。
    ///
    /// 依次写入 `party_revisions` 并 CAS 更新 `parties.current_revision_id`
    /// （基类乐观锁按 `id + version` 判定），保证「修订 + 生效指针」原子可见
    /// （数据模型 §6.2 稳定基础资料 + 不可变修订）。
    /// **必须收到事务执行器**：本方法不构成原子边界，传入 `NoTransaction`
    /// 时修订先各自提交，后续主体版本冲突会留下「新修订存在但主体指针未更新」
    /// 的半成品；Service 必须通过 `database::Transactional::with_transaction`
    /// 传入事务会话。
    ///
    /// # 参数
    /// * `party` - 待更新生效指针的主体（按当前版本做 CAS）
    /// * `revision` - 待写入的修订
    /// * `updated_by` - 本次变更执行人
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当修订违反 `(party_id, revision_no)` 唯一索引（透出
    /// [`crate::Error::DuplicateKey`]）、主体版本冲突（返回
    /// [`crate::Error::OptimisticLockingError`]）或 MongoDB 写入失败时返回错误。
    pub async fn append_party_revision(
        &self,
        party: &mut Party,
        revision: &PartyRevision,
        updated_by: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self.db.collection::<PartyRevision>(PARTY_REVISIONS),
            revision,
            executor,
        )
        .await?;
        party.stable.current_revision_id = Some(revision.base.id.clone());
        party.stable.touch(updated_by);
        Repository::new(self.db, PARTIES).update(party, executor).await
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

/// 主体列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn party_projection() -> Document {
    doc! {
        "id": 1,
        "party_no": 1,
        "party_kind": 1,
        "unified_credit_code": 1,
        "status": 1,
        "current_revision_id": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 主体修订列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn party_revision_projection() -> Document {
    doc! {
        "id": 1,
        "party_id": 1,
        "revision_no": 1,
        "legal_name": 1,
        "short_name": 1,
        "change_reason": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 联系人列表投影字段（不含敏感字段）。
///
/// # 返回
/// 返回投影条件文档。
fn party_contact_projection() -> Document {
    doc! {
        "id": 1,
        "party_id": 1,
        "contact_name": 1,
        "title": 1,
        "telephone": 1,
        "email": 1,
        "valid_from": 1,
        "valid_to": 1,
        "is_default": 1,
        "status": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 地址列表投影字段（不含敏感字段）。
///
/// # 返回
/// 返回投影条件文档。
fn party_address_projection() -> Document {
    doc! {
        "id": 1,
        "party_id": 1,
        "address_type": 1,
        "contact_name": 1,
        "valid_from": 1,
        "valid_to": 1,
        "is_default": 1,
        "status": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 税务资料列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn party_tax_profile_projection() -> Document {
    doc! {
        "id": 1,
        "party_id": 1,
        "tax_no": 1,
        "valid_from": 1,
        "valid_to": 1,
        "is_default": 1,
        "status": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 银行账户列表投影字段（不含敏感字段）。
///
/// # 返回
/// 返回投影条件文档。
fn party_bank_account_projection() -> Document {
    doc! {
        "id": 1,
        "bank_account_no": 1,
        "party_id": 1,
        "account_name": 1,
        "bank_name": 1,
        "bank_branch_name": 1,
        "valid_from": 1,
        "valid_to": 1,
        "is_default": 1,
        "status": 1,
        "version": 1,
        "created_at": 1,
    }
}

#[cfg(test)]
mod tests {
    use super::{sort_doc, PartyFilter, QueryFilter};
    use entities::party::{PartyKind, PartyStatus};
    use mongodb::bson::doc;

    #[test]
    fn party_filter_applies_keyword_regex_and_status() {
        let filter = PartyFilter {
            keyword: Some("P-20".to_string()),
            party_kind: Some(PartyKind::Enterprise),
            status: Some(PartyStatus::Active),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        assert_eq!(document.get_str("party_kind").unwrap(), "enterprise");
        assert_eq!(document.get_str("status").unwrap(), "active");
        let keyword = document.get_document("party_no").unwrap();
        assert_eq!(keyword.get_str("$regex").unwrap(), r"P\-20");
        assert_eq!(keyword.get_str("$options").unwrap(), "i");
    }

    #[test]
    fn sort_doc_falls_back_to_created_at_when_field_is_not_whitelisted() {
        assert_eq!(
            sort_doc(Some("revised_at"), false, &["created_at", "party_no"]),
            doc! { "created_at": -1 }
        );
        assert_eq!(
            sort_doc(Some("party_no"), true, &["created_at", "party_no"]),
            doc! { "party_no": 1 }
        );
        assert_eq!(sort_doc(None, false, &["created_at"]), doc! { "created_at": -1 });
    }
}
