use application_core::AuditActor;
use entity_core::BaseModel;
use erp_audit::AuditActorLogs;
use erp_core::common::time::Instant;
use erp_supply::api_order::{ApiOrderExecution, ApiOrderExt, ApiOrderSetupReceipt, ApiOrderStage};
use erp_supply::ports::connector::order::{CoordinateSystem, DeliveryMethod, GeocodedAddress, Recipient};
use erp_supply::service::supplier_fulfillment::FulfillmentOrderAccess;
use persistence_core::{NoTransaction, Transactional};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::SupplierApiOrderProcess;
use super::snapshot::ExecutionSnapshot;
use crate::{Error, Result};

/// 明确核验的百度坐标；不从自由文本或其它坐标系猜测。
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionGeocode {
    pub city_name: String,
    pub district: String,
    pub latitude: String,
    pub longitude: String,
    pub coordinate_system: String,
}

/// 配送补充命令，不允许修改冻结收件地址或商品。
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionSetupRequest {
    pub expected_version: u64,
    pub idempotency_key: String,
    pub geocode: Option<ExecutionGeocode>,
    pub choice_index: Option<usize>,
    pub recipient_phone: Option<String>,
}

/// 可供采购确认的配送选项；索引只在对应执行版本有效。
#[derive(Serialize)]
pub struct ExecutionChoiceView {
    pub index: usize,
    pub method: &'static str,
    pub date: Option<String>,
    pub slot: Option<String>,
    pub store_id: Option<String>,
    pub shipping_fee: String,
}

/// API 订单配送资料和真实后台阶段。
#[derive(Serialize)]
pub struct ExecutionSetupView {
    pub purchase_order_id: String,
    pub purchase_order_revision_id: String,
    pub recipient: Option<ExecutionRecipientView>,
    pub recipient_masked: bool,
    pub execution_version: u64,
    pub stage: ApiOrderStage,
    pub blocker: Option<String>,
    pub geocode: Option<ExecutionGeocode>,
    pub choices: Vec<ExecutionChoiceView>,
    pub selected_choice: Option<usize>,
    pub geocode_editable: bool,
    pub choice_editable: bool,
    pub recipient_phone_required: bool,
}

/// 授权采购核对坐标所需的最小冻结收件资料。
#[derive(Serialize)]
pub struct ExecutionRecipientView {
    pub name: String,
    pub phone: String,
    pub region: String,
    pub address: String,
}

impl SupplierApiOrderProcess {
    /// 读取当前执行资料，调用方须先执行供应商订单 detail 数据范围授权。
    /// # 参数
    /// `id` 为已授权的供应商订单身份。
    /// # 返回
    /// 真实阶段、阻断原因与可供确认选项。
    /// # 错误
    /// 非自动执行订单返回 NotFound，密文无效返回错误。
    pub async fn execution_setup(&self, id: &str) -> Result<ExecutionSetupView> {
        let execution = self.load(id).await?;
        let snapshot =
            if execution.request_encrypted.is_some() { Some(self.decode(&execution)?) } else { None };
        // 普通订单详情权限不授予完整地址或精确坐标揭示资格。
        let geocode = None;
        let choices = snapshot.as_ref().map(choice_views).unwrap_or_default();
        let waiting = execution.stage == ApiOrderStage::AwaitingSetup;
        let context_ready = snapshot.as_ref().is_some_and(|value| value.context.is_some());
        let recipient_phone_required =
            snapshot.as_ref().is_some_and(|value| value.recipient.phone.is_empty());
        let recipient = snapshot.as_ref().map(|snapshot| masked_recipient(&snapshot.recipient));
        Ok(ExecutionSetupView {
            purchase_order_id: execution.purchase_order_id,
            purchase_order_revision_id: execution.purchase_order_revision_id,
            recipient,
            recipient_masked: true,
            execution_version: execution.base.version,
            stage: execution.stage,
            blocker: execution.blocker,
            geocode,
            choices,
            selected_choice: snapshot.as_ref().and_then(|value| value.selected_choice),
            geocode_editable: waiting && !context_ready && snapshot.is_some(),
            choice_editable: waiting && context_ready,
            recipient_phone_required,
        })
    }

