//! 已核验的订单、预检及物流 DTO；不同方法封装由传输解码层分别指定。
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::wire::{WireId, WireText};

#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct JdOrder {
    pub jd_order_id: WireId,
    pub third_order_id: String,
    pub order_type: Option<WireText>,
    pub parent_type: WireText,
    pub parent_jd_order_id: Option<WireId>,
    #[serde(default)]
    pub child_jd_order_id_list: Option<Vec<WireId>>,
    #[serde(default)]
    pub sku_info_list: Vec<JdOrderLine>,
    pub order_price: Option<JdOrderPrice>,
    pub order_state: Option<JdOrderState>,
    pub qcjbylx: Option<WireText>,
    pub qyfwsp: Option<WireText>,
    #[serde(default)]
    pub order_ext_info: Option<BTreeMap<String, Option<WireText>>>,
}

impl JdOrder {
    // 确认预占或核对当前操作目标时，仅接受本次查询明确返回的根单：parentType=1、确认状态为 0 且未报告非空子单。
    // 确认状态缺失时不能推断为预占根单。
    pub(super) fn is_reservation_root(&self) -> bool {
        self.parent_type.0 == "1"
            && self.parent_jd_order_id.as_ref().is_none_or(|id| id.0 == "0")
            // 官方5824子单列表为可选；5856明确预占确认且支付后才拆单（预存款）。
            && self.child_order_ids().is_empty()
            && self
                .order_state
                .as_ref()
                .and_then(|state| state.confirm_state.as_ref())
                .is_some_and(|value| value.0 == "0")
    }

    // 补查缺失节点和验证叶子单时读取来源列出的子单号。
    // 空列表不证明拆单完成或具备操作资格；预占根单还须核对根身份及明确的未确认状态。
    pub(super) fn child_order_ids(&self) -> &[WireId] {
        self.child_jd_order_id_list.as_deref().unwrap_or_default()
    }

    // 核对订单能否走普通履约时，检查查询扩展中的 qcjbylx、qyfwsp、kmjhyy 和 medicine。
    // 顶层专门流程字段存在，或扩展标记非零（包括未知值）时，均不按普通实物处理。
    pub(super) fn requires_special_fulfillment(&self) -> bool {
        self.qcjbylx.is_some()
            || self.qyfwsp.is_some()
            || ["qcjbylx", "qyfwsp", "kmjhyy", "medicine"].iter().any(|key| {
                self.order_ext_info
                    .as_ref()
                    .and_then(|values| values.get(*key))
                    .and_then(Option::as_ref)
                    .is_some_and(|value| value.0 != "0")
            })
    }
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CancellationEligibility {
    pub jd_order_id: WireId,
    pub cancel_flag: WireText,
    pub reason_code: Option<String>,
}

#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct JdOrderLine {
    pub sku_id: WireId,
    pub sku_num: WireText,
    pub sku_price: Option<WireText>,
    #[serde(rename = "type")]
    pub line_type: Option<WireText>,
}

#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct JdOrderPrice {
    pub order_total_price: WireText,
    pub order_total_freight: Option<WireText>,
}

#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct JdOrderState {
    pub confirm_state: Option<WireText>,
    pub cancel_order_state: Option<WireText>,
    pub delivery_state: Option<WireText>,
    pub jd_order_state: Option<WireText>,
}

#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct SaleEligibility {
    pub sku_id: WireId,
    pub can_purchase: WireText,
}

#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct Stock {
    pub sku_id: WireId,
    pub stock_state_type: WireText,
    pub remain_num: WireText,
}

#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct Freight {
    pub total_freight: WireText,
}

#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct Balance {
    pub balance_info: Option<BalanceInfo>,
}

#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct BalanceInfo {
    pub remain_limit: WireText,
}

#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct Logistics {
    #[serde(default)]
    pub ziying_show_result: Vec<TrackEvent>,
    #[serde(default)]
    pub third_ps_show_result: Vec<ThirdLogistics>,
}

#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct ThirdLogistics {
    pub third_id: String,
    pub third_name: String,
    pub third_ps_show_info_list: Vec<TrackEvent>,
}

#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct TrackEvent {
    pub order_id: WireId,
    pub msg_time: String,
    pub content: String,
}

#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct CancelProgress {
    pub cancel_process_code: WireText,
    pub jd_order_id: WireId,
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, from_value, json};

    use super::*;

    fn reservation() -> Value {
        json!({"jdOrderId":"100","thirdOrderId":"original","parentType":"1",
            "childJdOrderIdList":[],"orderState":{"confirmState":0,"cancelOrderState":0}})
    }

    #[test]
    fn pending_parent_allows_optional_children_but_never_unknown_confirmation_or_explicit_split() {
        let source = reservation();
        assert!(from_value::<JdOrder>(source.clone()).unwrap().is_reservation_root());
        let mut omitted_children = source.clone();
        omitted_children.as_object_mut().unwrap().remove("childJdOrderIdList");
        assert!(from_value::<JdOrder>(omitted_children).unwrap().is_reservation_root());

        let mut uncertain = source.clone();
        uncertain.as_object_mut().unwrap().remove("orderState");
        assert!(!from_value::<JdOrder>(uncertain).unwrap().is_reservation_root());

        let mut split = source.clone();
        split["childJdOrderIdList"] = json!([101]);
        assert!(!from_value::<JdOrder>(split).unwrap().is_reservation_root());

        let mut confirmed = source;
        confirmed["orderState"]["confirmState"] = json!(1);
        assert!(!from_value::<JdOrder>(confirmed).unwrap().is_reservation_root());
    }

    #[test]
    fn requested_special_order_extensions_block_ordinary_order_handling() {
        for marker in ["qcjbylx", "qyfwsp", "kmjhyy", "medicine"] {
            let mut source = reservation();
            source["orderExtInfo"] = json!({marker:1});
            assert!(from_value::<JdOrder>(source).unwrap().requires_special_fulfillment());
        }

        let mut ordinary = reservation();
        ordinary["orderExtInfo"] = json!({"kmjhyy":0});
        assert!(!from_value::<JdOrder>(ordinary).unwrap().requires_special_fulfillment());
    }
}
