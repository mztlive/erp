use database::{AccessControlExt, NoTransaction, ProjectionExt, SalesOrderExt, Transactional};
use entities::ids::{
    SalesOrderProjectionDeliveryId, SalesOrderProjectionId, SalesOrderProjectionRevisionId,
    SalesOrderRevisionId,
};
use entities::projection::{
    ProjectionDeliveryStatus, ProjectionSource, SalesOrderProjection, SalesOrderProjectionData,
    SalesOrderProjectionDelivery, SalesOrderProjectionDeliveryData, SalesOrderProjectionRevision,
    SalesOrderProjectionRevisionData,
};
use id_generator::next_id;
use validator::Validate;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};
use crate::projection::dto::{
    CreateSalesOrderProjectionRequest, CreateSalesOrderProjectionRevisionRequest,
    SalesOrderProjectionRevisionView, SalesOrderProjectionView,
};
use crate::projection::service::ProjectionService;

use super::projection_content_hash;

impl ProjectionService {
    /// 建立执行投影及其首个投影版本与下发记录（跨集合事务写入）。
    ///
    /// 投影来源为存量单切换快照（phase-2 §8.5.4）：以 ERP 销售单当前版本作为
    /// 第一份执行投影版本，不产生新的销售单版本。白名单快照（面额/卡张数/
    /// 卡形态/履约期限/生效时间）由销售单当前版本与唯一卡券行派生。
    ///
    /// # 参数
    /// * `req` - 建立请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建投影的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 销售单或当前版本不存在
    /// * `ValidationError` - 非卡券单、卡券行数量不为 1 或商城标识为空
    /// * `ConflictError` - `(sales_order_id, target_mall_id)` 已存在（唯一索引透出）
    pub async fn create_projection(
        &self,
        req: CreateSalesOrderProjectionRequest,
        actor: &AuditActor,
    ) -> Result<SalesOrderProjectionView> {
        req.validate()?;
        let (revision, voucher_line) = self.load_current_sales_revision(&req.sales_order_id).await?;

        let projection = SalesOrderProjection::new(
            SalesOrderProjectionId::new(next_id()),
            SalesOrderProjectionData {
                sales_order_id: req.sales_order_id,
                target_mall_id: req.target_mall_id.clone(),
            },
        )?;
        let projection_revision = SalesOrderProjectionRevision::new(
            SalesOrderProjectionRevisionId::new(next_id()),
            1,
            SalesOrderProjectionRevisionData {
                projection_id: projection.base.id.clone().into(),
                projection_source: ProjectionSource::CutoverSnapshot,
                sales_order_revision_id: revision.base.id.clone().into(),
                customer_external_identity: req.customer_external_identity,
                voucher_category_external_identity: req.voucher_category_external_identity,
                voucher_expiry_at: voucher_expiry(revision.as_ref())?,
                face_value: voucher_line.face_value,
                card_count: voucher_line.card_count,
                card_form: to_projection_card_form(voucher_line.card_form),
                effective_at: revision.effective_at,
                content_hash: "placeholder".to_string(),
            },
        )?;
        let mut projection_revision = projection_revision;
        projection_revision.content_hash = projection_content_hash(&projection_revision);
        let delivery = SalesOrderProjectionDelivery::new(
            SalesOrderProjectionDeliveryId::new(next_id()),
            SalesOrderProjectionDeliveryData {
                projection_revision_id: projection_revision.base.id.clone().into(),
                target_mall_id: projection.target_mall_id.clone(),
                status: ProjectionDeliveryStatus::PendingSend,
                attempt_count: 0,
                next_attempt_at: None,
                mall_ack_at: None,
                mall_execution_baseline: None,
                error_code: None,
                error_summary: None,
            },
        )?;
        let audit = actor.clone().resource_log(
            "sales_order_projection.create",
            "sales_order_projection",
            projection.base.id.clone(),
        )?;

        let db = self.db.clone();
        let client = db.client().clone();
        let projection_tx = projection.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.projection()
                        .create_projection_revision(&projection_tx, &projection_revision, session)
                        .await?;
                    db.sales_order_projection_deliveries()
                        .create(&delivery, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(projection.into())
    }

    /// 推进执行投影版本（后续 ERP 销售版本 + 下发记录，跨集合事务写入）。
    ///
    /// 投影来源 `ErpRevision`：以销售单当前版本派生白名单快照；幂等键
    /// 「ERP 销售单号 + ERP 销售单版本 + 目标商城」由
    /// `(sales_order_revision_id, target_mall_id)` 唯一索引承接（§6.16）。
    ///
    /// # 参数
    /// * `projection_id` - 所属投影稳定身份
    /// * `req` - 推进请求（商城侧标识）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建投影版本的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 投影不存在
    /// * `ValidationError` - 销售单当前版本缺失或非卡券单
    /// * `ConflictError` - 同一销售版本已投影（唯一索引透出）
    pub async fn create_revision(
        &self,
        projection_id: &str,
        req: CreateSalesOrderProjectionRevisionRequest,
        actor: &AuditActor,
    ) -> Result<SalesOrderProjectionRevisionView> {
        req.validate()?;
        let projection = self
            .db
            .sales_order_projections()
            .find_by_id(projection_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("执行投影不存在".to_string()))?;
        let (revision, voucher_line) = self
            .load_current_sales_revision(&projection.sales_order_id)
            .await?;
        let revision_no = self.next_revision_no(projection_id).await?;
        let projection_revision = SalesOrderProjectionRevision::new(
            SalesOrderProjectionRevisionId::new(next_id()),
            revision_no,
            SalesOrderProjectionRevisionData {
                projection_id: projection.base.id.clone().into(),
                projection_source: ProjectionSource::ErpRevision,
                sales_order_revision_id: revision.base.id.clone().into(),
                customer_external_identity: req.customer_external_identity,
                voucher_category_external_identity: req.voucher_category_external_identity,
                voucher_expiry_at: voucher_expiry(revision.as_ref())?,
                face_value: voucher_line.face_value,
                card_count: voucher_line.card_count,
                card_form: to_projection_card_form(voucher_line.card_form),
                effective_at: revision.effective_at,
                content_hash: "placeholder".to_string(),
            },
        )?;
        let mut projection_revision = projection_revision;
        projection_revision.content_hash = projection_content_hash(&projection_revision);
        let delivery = SalesOrderProjectionDelivery::new(
            SalesOrderProjectionDeliveryId::new(next_id()),
            SalesOrderProjectionDeliveryData {
                projection_revision_id: projection_revision.base.id.clone().into(),
                target_mall_id: projection.target_mall_id.clone(),
                status: ProjectionDeliveryStatus::PendingSend,
                attempt_count: 0,
                next_attempt_at: None,
                mall_ack_at: None,
                mall_execution_baseline: None,
                error_code: None,
                error_summary: None,
            },
        )?;
        let audit = actor.clone().resource_log(
            "sales_order_projection_revision.submit",
            "sales_order_projection_revision",
            projection_revision.base.id.clone(),
        )?;

        let db = self.db.clone();
        let client = db.client().clone();
        let projection_revision_tx = projection_revision.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.projection()
                        .create_projection_revision_with_delivery(&projection_revision_tx, &delivery, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(SalesOrderProjectionRevisionView {
            id: projection_revision.base.id,
            projection_id: projection_revision.projection_id.to_string(),
            revision_no: projection_revision.revision.revision_no,
            projection_source: projection_revision.projection_source,
            sales_order_revision_id: projection_revision.sales_order_revision_id.to_string(),
            customer_external_identity: projection_revision.customer_external_identity,
            face_value: projection_revision.face_value,
            card_count: projection_revision.card_count,
            card_form: projection_revision.card_form,
            effective_at: projection_revision.effective_at.unix_secs(),
            version: projection_revision.base.version,
            created_at: projection_revision.base.created_at,
        })
    }

    /// 计算投影的下一个修订序号（当前最大序号 + 1，首个修订为 1）。
    ///
    /// # 参数
    /// * `projection_id` - 所属投影稳定身份
    ///
    /// # 返回
    /// 返回下一个修订序号。
    async fn next_revision_no(&self, projection_id: &str) -> Result<u32> {
        let rows = self
            .db
            .sales_order_projection_revisions()
            .list_revisions_by_projection(
                &SalesOrderProjectionId::new(projection_id.to_string()),
                &mut NoTransaction,
            )
            .await?;
        Ok(rows.first().map(|row| row.revision_no + 1).unwrap_or(1))
    }

    /// 加载销售单当前版本与唯一卡券行（跨域读 D13 仓储）。
    ///
    /// # 参数
    /// * `sales_order_id` - 卡券销售单
    ///
    /// # 返回
    /// 返回 `(销售版本, 卡券行版本)` 元组。
    ///
    /// # 错误
    /// * `NotFound` - 销售单或当前版本不存在
    /// * `ValidationError` - 非卡券单或卡券行数量不为 1
    async fn load_current_sales_revision(
        &self,
        sales_order_id: &entities::ids::SalesOrderId,
    ) -> Result<(
        Box<entities::sales_order::SalesOrderRevision>,
        Box<entities::sales_order::SalesOrderVoucherLineRevision>,
    )> {
        let order = self
            .db
            .sales_orders()
            .find_by_id(sales_order_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售单不存在".to_string()))?;
        let revision_id = order
            .stable
            .current_revision_id
            .ok_or_else(|| Error::ValidationError("销售单尚未形成生效版本，无法建立投影".to_string()))?;
        let revision = self
            .db
            .sales_order_revisions()
            .find_by_id(&revision_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售单当前版本不存在".to_string()))?;
        voucher_expiry(&revision)?;
        let lines = self
            .db
            .sales_order_revision_lines()
            .list_lines_by_revision(
                &SalesOrderRevisionId::new(revision.base.id.clone()),
                &mut NoTransaction,
            )
            .await?;
        let line_ids: Vec<entities::ids::SalesOrderRevisionLineId> = lines
            .iter()
            .map(|line| entities::ids::SalesOrderRevisionLineId::new(line.base.id.clone()))
            .collect();
        let voucher_lines = self
            .db
            .sales_order_voucher_line_revisions()
            .list_by_revision_line_ids(&line_ids, &mut NoTransaction)
            .await?;
        let voucher_line = match voucher_lines.len() {
            1 => voucher_lines.into_iter().next().expect("长度已判 1"),
            _ => {
                return Err(Error::ValidationError(
                    "卡券销售单必须恰好一条卡券行才能建立执行投影".to_string(),
                ))
            }
        };
        Ok((Box::new(revision), Box::new(voucher_line)))
    }
}

/// 校验销售版本为卡券单并返回表头履约期限。
///
/// # 参数
/// * `revision` - 销售版本
///
/// # 返回
/// 返回表头履约期限。
///
/// # 错误
/// 卡券类目与履约期限缺失时返回 `ValidationError`。
fn voucher_expiry(
    revision: &entities::sales_order::SalesOrderRevision,
) -> Result<entities::common::time::Instant> {
    if revision.voucher_category_sku_id.is_none() || revision.voucher_expiry_at.is_none() {
        return Err(Error::ValidationError("非卡券销售单无法建立执行投影".to_string()));
    }
    Ok(revision.voucher_expiry_at.expect("卡券履约期限必填"))
}

/// 把销售单卡形态映射为投影卡形态（两枚举同构，投影白名单值对象）。
///
/// # 参数
/// * `form` - 销售单卡形态
///
/// # 返回
/// 返回投影卡形态。
fn to_projection_card_form(form: entities::sales_order::CardForm) -> entities::projection::CardForm {
    match form {
        entities::sales_order::CardForm::Electronic => entities::projection::CardForm::Electronic,
        entities::sales_order::CardForm::Physical => entities::projection::CardForm::Physical,
    }
}
