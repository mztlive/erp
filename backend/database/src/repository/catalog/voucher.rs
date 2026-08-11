use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use serde::{Deserialize, Serialize};

use entities::catalog::{EnableStatus, VoucherCategoryProfileRevision};

use super::super::{PageResult, Pagination, QueryFilter, Repository};
use super::shared::sort_doc;
use crate::executor::Executor;
use crate::{mongo_ops, Result};

/// 卡券类目扩展修订列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VoucherCategoryProfileRevisionRow {
    /// 实体主键。
    pub id: String,
    /// 卡券类目使用的 VOUCHER SKU 稳定身份。
    pub sku_id: String,
    /// 修订序号。
    pub revision_no: u32,
    /// 卡券类目描述。
    pub description: String,
    /// 启停状态。
    pub status: EnableStatus,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 卡券类目扩展修订列表筛选条件（修订表追加写入，无软删除过滤）。
#[derive(Debug, Clone)]
pub struct VoucherCategoryProfileRevisionFilter {
    /// 卡券类目 SKU；`None` 表示不筛选。
    pub sku_id: Option<String>,
    /// 启停状态；`None` 表示不筛选。
    pub status: Option<EnableStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单：`created_at`/`revision_no`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for VoucherCategoryProfileRevisionFilter {
    /// 转换为 MongoDB 查询条件（修订表不参与软删除）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(sku_id) = &self.sku_id {
            filter.insert("sku_id", sku_id);
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for VoucherCategoryProfileRevisionFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, VoucherCategoryProfileRevision> {
    /// 分页检索卡券类目扩展修订列表（投影查询）。
    ///
    /// 只返回 [`VoucherCategoryProfileRevisionRow`] 所需的列表字段；排序字段
    /// 白名单化（`created_at`/`revision_no`）。
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
    pub async fn search_voucher_category_profile_revisions(
        &self,
        filter: &VoucherCategoryProfileRevisionFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<VoucherCategoryProfileRevisionRow>> {
        let options = FindOptions::builder()
            .sort(voucher_revision_sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(voucher_revision_projection())
            .build();
        let collection = self
            .collection()
            .clone_with_type::<VoucherCategoryProfileRevisionRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }
}

/// 构建卡券类目修订排序文档（白名单：`created_at`/`revision_no`）。
fn voucher_revision_sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    let field = match sort_by {
        Some("revision_no") => "revision_no",
        _ => "created_at",
    };
    sort_doc(field, sort_ascending)
}

/// 卡券类目扩展修订列表投影字段。
fn voucher_revision_projection() -> Document {
    doc! {
        "id": 1,
        "sku_id": 1,
        "revision_no": 1,
        "description": 1,
        "status": 1,
        "version": 1,
        "created_at": 1,
    }
}
