use std::str::FromStr;

use database::{AccessControlExt, FulfillmentExt, NoTransaction, Transactional};
use entities::fulfillment::{
    PurchaseReceipt, PurchaseReceiptData, PurchaseReceiptLine, PurchaseReceiptLineData, QualityResult,
};
use entities::ids::{PurchaseReceiptId, PurchaseReceiptLineId};
use entities::money::Quantity;
use id_generator::next_id;
use validator::Validate;

use crate::audit::AuditActor;
use crate::document_registry::{new_registered_document, persist_registered_document};
use crate::errors::{Error, Result};
use entities::document_registry::DocumentType;

use super::dto::SortDir;
use super::{
    CreatePurchaseReceiptRequest, FulfillmentService, PageView, PurchaseReceiptDetailView,
    PurchaseReceiptLineInput, PurchaseReceiptLineView, PurchaseReceiptListParams, PurchaseReceiptView,
    UpdatePurchaseReceiptRequest,
};

/// 采购入库单列表筛选条件类型（经 `FulfillmentExt` 关联类型跨 crate 可达）。
type PurchaseReceiptFilter = <mongodb::Database as FulfillmentExt>::PurchaseReceiptFilter;

impl FulfillmentService {
    /// 分页查询采购入库单列表（W09 入库视图）。
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
    pub async fn purchase_receipt_list(
        &self,
        params: &PurchaseReceiptListParams,
    ) -> Result<PageView<PurchaseReceiptView>> {
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
        let page = self
            .db
            .purchase_receipts()
            .search_purchase_receipts(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| PurchaseReceiptView {
                id: row.id,
                receipt_no: row.receipt_no,
                purchase_order_id: row.purchase_order_id.to_string(),
                warehouse_id: row.warehouse_id.to_string(),
                status: row.status,
                posted_at: row.posted_at.map(|instant| instant.unix_secs()),
                version: row.version,
                created_at: row.created_at,
            })
            .collect();
        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
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
    pub async fn purchase_receipt_detail(&self, id: &str) -> Result<PurchaseReceiptDetailView> {
        let receipt = self
            .db
            .purchase_receipts()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("采购入库单不存在".to_string()))?;
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

    /// 创建采购入库单（草稿，跨集合：表头 + 行 + 审计）。
    ///
    /// 行的质量结果由服务端按合格/到货关系派生（全部合格/全部不合格/部分合格）。
    ///
    /// # 参数
    /// * `req` - 创建请求（表头 + 行）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建入库单的响应视图。
    ///
    /// # 错误
    /// * `ValidationError` - 请求体校验失败
    /// * `ConflictError` - 单号重复（唯一索引透出）
    /// * `RepositoryError` - 数据库写入失败
    pub async fn create_purchase_receipt(
        &self,
        req: CreatePurchaseReceiptRequest,
        actor: &AuditActor,
    ) -> Result<PurchaseReceiptView> {
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
        let lines = build_receipt_lines(&id, &req.lines)?;
        let audit =
            actor
                .clone()
                .resource_log("purchase_receipt.create", "purchase_receipt", id.to_string())?;
        let document =
            new_registered_document(&id, DocumentType::PurchaseReceipt, receipt.receipt_no.clone())?;
        let db = self.db.clone();
        let client = db.client().clone();
        let receipt_for_tx = receipt.clone();
        let lines_for_tx = lines.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.fulfillment()
                        .create_purchase_receipt_with_lines(&receipt_for_tx, &lines_for_tx, session)
                        .await?;
                    persist_registered_document(&db, &document, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;
        Ok(receipt.into())
    }

    /// 更新采购入库单（仅草稿；乐观锁语义）。
    ///
    /// # 参数
    /// * `id` - 入库单主键
    /// * `req` - 更新请求（含期望版本）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回更新后入库单的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 入库单不存在
    /// * `ConflictError` - 期望版本与当前版本不一致
    /// * `ValidationError` - 请求体校验失败
    pub async fn update_purchase_receipt(
        &self,
        id: &str,
        req: UpdatePurchaseReceiptRequest,
        actor: &AuditActor,
    ) -> Result<PurchaseReceiptView> {
        req.validate()?;
        let mut receipt = self
            .db
            .purchase_receipts()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("采购入库单不存在".to_string()))?;
        if receipt.base.version != req.version {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        receipt.update(entities::fulfillment::PurchaseReceiptUpdate {
            warehouse_id: req.warehouse_id.or(Some(receipt.warehouse_id.clone())),
        })?;
        let audit =
            actor
                .clone()
                .resource_log("purchase_receipt.update", "purchase_receipt", id.to_string())?;
        let db = self.db.clone();
        let client = db.client().clone();
        let updated = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.purchase_receipts().update(&mut receipt, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<PurchaseReceipt, crate::errors::Error>(receipt)
                })
            })
            .await?;
        Ok(updated.into())
    }
}