    /// 保存经采购确认的坐标或配送选择，后台继续执行，保存不代表下单成功。
    /// # 参数
    /// `id` 为已授权订单；`request` 携带预期执行版本及稳定操作号。
    /// # 返回
    /// 已保存资料及新执行阶段。
    /// # 错误
    /// 旧版本、非补充阶段、无效坐标、旧选项或异载荷重试时拒绝。
    pub async fn save_execution_setup(
        &self,
        id: &str,
        request: ExecutionSetupRequest,
        actor: &AuditActor,
        access: &FulfillmentOrderAccess,
    ) -> Result<ExecutionSetupView> {
        validate_request(&request)?;
        let hash = hex::encode(Sha256::digest(
            serde_json::to_vec(&(&request.geocode, request.choice_index, &request.recipient_phone))
                .map_err(|_| Error::ValidationError("配送补充资料无效".into()))?,
        ));
        let mut execution = self.load(id).await?;
        let receipt = ApiOrderSetupReceipt {
            base: BaseModel::new(format!(
                "setup-{}",
                hex::encode(Sha256::digest(format!("{id}\0{}", request.idempotency_key)))
            )),
            execution_id: id.into(),
            payload_hash: hash.clone(),
        };
        if let Some(previous) = self
            .db
            .supplier_api_order_setup_receipts()
            .find_by_id(&receipt.base.id, &mut NoTransaction)
            .await?
        {
            previous.ensure_replay(id, &hash)?;
            return self.execution_setup(id).await;
        }
        if execution.base.version != request.expected_version
            || execution.stage != ApiOrderStage::AwaitingSetup
        {
            return Err(Error::ConflictError("接口执行资料已变化，请刷新后核对".into()));
        }
        let mut snapshot = self.decode(&execution)?;
        apply_setup(&mut execution, &mut snapshot, &request)?;
        self.encode(&mut execution, &snapshot)?;
        execution.setup_idempotency_key = Some(request.idempotency_key);
        execution.setup_payload_hash = Some(hash);
        execution.blocker = None;
        execution.next_run_at = Instant::now();
        self.persist_setup(execution, receipt, actor, access).await?;
        self.execution_setup(id).await
    }

    async fn persist_setup(
        &self,
        mut execution: ApiOrderExecution,
        receipt: ApiOrderSetupReceipt,
        actor: &AuditActor,
        access: &FulfillmentOrderAccess,
    ) -> Result<()> {
        let db = self.db.clone();
        let client = db.client().clone();
        let actor = actor.clone();
        let access = access.clone();
        client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    access.require_order(&actor, "submit", &execution.base.id, None, executor).await?;
                    db.supplier_api_order_executions().update(&mut execution, executor).await?;
                    db.supplier_api_order_setup_receipts().create(&receipt, executor).await?;
                    let audit = actor.resource_log(
                        "supplier_fulfillment.execution_setup",
                        "supplier_fulfillment_order",
                        execution.base.id.clone(),
                    )?;
                    crate::audit::persist_log(&db, &audit, executor).await?;
                    Ok::<_, Error>(())
                })
            })
            .await
    }
}

impl TryFrom<ExecutionGeocode> for GeocodedAddress {
    type Error = Error;
    fn try_from(value: ExecutionGeocode) -> Result<Self> {
        let latitude: Decimal =
            value.latitude.parse().map_err(|_| Error::ValidationError("百度纬度格式无效".into()))?;
        let longitude: Decimal =
            value.longitude.parse().map_err(|_| Error::ValidationError("百度经度格式无效".into()))?;
        if value.coordinate_system != "BD09"
            || value.city_name.trim().is_empty()
            || value.district.trim().is_empty()
            || latitude < Decimal::from(-90)
            || latitude > Decimal::from(90)
            || longitude < Decimal::from(-180)
            || longitude > Decimal::from(180)
        {
            return Err(Error::ValidationError("请提供已核验的城市、区县与 BD-09 坐标".into()));
        }
        Ok(Self {
            city_name: value.city_name.trim().into(),
            district: value.district.trim().into(),
            latitude: value.latitude,
            longitude: value.longitude,
            coordinate_system: CoordinateSystem::Bd09,
        })
    }
}

fn validate_request(request: &ExecutionSetupRequest) -> Result<()> {
    if request.idempotency_key.trim().is_empty()
        || request.idempotency_key.len() > 128
        || request.expected_version == 0
        || request.geocode.is_none() == request.choice_index.is_none()
    {
        return Err(Error::ValidationError(
            "每次请选择补充坐标或确认配送方案，并提供有效版本与操作号".into(),
        ));
    }
    Ok(())
}

