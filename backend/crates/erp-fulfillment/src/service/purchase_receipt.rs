//! 采购入库单的查询、草稿准备与调用方事务内本域写入。
use erp_core::ids::PurchaseReceiptId;
use id_generator::next_id;
use persistence_core::{Executor, NoTransaction};
use validator::Validate;

use super::FulfillmentService;
use super::purchase_receipt_lines::receipt_line_specs;
use crate::dto::{
    CreatePurchaseReceiptRequest, PurchaseReceiptDetailView, PurchaseReceiptLineView,
    PurchaseReceiptListParams, PurchaseReceiptView, SortDir, UpdatePurchaseReceiptRequest,
};
use crate::entity::fulfillment::{
    PurchaseReceipt, PurchaseReceiptData, PurchaseReceiptLine, PurchaseReceiptLineBatch,
};
use crate::repository::FulfillmentExt;
use crate::{Error, Result};
/// 采购入库单列表筛选条件类型（经 `FulfillmentExt` 关联类型跨 crate 可达）。
type PurchaseReceiptFilter = <mongodb::Database as FulfillmentExt>::PurchaseReceiptFilter;
impl FulfillmentService {
    /// 分页查询采购入库单列表（W01 履约任务作业面）。
    ///
    /// # 参数
    /// * `params` - 查询参数（`purchase_order_id`/`status` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    #[tracing::instrument(
        name = "fulfillment.purchase_receipt_list",
        skip_all,
        fields(layer = "service", domain = "fulfillment", operation = "purchase_receipt_list")
    )]
    pub async fn purchase_receipt_list(
        &self,
        params: &PurchaseReceiptListParams,
    ) -> Result<crate::dto::PageView<PurchaseReceiptView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = PurchaseReceiptFilter {
            purchase_order_id: query.purchase_order_id,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self.db.purchase_receipts().search_purchase_receipts(&filter, &mut NoTransaction).await?;
        super::map_search_page(
            async { Ok(page) },
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
    ///
    /// # 参数
    /// * `id` - 入库单主键
    ///
    /// # 返回
    /// 返回入库单详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 入库单不存在
    /// * `RepositoryError` - 数据库查询失败
    #[tracing::instrument(
        name = "fulfillment.purchase_receipt_detail",
        skip_all,
        fields(layer = "service", domain = "fulfillment", operation = "purchase_receipt_detail")
    )]
    pub async fn purchase_receipt_detail(&self, id: &str) -> Result<PurchaseReceiptDetailView> {
        let receipt = super::find_header_or_not_found(
            self.db.purchase_receipts().find_by_id(id, &mut NoTransaction),
            "采购入库单不存在",
        )
        .await?;
        let lines = self
            .db
            .fulfillment()
            .receipt_lines_by_receipt_ids(&[receipt.base.id.clone().into()], &mut NoTransaction)
            .await?;
        Ok(PurchaseReceiptDetailView {
            receipt: receipt.into(),
            lines: lines.into_iter().map(Into::into).collect(),
        })
    }
    /// 校验并构造采购入库草稿；身份和行顺序保持请求处理时点。
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
    pub async fn prepare_purchase_receipt_update(
        &self,
        id: &str,
        req: UpdatePurchaseReceiptRequest,
    ) -> Result<PurchaseReceipt> {
        req.validate()?;
        let mut receipt = self
            .db
            .purchase_receipts()
            .find_by_id(id, &mut NoTransaction)
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
    /// 从入库单实体构造视图。
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
    /// 从入库行实体构造视图。
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

    /// 创建路径经实体批量工厂派生质量：旧 Service helper 已删除。
    #[test]
    fn receipt_create_uses_entity_batch_factory() {
        let production = include_str!("purchase_receipt.rs").split("#[cfg(test)]").next().expect("生产代码");
        assert!(!production.contains("fn build_receipt_lines"), "旧 helper 必须删除");
        assert!(production.contains("PurchaseReceiptLineBatch::build"), "创建路径必须调用实体工厂");
        assert!(!production.contains("QualityResult::from_quantities"), "质量派生不得留在 Service");
    }
}
