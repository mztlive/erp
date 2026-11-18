//! 客户选品工作簿：有界解析、逐行预览和原子确认。

mod access;
mod confirm;
mod creation;
mod creation_preview;
mod facts;
pub(crate) mod parse;
mod pool;
mod pool_delivery;
mod pool_delivery_checks;
mod preview;
mod scalar_cells;
mod views;

use std::sync::Arc;

use erp_catalog::ports::supply::CatalogSupplyQueryPort;
use erp_identity::AuthorizationService;
use erp_order_to_cash::sales_inquiry::SalesInquiryProcess;
use mongodb::Database;
pub use views::{ConfirmInquiryImportCommand, CreateInquiryImportCommand, InquiryImportPreviewView};

/// 仅通过当前询价销售范围访问导入文件、预览和确认结果。
#[derive(Clone)]
pub struct SalesInquiryImportProcess {
    inquiry: Arc<SalesInquiryProcess>,
    db: Database,
    authorization: Arc<AuthorizationService>,
    supply: Arc<dyn CatalogSupplyQueryPort>,
}

impl SalesInquiryImportProcess {
    /// 绑定数据库及授权源。
    ///
    /// `db` 为业务数据库，`authorization` 为当前权限服务，`supply` 为目录事实 Port，`inquiry` 为已装配的询价编排。
    ///
    /// 未读取业务事实的导入流程。
    pub fn new(
        db: Database,
        authorization: Arc<AuthorizationService>,
        supply: Arc<dyn CatalogSupplyQueryPort>,
        inquiry: Arc<SalesInquiryProcess>,
    ) -> Self {
        Self { db, authorization, supply, inquiry }
    }
}

pub use access::preview_facts_reader;
