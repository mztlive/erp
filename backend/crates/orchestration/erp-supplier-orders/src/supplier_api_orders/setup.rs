use entity_core::BaseModel;
use erp_read_models::supplier_center::execution_progress::{ExecutionOrderTreeView, ExecutionShipmentView};
use erp_supply::api_order::{ApiOrderExecution, ApiOrderSetupReceipt, ApiOrderStage};
use erp_supply::ports::connector::address::{
    AddressRequirement, ConfirmedSupplierAddress, SupplierAddressLevel,
};
use erp_supply::ports::connector::order::{CoordinateSystem, DeliveryMethod, GeocodedAddress, Recipient};
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
    pub supplier_address_ticket: Option<String>,
}

/// 可供采购确认的配送选项；索引只在对应执行版本有效。
#[derive(Serialize)]
pub struct ExecutionChoiceView {
    pub index: usize,
    pub method: &'static str,
    pub date: Option<String>,
    pub slot: Option<String>,
    pub store_id: Option<String>,
    pub shipping_fee: Option<String>,
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
    pub address_requirement: &'static str,
    pub supplier_address: Option<ExecutionSupplierAddressView>,
    pub supplier_address_editable: bool,
    pub order_tree: Option<ExecutionOrderTreeView>,
    pub shipments: Vec<ExecutionShipmentView>,
    pub result_query_available: bool,
}

/// 只返回人工已确认的四级名称及编码，不返回账号、地址指纹或原执行身份。
#[derive(Serialize)]
pub struct ExecutionSupplierAddressView {
    pub version: u64,
    pub province: SupplierAddressLevel,
    pub city: SupplierAddressLevel,
    pub county: SupplierAddressLevel,
    pub town: SupplierAddressLevel,
}

impl From<&ConfirmedSupplierAddress> for ExecutionSupplierAddressView {
    fn from(value: &ConfirmedSupplierAddress) -> Self {
        Self {
            version: value.version,
            province: value.province.clone(),
            city: value.city.clone(),
            county: value.county.clone(),
            town: value.town.clone(),
        }
    }
}

impl ExecutionSetupRequest {
    fn receipt(&self, id: &str) -> Result<(String, ApiOrderSetupReceipt)> {
        let hash = hex::encode(Sha256::digest(
            serde_json::to_vec(&(
                &self.geocode,
                self.choice_index,
                &self.recipient_phone,
                &self.supplier_address_ticket,
            ))
            .map_err(|_| Error::ValidationError("配送补充资料无效".into()))?,
        ));

        let receipt = ApiOrderSetupReceipt {
            base: BaseModel::new(format!(
                "setup-{}",
                hex::encode(Sha256::digest(format!("{id}\0{}", self.idempotency_key)))
            )),
            execution_id: id.into(),
            payload_hash: hash.clone(),
        };
        Ok((hash, receipt))
    }
}

/// 授权采购核对坐标所需的最小冻结收件资料。
#[derive(Serialize)]
pub struct ExecutionRecipientView {
    pub name: String,
    pub phone: String,
    pub region: String,
    pub address: String,
}

impl ExecutionSetupView {
    /// 按既有成本读取资格移除费用及可能含成本的来源错误正文。
    /// # 参数
    /// 无；Handler 仅在缺少 supplier_offering_cost:detail 时调用。
    /// # 返回
    /// 保留可操作业务原因、阶段、数量和物流，费用保持缺失而非伪造零。
    /// # 错误
    /// 不返回错误。
    pub fn without_costs(&mut self) {
        for choice in &mut self.choices {
            choice.shipping_fee = None;
        }
        self.blocker =
            self.blocker.as_deref().map(|reason| public_execution_blocker(reason, self.stage).to_string());
    }

    fn from_execution(
        execution: &ApiOrderExecution,
        snapshot: Option<&ExecutionSnapshot>,
        requirement: Option<AddressRequirement>,
        order_tree: Option<ExecutionOrderTreeView>,
        shipments: Vec<ExecutionShipmentView>,
    ) -> Self {
        let waiting = execution.stage == ApiOrderStage::AwaitingSetup;
        let context_ready = snapshot.is_some_and(|value| value.context.is_some());
        let address_editable = waiting && !context_ready && snapshot.is_some();
        Self {
            purchase_order_id: execution.purchase_order_id.clone(),
            purchase_order_revision_id: execution.purchase_order_revision_id.clone(),
            recipient: snapshot.map(|snapshot| masked_recipient(&snapshot.recipient)),
            recipient_masked: true,
            execution_version: execution.base.version,
            stage: execution.stage,
            blocker: execution.blocker.as_deref().or(execution.receipt_blocker.as_deref()).map(|reason| {
                if execution_outcome_unknown(execution.stage) {
                    public_execution_blocker(reason, execution.stage).into()
                } else {
                    reason.into()
                }
            }),
            geocode: None,
            choices: snapshot.map(choice_views).unwrap_or_default(),
            selected_choice: snapshot.and_then(|value| value.selected_choice),
            geocode_editable: address_editable
                && requirement == Some(AddressRequirement::Geocoded(CoordinateSystem::Bd09)),
            choice_editable: waiting && context_ready,
            recipient_phone_required: snapshot.is_some_and(|value| value.recipient.phone.is_empty()),
            address_requirement: match requirement {
                Some(AddressRequirement::Text) => "TEXT",
                Some(AddressRequirement::FourLevel) => "FOUR_LEVEL",
                Some(AddressRequirement::Geocoded(_)) => "BD09",
                None => "UNAVAILABLE",
            },
            supplier_address: snapshot
                .and_then(|value| value.recipient.supplier_address.as_ref())
                .map(ExecutionSupplierAddressView::from),
            supplier_address_editable: address_editable && requirement == Some(AddressRequirement::FourLevel),
            order_tree,
            shipments,
            result_query_available: execution.original_result_query_allowed(),
        }
    }
}

