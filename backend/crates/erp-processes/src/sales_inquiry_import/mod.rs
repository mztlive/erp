//! 客户选品工作簿：有界解析、逐行预览和原子确认。

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

use erp_identity::SharedRbacService;
use mongodb::Database;
pub use views::{ConfirmInquiryImportCommand, CreateInquiryImportCommand, InquiryImportPreviewView};

/// 仅通过当前询价销售范围访问导入文件、预览和确认结果。
#[derive(Clone)]
pub struct SalesInquiryImportProcess {
    db: Database,
    rbac: SharedRbacService,
}

impl SalesInquiryImportProcess {
    /// 绑定数据库及授权源。
    /// # 参数
    /// `db` 为业务数据库，`rbac` 为当前权限服务。
    /// # 返回
    /// 未读取业务事实的导入流程。
    /// # 错误
    /// 无。
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self { db, rbac }
    }
}
