//! 供应商领域独立交接回执集合及仓储入口。

use mongodb::Database;
use persistence_core::Repository;

use crate::entity::handover_receipt::SupplierHandoverReceipt;

/// 供应商及能力交接回执仓储。
pub type SupplierHandoverReceiptRepository<'a> = Repository<'a, SupplierHandoverReceipt>;

/// 供应商领域拥有的交接命令回执访问器。
pub trait SupplierHandoverReceiptExt {
    /// 两类目标由强类型结果及身份中的 resource_type 区分。
    const HANDOVER_COMMAND_RECEIPTS: &'static str = "supplier_handover_command_receipts";
    /// 获取不可变命令回执仓储，查证包含软删除身份。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回当前数据库的仓储。
    /// # 错误
    /// 无；数据库错误由仓储方法返回。
    fn supplier_handover_receipts(&self) -> SupplierHandoverReceiptRepository<'_>;
}

impl SupplierHandoverReceiptExt for Database {
    fn supplier_handover_receipts(&self) -> SupplierHandoverReceiptRepository<'_> {
        Repository::new(self, Self::HANDOVER_COMMAND_RECEIPTS)
    }
}
