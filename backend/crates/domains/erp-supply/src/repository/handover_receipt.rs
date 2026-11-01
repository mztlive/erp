//! 供给交接回执集合访问器，唯一集合名与领域实体绑定。

use mongodb::Database;
use persistence_core::Repository;

use crate::entity::handover_receipt::OfferingHandoverReceipt;

/// 供给交接独立回执仓储。
pub type OfferingHandoverReceiptRepository<'a> = Repository<'a, OfferingHandoverReceipt>;

/// 供给交接回执的拥有领域访问入口。
pub trait OfferingHandoverReceiptExt {
    /// 独立回执集合名。
    const HANDOVER_COMMAND_RECEIPTS: &'static str = "supplier_offering_handover_command_receipts";
    /// 获取不可变回执仓储；查证须读取包含软删除的原身份。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回当前数据库的回执仓储。
    /// # 错误
    /// 无；I/O 错误由仓储操作返回。
    fn offering_handover_receipts(&self) -> OfferingHandoverReceiptRepository<'_>;
}

impl OfferingHandoverReceiptExt for Database {
    fn offering_handover_receipts(&self) -> OfferingHandoverReceiptRepository<'_> {
        Repository::new(self, Self::HANDOVER_COMMAND_RECEIPTS)
    }
}