fn masked_recipient(recipient: &Recipient) -> ExecutionRecipientView {
    let name = recipient.name.chars().next().map(|first| format!("{first}**")).unwrap_or_else(|| "—".into());
    let suffix =
        recipient.phone.chars().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect::<String>();
    let region = erp_core::region::RegionCode::parse(&recipient.region)
        .map(|code| code.full_name())
        .unwrap_or_else(|_| "收件地区待核对".into());
    ExecutionRecipientView {
        name,
        phone: if suffix.is_empty() { String::new() } else { format!("***{suffix}") },
        region: recipient.region.clone(),
        address: format!("{region}（详细地址需授权查看）"),
    }
}

fn apply_setup(
    execution: &mut ApiOrderExecution,
    snapshot: &mut ExecutionSnapshot,
    request: &ExecutionSetupRequest,
) -> Result<()> {
    if let Some(phone) = request.recipient_phone.as_ref() {
        if !snapshot.recipient.phone.is_empty()
            || snapshot.context.is_some()
            || phone.trim().is_empty()
            || phone.len() > 64
        {
            return Err(Error::ValidationError("仅允许补齐来源收货仓缺失的有效联系电话".into()));
        }
        snapshot.recipient.phone = phone.trim().into();
    }
    if snapshot.recipient.phone.is_empty() {
        return Err(Error::ValidationError("请补齐来源收货仓联系电话".into()));
    }
    if let Some(geocode) = request.geocode.clone() {
        if snapshot.context.is_some() {
            return Err(Error::ConflictError("供应商地址已准备，不能直接更改坐标".into()));
        }
        snapshot.recipient.geocoded_address = Some(geocode.try_into()?);
        execution.stage = ApiOrderStage::PrepareAddress;
    }
    if let Some(index) = request.choice_index {
        let choice = snapshot
            .choices
            .get(index)
            .ok_or_else(|| Error::ValidationError("配送选项已失效，请刷新".into()))?;
        if snapshot.context.is_none() || choice.shipping_fee.to_decimal().is_sign_negative() {
            return Err(Error::ValidationError("配送上下文或费用无效".into()));
        }
        snapshot.selected_choice = Some(index);
        execution.stage = ApiOrderStage::PlanDelivery;
    }
    Ok(())
}

fn choice_views(snapshot: &ExecutionSnapshot) -> Vec<ExecutionChoiceView> {
    snapshot
        .choices
        .iter()
        .enumerate()
        .map(|(index, choice)| {
            let (method, date, slot, store_id) = match &choice.method {
                DeliveryMethod::Courier => ("COURIER", None, None, None),
                DeliveryMethod::LocalDelivery { date, slot } => {
                    ("LOCAL_DELIVERY", Some(date.to_string()), Some(slot.clone()), None)
                },
                DeliveryMethod::Pickup { store_id, date, slot } => {
                    ("PICKUP", Some(date.to_string()), Some(slot.clone()), Some(store_id.clone()))
                },
            };
            ExecutionChoiceView {
                index,
                method,
                date,
                slot,
                store_id,
                shipping_fee: choice.shipping_fee.to_string(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn geocode_rejects_unproven_coordinate_system_and_range() {
        let mut value = ExecutionGeocode {
            city_name: "北京市".into(),
            district: "朝阳区".into(),
            latitude: "39.9".into(),
            longitude: "116.4".into(),
            coordinate_system: "GCJ02".into(),
        };
        assert!(GeocodedAddress::try_from(value.clone()).is_err());
        value.coordinate_system = "BD09".into();
        assert!(GeocodedAddress::try_from(value.clone()).is_ok());
        value.latitude = "91".into();
        assert!(GeocodedAddress::try_from(value).is_err());
    }

    #[test]
    fn setup_default_view_never_reveals_full_recipient() {
        let value = Recipient {
            name: "张三".into(),
            phone: "13812345678".into(),
            region: "110105".into(),
            address: "北京市朝阳区某路123号".into(),
            geocoded_address: None,
        };
        let masked = masked_recipient(&value);
        assert_eq!(masked.name, "张**");
        assert_eq!(masked.phone, "***5678");
        assert!(!masked.address.contains("123号"));
        assert!(!masked.address.contains("某路"));
    }
}
