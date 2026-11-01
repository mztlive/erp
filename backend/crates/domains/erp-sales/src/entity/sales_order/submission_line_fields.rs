//! 从销售提交快照恢复商品与卡券字段组。

use erp_core::Result;

use super::stored_line_fields::StoredLineFields;
use super::{GoodsLineFields, SalesOrderSubmissionLine, VoucherLineDraft};

impl SalesOrderSubmissionLine {
    /// 还原实物及服务字段组。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回用于正式版本行构造的完整实物及服务字段组。
    ///
    /// # 错误
    /// 行类型不是实物及服务，或任一必填字段缺失时返回错误。
    pub fn goods_fields(&self) -> Result<GoodsLineFields> {
        StoredLineFields::from(self).goods()
    }

    /// 还原卡券字段组。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回用于正式版本行构造的完整卡券字段组。
    ///
    /// # 错误
    /// 行类型不是卡券，或任一必填字段缺失时返回错误。
    pub fn voucher_fields(&self) -> Result<VoucherLineDraft> {
        StoredLineFields::from(self).voucher()
    }
}