/// 构建入库行实体集合（行号从 1 递增，质量结果按数量派生）。
///
/// # 参数
/// * `receipt_id` - 入库单主键
/// * `inputs` - 行输入
///
/// # 返回
/// 返回行实体集合。
///
/// # 错误
/// 行数量约束不合法时返回错误（实体构造）。
fn build_receipt_lines(
    receipt_id: &PurchaseReceiptId,
    inputs: &[PurchaseReceiptLineInput],
) -> Result<Vec<PurchaseReceiptLine>> {
    let mut lines = Vec::with_capacity(inputs.len());
    for (index, input) in inputs.iter().enumerate() {
        let line_no = index as u32 + 1;
        let quality_result = derive_quality_result(input);
        lines.push(
            PurchaseReceiptLine::new(
                PurchaseReceiptLineId::new(next_id()),
                PurchaseReceiptLineData {
                    purchase_receipt_id: receipt_id.clone(),
                    line_no,
                    purchase_order_revision_line_id: input.purchase_order_revision_line_id.clone(),
                    received_quantity: input.received_quantity,
                    qualified_quantity: input.qualified_quantity,
                    rejected_quantity: input.rejected_quantity,
                    quality_result,
                },
            )
            .map_err(Error::Logic)?,
        );
    }
    Ok(lines)
}

/// 按合格/到货数量派生质量结果（§6.7：全部合格/全部不合格/部分合格）。
///
/// # 参数
/// * `input` - 行输入
///
/// # 返回
/// 返回质量结果。
fn derive_quality_result(input: &PurchaseReceiptLineInput) -> QualityResult {
    let zero = Quantity::from_str("0").unwrap();
    let qualified = input.qualified_quantity.to_decimal();
    let rejected = input.rejected_quantity.to_decimal();
    if rejected <= zero.to_decimal() {
        QualityResult::Passed
    } else if qualified <= zero.to_decimal() {
        QualityResult::Rejected
    } else {
        QualityResult::Partial
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
    use super::{build_receipt_lines, derive_quality_result};
    use crate::fulfillment::PurchaseReceiptLineInput;
    use entities::fulfillment::{PurchaseReceiptLineData, QualityResult};
    use entities::ids::{PurchaseOrderRevisionLineId, PurchaseReceiptId};
    use entities::money::Quantity;
    use std::str::FromStr;

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
        assert_eq!(derive_quality_result(&passed_line()), QualityResult::Passed);
        let rejected = PurchaseReceiptLineInput {
            qualified_quantity: Quantity::from_str("0").unwrap(),
            rejected_quantity: Quantity::from_str("10").unwrap(),
            ..passed_line()
        };
        assert_eq!(derive_quality_result(&rejected), QualityResult::Rejected);
        let partial = PurchaseReceiptLineInput {
            qualified_quantity: Quantity::from_str("9").unwrap(),
            rejected_quantity: Quantity::from_str("1").unwrap(),
            ..passed_line()
        };
        assert_eq!(derive_quality_result(&partial), QualityResult::Partial);
    }

    #[test]
    fn receipt_lines_are_built_with_incrementing_line_no_and_validation() {
        let lines = build_receipt_lines(
            &PurchaseReceiptId::new("r-1"),
            &[
                passed_line(),
                PurchaseReceiptLineInput {
                    purchase_order_revision_line_id: PurchaseOrderRevisionLineId::new("porl-2"),
                    received_quantity: Quantity::from_str("5").unwrap(),
                    qualified_quantity: Quantity::from_str("5").unwrap(),
                    rejected_quantity: Quantity::from_str("0").unwrap(),
                },
            ],
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
        assert!(build_receipt_lines(&PurchaseReceiptId::new("r-2"), &[over_sum]).is_err());
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
