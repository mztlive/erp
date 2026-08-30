//! 域 D27 `projection` 服务编排。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 建立投影 + 首个投影版本 + 下发记录：跨集合原子写入（`create_projection_revision`
//!   + 下发记录 + 审计）→ `database::Transactional::with_transaction`；
//! - 推进投影版本 + 下发记录：跨集合原子写入 → 事务；
//! - 投影下发（二期专属，P3 §3/§7）：**外部 HTTP 调用在事务之外完成**——
//!   事务 1 先落 `inbox_message`（Received）+ 审计；事务外经 [`MallConnector`]
//!   尝试下发（超时/重试上限/错误分类）；事务 2 把结果落
//!   `sales_order_projection_delivery` 与 `inbox_message`（Processed/Failed）+
//!   `integration_error_task`（失败时），商城确认后在同一事务推进投影的
//!   `current_acked_revision_id`。
//!
//! 跨域只调对方 Repository（P3 §2）：D13 `sales_orders`/`sales_order_revisions`/
//! `sales_order_revision_lines`/`sales_order_voucher_line_revisions` 由 ERP 销售单
//! 当前版本派生投影白名单快照（面额/卡张数/卡形态/履约期限/生效时间，§6.16）。
//! 业务规则（卡券单必填、唯一卡券行、面额/张数约束）在 entities，Service 只编排。

mod connector;
mod delivery;
mod dto;
mod query;
mod revision;
mod service;

pub use self::connector::{
    ClassifiedError, DeliverAck, MallConnector, QueryProjectionResult, UnavailableMallConnector,
};
pub(crate) use self::dto::projection_content_hash;
pub use self::dto::{
    CreateSalesOrderProjectionRequest, CreateSalesOrderProjectionRevisionRequest,
    DeliverProjectionRevisionRequest, PageView, ProcessProjectionDeliveriesRequest,
    ProcessProjectionDeliveriesResult, ProjectionActionBlockerView, ProjectionBulkAction,
    ProjectionBulkCommandRequest, ProjectionBulkCommandResultView, ProjectionBulkItemResultView,
    ProjectionDeliveryAction, ProjectionDeliveryActionResult, ProjectionDeliveryCommand,
    ProjectionDeliveryResultView, SalesOrderProjectionDeliveryListParams, SalesOrderProjectionDeliveryView,
    SalesOrderProjectionListItemView, SalesOrderProjectionListParams, SalesOrderProjectionRevisionView,
    SalesOrderProjectionView,
};
pub use self::service::ProjectionService;
