//! 从销售变更提交行恢复商品与卡券字段快照。
use super::{GoodsLineFields, Result, SalesChangeSubmissionLine, VoucherLineDraft};
use crate::entity::sales_order::stored_line_fields::StoredLineFields;

impl SalesChangeSubmissionLine {
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
    ///
    /// # 关键业务约束
    /// 可选的福利场景和服务区域缺失时必须保留为 `None`，不得填默认值。
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
    ///
    /// # 关键业务约束
    /// 可选配赠率缺失时必须保留为 `None`，由卡券行实体按公式推导。
    pub fn voucher_fields(&self) -> Result<VoucherLineDraft> {
        StoredLineFields::from(self).voucher().map(Into::into)
    }
}
