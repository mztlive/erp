//! 附加费用的来源资格由销售单当前性质和商业状态决定。
use super::{CommercialStatus, SalesOrder};
use crate::entity::sales_order::BusinessType;
use crate::{Error, Result};

impl SalesOrder {
    /// 校验本单可作为附加费用来源；关闭展示态不影响资格。
    /// # 参数
    /// 无。
    /// # 返回
    /// 已生效、未作废的实物与服务销售单返回成功。
    /// # 错误
    /// 卡券、草稿、审批中或作废销售单返回业务错误。
    pub fn ensure_extra_cost_source(&self) -> Result<()> {
        if self.business_type != BusinessType::GoodsService {
            return Err(Error::BusinessLogicError("卡券销售单不登记附加费用".into()));
        }
        if self.commercial_status != CommercialStatus::Effective {
            return Err(Error::BusinessLogicError("仅已生效且未作废的销售单可追加费用".into()));
        }
        Ok(())
    }
}
