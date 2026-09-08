//! 导入和供应侧异常的共同只读字段；不装载加密地址、连接凭据和文件指纹。
use super::brief::{format_instant_datetime, push_section, ObjectBriefSource};
use super::{object_ids, ObjectKind, WorkbenchObjectFactMap, WorkbenchReadService};
use crate::errors::Result;
use erp_import::LegacyImportExt;
use erp_supply::entity::supplier_fulfillment::SupplierFulfillmentOrder;
use erp_supply::entity::supplier_offering::{SupplierOffering, SupplierOfferingAvailability};
use erp_supply::repository::{SupplierFulfillmentExt, SupplierOfferingExt};
use persistence_core::Executor;
use std::collections::{HashMap, HashSet};

impl<A: erp_workflow::WorkflowAuthorizationPort> WorkbenchReadService<A> {
    /// 对已授权装载的导入、供应商订单和供给异常补齐公共事实。
    ///
    /// # 参数
    /// `keys` 为本批对象键，`facts` 为已装载事实，`executor` 为查询执行器。
    /// # 返回
    /// 原地补充公共字段，不修改授权事实。
    /// # 错误
    /// 任一仓储查询失败时返回原错误。
    pub(super) async fn load_operational_briefs(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut WorkbenchObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.import_briefs(keys, facts, executor).await?;
        self.supplier_order_briefs(keys, facts, executor).await?;
        self.offering_briefs(keys, facts, executor).await
    }

    /// 导入确认的批次、基准日和成功失败数量对所有可读角色一致。
    async fn import_briefs(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut WorkbenchObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::LegacyImportBatch);
        if ids.is_empty() {
            return Ok(());
        }
        for batch in self
            .db
            .legacy_import_batches()
            .list_active_by_ids(&ids, executor)
            .await?
        {
            let Some(fact) = facts.get_mut(&(ObjectKind::LegacyImportBatch, batch.base.id.clone())) else {
                continue;
            };
            fact.display.brief_source = Some(sections([
                ("导入批次", Some(batch.batch_no)),
                ("批次状态", Some(batch.status.label().into())),
                ("业务基准日", Some(batch.baseline_date.to_string())),
                ("总行数", Some(batch.total_rows.to_string())),
                ("成功行数", Some(batch.success_rows.to_string())),
                ("失败行数", Some(batch.failed_rows.to_string())),
            ]));
        }
        Ok(())
    }

    /// 供应商履约异常保留供应商名称、订单号与三条独立状态及里程碑。
    async fn supplier_order_briefs(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut WorkbenchObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::SupplierFulfillmentOrder);
        if ids.is_empty() {
            return Ok(());
        }
        let rows = self
            .db
            .supplier_fulfillment_orders()
            .list_active_by_ids(&ids, executor)
            .await?;
        let names = self
            .supplier_display_names(
                &rows
                    .iter()
                    .map(|row| row.supplier_id.to_string())
                    .collect::<Vec<_>>(),
                executor,
            )
            .await?;
        for row in rows {
            let Some(fact) = facts.get_mut(&(ObjectKind::SupplierFulfillmentOrder, row.base.id.clone()))
            else {
                continue;
            };
            fact.display.counterparty_label = names.get(row.supplier_id.as_ref()).cloned();
            fact.display.brief_source = Some(supplier_order_source(
                row,
                fact.display.counterparty_label.clone(),
            ));
        }
        Ok(())
    }

    /// 停供异常保留业务订货编码、登记状态；不把内部 SKU 身份当作商品名称。
    async fn offering_briefs(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut WorkbenchObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::SupplierOffering);
        if ids.is_empty() {
            return Ok(());
        }
        let rows = self
            .db
            .supplier_offerings()
            .list_active_by_ids(&ids, executor)
            .await?;
        let names = self
            .supplier_display_names(
                &rows
                    .iter()
                    .map(|row| row.supplier_id.to_string())
                    .collect::<Vec<_>>(),
                executor,
            )
            .await?;
        let offering_ids = rows
            .iter()
            .map(|row| erp_core::ids::SupplierOfferingId::new(row.base.id.clone()))
            .collect::<Vec<_>>();
        let availability = self
            .db
            .supplier_offering_availabilities()
            .find_by_offering_ids(&offering_ids, executor)
            .await?
            .into_iter()
            .map(|row| (row.supplier_offering_id.to_string(), row))
            .collect::<HashMap<_, _>>();
        for row in rows {
            let Some(fact) = facts.get_mut(&(ObjectKind::SupplierOffering, row.base.id.clone())) else {
                continue;
            };
            fact.display.counterparty_label = names.get(row.supplier_id.as_ref()).cloned();
            fact.display.brief_source = Some(offering_source(
                &row,
                fact.display.counterparty_label.clone(),
                availability.get(&row.base.id),
            ));
        }
        Ok(())
    }
}

/// 只纳入有值的业务字段；空摘要与不存在的金额不补零。
fn sections<const N: usize>(fields: [(&str, Option<String>); N]) -> ObjectBriefSource {
    let mut source = ObjectBriefSource::default();
    for (label, value) in fields {
        push_section(&mut source.extra_sections, label, value.as_deref(), false);
    }
    source
}

/// 供应商订单事实独立于调查动作，状态轴不得互相替代。
fn supplier_order_source(row: SupplierFulfillmentOrder, supplier: Option<String>) -> ObjectBriefSource {
    sections([
        ("供应商", supplier),
        ("履约订单号", Some(row.fulfillment_order_no)),
        ("供应商订单号", row.external_order_no),
        ("履约状态", Some(row.fulfillment_status.label().into())),
        ("取消状态", Some(row.cancel_status.label().into())),
        ("退款状态", Some(row.refund_status.label().into())),
        ("提交时间", row.submitted_at.map(format_instant_datetime)),
        ("接单时间", row.accepted_at.map(format_instant_datetime)),
        ("完成时间", row.completed_at.map(format_instant_datetime)),
    ])
}

/// 停供可能来自登记状态或实时可供状态，两者都必须展示。
fn offering_source(
    row: &SupplierOffering,
    supplier: Option<String>,
    availability: Option<&SupplierOfferingAvailability>,
) -> ObjectBriefSource {
    let mut source = sections([
        ("供应商", supplier),
        ("供应商订货编码", Some(row.supplier_sku_code.clone())),
        ("供应商商品编码", row.supplier_product_code.clone()),
        ("供给登记状态", Some(row.stable.status.label().into())),
        ("登记来源", Some(row.source_type.label().into())),
    ]);
    if let Some(availability) = availability {
        push_section(
            &mut source.extra_sections,
            "当前可供状态",
            Some(availability.availability_status.label()),
            false,
        );
        let quantity = availability
            .available_quantity
            .as_ref()
            .map(|quantity| super::brief::format_quantity(quantity, None));
        push_section(
            &mut source.extra_sections,
            "当前可供数量",
            quantity.as_deref(),
            true,
        );
        push_section(
            &mut source.extra_sections,
            "供给更新时间",
            Some(&format_instant_datetime(availability.source_updated_at)),
            false,
        );
    }
    source
}
