//! 销售询价导入预览与永久命令回执集合。

use mongodb::Database;
use persistence_core::Repository;

use crate::entity::sales_inquiry_import::InquiryImportReceipt;
use crate::repository::sales_inquiry_import::InquiryImportPreviewRepository;

/// 导入预览及幂等命令回执访问入口。
pub trait SalesInquiryImportExt {
    const INQUIRY_IMPORT_PREVIEWS: &'static str = "sales_inquiry_import_previews";
    const INQUIRY_IMPORT_RECEIPTS: &'static str = "sales_inquiry_import_receipts";

    /// 返回仅包含客户字段的导入预览仓储。
    /// # 参数
    /// 无。
    /// # 返回
    /// 使用调用方 Executor 的预览仓储。
    /// # 错误
    /// 无。
    fn inquiry_import_previews(&self) -> InquiryImportPreviewRepository<'_>;

    /// 返回不可过期的导入命令回执仓储。
    /// # 参数
    /// 无。
    /// # 返回
    /// 使用调用方 Executor 的回执仓储。
    /// # 错误
    /// 无。
    fn inquiry_import_receipts(&self) -> Repository<'_, InquiryImportReceipt>;
}

impl SalesInquiryImportExt for Database {
    fn inquiry_import_previews(&self) -> InquiryImportPreviewRepository<'_> {
        InquiryImportPreviewRepository::new(self)
    }

    fn inquiry_import_receipts(&self) -> Repository<'_, InquiryImportReceipt> {
        Repository::new(self, Self::INQUIRY_IMPORT_RECEIPTS)
    }
}
