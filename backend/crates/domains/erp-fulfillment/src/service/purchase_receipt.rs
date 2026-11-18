//! 采购入库单的查询、草稿准备与调用方事务内本域写入。
use erp_core::ids::{PurchaseReceiptId, PurchaseReceiptLineId};
use id_generator::next_id;
use persistence_core::Executor;
use validator::Validate;

use super::FulfillmentService;
use crate::dto::{
    CreatePurchaseReceiptRequest, PurchaseReceiptDetailView, PurchaseReceiptLineInput,
    PurchaseReceiptLineView, PurchaseReceiptListParams, PurchaseReceiptView, UpdatePurchaseReceiptRequest,
};
use crate::entity::fulfillment::{
    PurchaseReceipt, PurchaseReceiptData, PurchaseReceiptLine, PurchaseReceiptLineBatch,
    PurchaseReceiptLineSpec,
};
use crate::repository::FulfillmentExt;
use crate::repository::prelude::*;
use crate::{Error, Result};
/// 采购入库单列表筛选条件类型（经 `FulfillmentExt` 关联类型跨 crate 可达）。
type PurchaseReceiptFilter = <mongodb::Database as FulfillmentExt>::PurchaseReceiptFilter;
impl FulfillmentService {
    // 分页查询采购入库单列表（W01 履约任务作业面）。
    // 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    // `params` 为查询参数（`purchase_order_id`/`status` 扁平筛选）。
    //
    // 失败条件：
    // * `ValidationError` - 分页参数非法或排序字段不在白名单
    // * `RepositoryError` - 数据库查询失败
    #[tracing::instrument(
        name = "fulfillment.purchase_receipt_list",
        skip_all,
        fields(layer = "service", domain = "fulfillment", operation = "purchase_receipt_list")
    )]
    pub(super) async fn load_purchase_receipts(
        &self,
        params: &PurchaseReceiptListParams,
        executor: &mut dyn Executor,
    ) -> Result<crate::dto::PageView<PurchaseReceiptView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = PurchaseReceiptFilter {
            purchase_order_id: query.purchase_order_id,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: super::sort_ascending(query.paging.sort_dir),
        };

        super::map_search_page(
            self.db.purchase_receipts().search_purchase_receipts(&filter, executor),
            |row| PurchaseReceiptView {
                id: row.id,
                receipt_no: row.receipt_no,
                purchase_order_id: row.purchase_order_id.to_string(),
                warehouse_id: row.warehouse_id.to_string(),
                status: row.status,
                posted_at: row.posted_at.map(|instant| instant.unix_secs()),
                version: row.version,
                created_at: row.created_at,
            },
            filter.page,
            filter.page_size,
        )
        .await
    }
    /// 查询采购入库单详情（表头 + 行）。
    /// 调用方必须已完成所属业务授权，并传入原读取阶段执行器。
    /// 返回入库单详情视图。
    /// `id` 为入库单主键。
    ///
    /// 失败条件：
    /// * `NotFound` - 入库单不存在
    /// * `RepositoryError` - 数据库查询失败
    #[tracing::instrument(
        name = "fulfillment.purchase_receipt_detail",
        skip_all,
        fields(layer = "service", domain = "fulfillment", operation = "purchase_receipt_detail")
    )]
    pub async fn load_purchase_receipt_view(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<PurchaseReceiptDetailView> {
        let receipt = super::find_header_or_not_found(
            self.db.purchase_receipts().find_by_id(id, executor),
            "采购入库单不存在",
        )
        .await?;
        let lines = self
            .db
            .fulfillment()
            .receipt_lines_by_receipt_ids(&[receipt.base.id.clone().into()], executor)
            .await?;

        Ok(PurchaseReceiptDetailView {
            receipt: receipt.into(),
            lines: lines.into_iter().map(Into::into).collect(),
        })
    }
    /// 校验并构造采购入库草稿；身份和行顺序保持请求处理时点。
    /// 返回入库表头及其行。
    /// `req` 为采购入库创建请求。
    /// 请求校验失败时返回 `ValidationError`；表头构造失败时返回对应错误；行批量构造失败时返回 `Logic`。
    pub fn prepare_purchase_receipt(
        req: CreatePurchaseReceiptRequest,
    ) -> Result<(PurchaseReceipt, Vec<PurchaseReceiptLine>)> {
        req.validate()?;

        let id = PurchaseReceiptId::new(next_id());
        let receipt = PurchaseReceipt::new(
            id.clone(),
            PurchaseReceiptData {
                receipt_no: req.receipt_no,
                purchase_order_id: req.purchase_order_id,
                warehouse_id: req.warehouse_id,
            },
        )?;

        let lines = PurchaseReceiptLineBatch::build(id.clone(), receipt_line_specs(&req.lines))
            .map_err(Error::Logic)?;
        Ok((receipt, lines))
    }
    /// 读取并校验草稿更新；版本守卫先于冻结仓库守卫。
    /// 返回已应用更新、尚未写回的草稿。
    /// `id` 为入库单主键；`req` 为带期望版本的更新请求。
    /// `executor` 为调用方授权事务的同一执行器。
    /// 请求校验失败时返回 `ValidationError`；入库单不存在时返回 `NotFound`；版本不一致时返回 `ConflictError`；
    /// 请求变更已冻结仓库时返回 `ValidationError`；领域更新失败时返回对应错误。
    pub async fn prepare_purchase_receipt_update(
        &self,
        id: &str,
        req: UpdatePurchaseReceiptRequest,
        executor: &mut dyn Executor,
    ) -> Result<PurchaseReceipt> {
        req.validate()?;

        let mut receipt = self
            .db
            .purchase_receipts()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("采购入库单不存在".to_string()))?;
        if receipt.base.version != req.version {
            return Err(Error::ConflictError("数据已被其他请求修改，请刷新后重试".to_string()));
        }
        if req.warehouse_id.as_ref().is_some_and(|warehouse_id| warehouse_id != &receipt.warehouse_id) {
            return Err(Error::ValidationError(
                "采购入库单的目标仓库已冻结，不能在任务生成后变更".to_string(),
            ));
        }

        receipt.update(crate::entity::fulfillment::PurchaseReceiptUpdate {
            warehouse_id: req.warehouse_id.or(Some(receipt.warehouse_id.clone())),
        })?;
        Ok(receipt)
    }
    /// 在调用方事务内创建采购入库表头及其行。
    /// 创建成功时无返回值。
    /// `receipt` 为入库表头；`lines` 为入库行。
    /// 沿用调用方当前事务的执行器 `executor`。
    /// 传播仓储写入错误。
    pub async fn persist_created_purchase_receipt(
        &self,
        receipt: &PurchaseReceipt,
        lines: &[PurchaseReceiptLine],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.db.fulfillment().create_purchase_receipt_with_lines(receipt, lines, executor).await?;
        Ok(())
    }
    /// 在调用方事务内按原乐观锁条件写回采购入库单。
    /// `receipt` 为已完成领域修改的入库单；沿用调用方当前事务的执行器 `executor`。
    /// 版本冲突或仓储写入失败时返回对应错误。
    pub async fn persist_purchase_receipt(
        &self,
        receipt: &mut PurchaseReceipt,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.db.purchase_receipts().update(receipt, executor).await?;
        Ok(())
    }
}
impl From<PurchaseReceipt> for PurchaseReceiptView {
    // 从入库单实体构造视图。
    fn from(receipt: PurchaseReceipt) -> Self {
        Self {
            id: receipt.base.id,
            receipt_no: receipt.receipt_no,
            purchase_order_id: receipt.purchase_order_id.to_string(),
            warehouse_id: receipt.warehouse_id.to_string(),
            status: receipt.status,
            posted_at: receipt.posted_at.map(|instant| instant.unix_secs()),
            version: receipt.base.version,
            created_at: receipt.base.created_at,
        }
    }
}