mod read;
mod write;

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
    let actions = usize::from(request.geocode.is_some())
        + usize::from(request.choice_index.is_some())
        + usize::from(request.supplier_address_ticket.is_some());
    if request.idempotency_key.trim().is_empty()
        || request.idempotency_key.len() > 128
        || request.expected_version == 0
        || actions > 1
        || (actions == 0 && request.recipient_phone.is_none())
    {
        return Err(Error::ValidationError(
            "每次请选择补充收货地址、坐标或确认配送方案，并提供有效版本与操作号".into(),
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

    if request.geocode.is_none()
        && request.choice_index.is_none()
        && request.supplier_address_ticket.is_none()
    {
        execution.stage = ApiOrderStage::PrepareAddress;
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
                shipping_fee: Some(choice.shipping_fee.to_string()),
            }
        })
        .collect()
}

fn execution_outcome_unknown(stage: ApiOrderStage) -> bool {
    matches!(
        stage,
        ApiOrderStage::AddressUnknown
            | ApiOrderStage::CreationUnknown
            | ApiOrderStage::ConfirmationUnknown
            | ApiOrderStage::PaymentUnknown
            | ApiOrderStage::CancellationUnknown
    )
}

fn public_execution_blocker(reason: &str, stage: ApiOrderStage) -> &'static str {
    if execution_outcome_unknown(stage) {
        return "原执行结果待核实，请查询原结果；系统不会重复发送原请求";
    }
    if reason.contains("运费") {
        return "供应商运费需要核对，请由有权限的采购经办人处理";
    }
    if reason.contains("余额") {
        return "供应商账户余额需要核对，请联系财务处理";
    }
    if reason.contains("授权") || reason.contains("付款") {
        return "付款批准或执行授权需要核对，请联系财务处理";
    }
    if reason.contains("金额") || reason.contains("价格") || reason.contains("报价") {
        return "采购金额或报价需要核对，请由有权限的采购经办人处理";
    }
    if reason.contains("地址") || reason.contains("坐标") {
        return "请按供应商要求补齐并核对收货资料";
    }
    if reason.contains("配置") || reason.contains("接口类型") {
        return "供应商接口配置尚未就绪，请联系管理员处理";
    }
    "当前执行需要经办人核对，请联系采购负责人处理"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_order_uses_recovery_guidance_even_when_supplier_suggests_retry() {
        let reason = public_execution_blocker("京东当前查询较多，请稍后再试", ApiOrderStage::CreationUnknown);
        assert!(reason.contains("请查询原结果"));
        assert!(reason.contains("不会重复发送"));
    }
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
            supplier_address: None,
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

    fn address_request() -> ExecutionSetupRequest {
        ExecutionSetupRequest {
            expected_version: 1,
            idempotency_key: "operation".into(),
            geocode: None,
            choice_index: None,
            recipient_phone: None,
            supplier_address_ticket: Some("issued-town-ticket".into()),
        }
    }

    #[test]
    fn address_setup_accepts_one_action_and_receipt_rejects_changed_payload() {
        let original = address_request();
        validate_request(&original).unwrap();
        let (hash, receipt) = original.receipt("execution").unwrap();
        receipt.ensure_replay("execution", &hash).unwrap();

        let mut changed = original.clone();
        changed.supplier_address_ticket = Some("other-town-ticket".into());
        let (other_hash, other_receipt) = changed.receipt("execution").unwrap();
        assert_eq!(receipt.base.id, other_receipt.base.id);
        assert!(receipt.ensure_replay("execution", &other_hash).is_err());

        changed.choice_index = Some(0);
        assert!(validate_request(&changed).is_err());
        let mut empty = original;
        empty.supplier_address_ticket = None;
        assert!(validate_request(&empty).is_err());
    }

    #[test]
    fn setup_rejects_client_invented_four_level_codes() {
        let request = serde_json::from_str::<ExecutionSetupRequest>(
            r#"{
            "expected_version":1,"idempotency_key":"operation","supplier_address":{"province":"1"}
        }"#,
        );
        assert!(request.is_err());
    }

    #[test]
    fn masked_execution_reason_and_choices_do_not_expose_costs() {
        let mut execution =
            ApiOrderExecution::queued("e".into(), "p".into(), "r".into(), "c".into(), "s".into(), vec![]);
        execution.stage = ApiOrderStage::AwaitingSetup;
        execution.blocker = Some("运费为19.98，超过批准范围".into());
        let mut view = ExecutionSetupView::from_execution(
            &execution,
            None,
            Some(AddressRequirement::FourLevel),
            None,
            vec![],
        );
        view.choices.push(ExecutionChoiceView {
            index: 0,
            method: "COURIER",
            date: None,
            slot: None,
            store_id: None,
            shipping_fee: Some("19.98".into()),
        });
        view.without_costs();
        assert_eq!(view.choices[0].shipping_fee, None);
        assert_eq!(view.blocker.as_deref(), Some("供应商运费需要核对，请由有权限的采购经办人处理"));

        execution.stage = ApiOrderStage::ConfirmationUnknown;
        let mut unknown = ExecutionSetupView::from_execution(&execution, None, None, None, vec![]);
        unknown.without_costs();
        assert_eq!(unknown.address_requirement, "UNAVAILABLE");
        assert!(!unknown.geocode_editable);
        assert!(!unknown.supplier_address_editable);
        assert_eq!(
            unknown.blocker.as_deref(),
            Some("原执行结果待核实，请查询原结果；系统不会重复发送原请求")
        );
    }
}
