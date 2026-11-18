//! 销售草稿续编补齐与审批一致的锁定版本价格。

use erp_sales::dto::sales_order::WorkingCopyView;
use persistence_core::Executor;

use super::SalesOrderReadService;
use crate::Result;
use crate::sales_center::prices::enrich_line_prices;

impl SalesOrderReadService {
    // 组装销售草稿锁定参考价时，为已授权草稿补齐锁定版本价格，不覆盖单据金额。
    //
    // 调用参数：
    // `view` 为已读取的工作副本。
    //
    // 原地补齐参考价和原采购供给价。
    //
    // 批量读取失败时返回错误。
    pub(super) async fn enrich_working_copy_prices(
        &self,
        view: &mut WorkingCopyView,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        enrich_line_prices(&self.db, &mut view.lines, executor).await
    }
}