impl From<PurchaseReceiptLine> for PurchaseReceiptLineView {
    // 从入库行实体构造视图。
    fn from(line: PurchaseReceiptLine) -> Self {
        Self {
            id: line.base.id,
            line_no: line.line_no,
            purchase_order_revision_line_id: line.purchase_order_revision_line_id.to_string(),
            received_quantity: line.received_quantity,
            qualified_quantity: line.qualified_quantity,
            rejected_quantity: line.rejected_quantity,
            quality_result: line.quality_result,
        }
    }
}

fn receipt_line_specs(inputs: &[PurchaseReceiptLineInput]) -> Vec<PurchaseReceiptLineSpec> {
    inputs
        .iter()
        .map(|input| PurchaseReceiptLineSpec {
            line_id: PurchaseReceiptLineId::new(next_id()),
            purchase_order_revision_line_id: input.purchase_order_revision_line_id.clone(),
            received_quantity: input.received_quantity,
            qualified_quantity: input.qualified_quantity,
            rejected_quantity: input.rejected_quantity,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::ids::{PurchaseOrderRevisionLineId, PurchaseReceiptId};
    use erp_core::money::Quantity;

    use super::receipt_line_specs;
    use crate::dto::PurchaseReceiptLineInput;
    use crate::entity::fulfillment::{PurchaseReceiptLineBatch, PurchaseReceiptLineData, QualityResult};

    fn passed_line() -> PurchaseReceiptLineInput {
        PurchaseReceiptLineInput {
            purchase_order_revision_line_id: PurchaseOrderRevisionLineId::new("porl-1"),
            received_quantity: Quantity::from_str("10").unwrap(),
            qualified_quantity: Quantity::from_str("10").unwrap(),
            rejected_quantity: Quantity::from_str("0").unwrap(),
        }
    }

    #[test]
    fn quality_result_is_derived_from_quantities() {
        let passed = passed_line();
        assert_eq!(
            QualityResult::from_quantities(passed.qualified_quantity, passed.rejected_quantity),
            QualityResult::Passed
        );

        let rejected = PurchaseReceiptLineInput {
            qualified_quantity: Quantity::from_str("0").unwrap(),
            rejected_quantity: Quantity::from_str("10").unwrap(),
            ..passed_line()
        };
        assert_eq!(
            QualityResult::from_quantities(rejected.qualified_quantity, rejected.rejected_quantity),
            QualityResult::Rejected
        );

        let partial = PurchaseReceiptLineInput {
            qualified_quantity: Quantity::from_str("9").unwrap(),
            rejected_quantity: Quantity::from_str("1").unwrap(),
            ..passed_line()
        };
        assert_eq!(
            QualityResult::from_quantities(partial.qualified_quantity, partial.rejected_quantity),
            QualityResult::Partial
        );
    }

    #[test]
    fn receipt_lines_are_built_with_incrementing_line_no_and_validation() {
        let lines = PurchaseReceiptLineBatch::build(
            PurchaseReceiptId::new("r-1"),
            receipt_line_specs(&[
                passed_line(),
                PurchaseReceiptLineInput {
                    purchase_order_revision_line_id: PurchaseOrderRevisionLineId::new("porl-2"),
                    received_quantity: Quantity::from_str("5").unwrap(),
                    qualified_quantity: Quantity::from_str("5").unwrap(),
                    rejected_quantity: Quantity::from_str("0").unwrap(),
                },
            ]),
        )
        .unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].line_no, 1);
        assert_eq!(lines[1].line_no, 2);

        let over_sum = PurchaseReceiptLineInput {
            qualified_quantity: Quantity::from_str("9.5").unwrap(),
            rejected_quantity: Quantity::from_str("1").unwrap(),
            ..passed_line()
        };
        assert!(
            PurchaseReceiptLineBatch::build(PurchaseReceiptId::new("r-2"), receipt_line_specs(&[over_sum]))
                .is_err()
        );

        let _ = PurchaseReceiptLineData {
            purchase_receipt_id: PurchaseReceiptId::new("r-1"),
            line_no: 1,
            purchase_order_revision_line_id: PurchaseOrderRevisionLineId::new("porl-1"),
            received_quantity: Quantity::from_str("10").unwrap(),
            qualified_quantity: Quantity::from_str("9").unwrap(),
            rejected_quantity: Quantity::from_str("1").unwrap(),
            quality_result: QualityResult::Partial,
        };
    }
}
