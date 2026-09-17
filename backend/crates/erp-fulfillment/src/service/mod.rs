//! 履约本域查询、草稿准备与事务内事实写入。

pub mod acceptance_eligibility;
pub mod customer_acceptance;
pub mod customer_acceptance_lines;
pub mod customer_acceptance_posting;
pub mod delivery;
pub mod delivery_lines;
pub mod delivery_posting;
pub mod document_number;
pub mod electronic_delivery;
pub mod electronic_delivery_crypto;
pub mod purchase_receipt;
pub mod purchase_receipt_lines;
pub mod purchase_receipt_posting;
pub mod service_fulfillment;
pub mod service_fulfillment_confirm;
pub mod service_fulfillment_crypto;

use mongodb::Database;

use crate::dto::PageView;
use crate::{Error, Result};

/// 履约本域服务；跨域根事务与身份配置由履约流程持有。
pub struct FulfillmentService {
    pub(super) db: Database,
}

impl FulfillmentService {
    /// 使用数据库构造本域查询与事务内写入服务。
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

/// 执行投影分页查询并映射视图行（五类列表共用编排）。
///
/// 各列表方法只保留筛选字段组装与行转视图闭包；查询语义与返回形状不变.
///
/// # 参数
/// * `search` - 投影分页查询 future
/// * `map_row` - 投影行转视图闭包
/// * `page` - 页码（1 起）
/// * `page_size` - 单页条数
///
/// # 返回
/// 返回契约形状的分页视图。
///
/// # 错误
/// 投影查询失败时返回仓储错误。
pub(super) async fn map_search_page<Row, View>(
    search: impl std::future::Future<Output = persistence_core::Result<persistence_core::PageResult<Row>>>,
    map_row: impl FnMut(Row) -> View,
    page: u64,
    page_size: u32,
) -> Result<PageView<View>> {
    let page_result = search.await?;
    Ok(PageView {
        items: page_result.items.into_iter().map(map_row).collect(),
        total: page_result.total,
        page,
        page_size,
    })
}

/// 按主键加载表头，缺失时报领域 `NotFound`（五类详情共用）。
///
/// owned 仓储返回 `persistence_core::Result`，经 `Error::from` 统一转为领域错误.
///
/// # 参数
/// * `header` - 表头主键查询 future
/// * `message` - 表头缺失时的领域提示
///
/// # 返回
/// 返回表头实体。
///
/// # 错误
/// 表头不存在时返回 `NotFound`；查询失败时返回仓储错误。
pub(super) async fn find_header_or_not_found<Header>(
    header: impl std::future::Future<Output = persistence_core::Result<Option<Header>>>,
    message: &str,
) -> Result<Header> {
    header.await.map_err(Error::from)?.ok_or_else(|| Error::NotFound(message.to_string()))
}
