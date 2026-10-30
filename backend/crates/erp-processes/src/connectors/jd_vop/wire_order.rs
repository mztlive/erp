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
    /// 核对来源明确报告的未确认、未拆分原预占父单。
    /// # 参数
    /// 无；只使用本次原订单查询事实。
    /// # 返回
    /// 原根、parentType=1、确认状态0且未报告非空子单时返回真。
    /// # 错误
    /// 不返回错误；缺少确认状态不能推定为预占根单。
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

    /// 读取来源已经提供的子单身份，不单独以缺失列表判定操作资格。
    /// # 参数
    /// 无；预占根资格还须由 is_reservation_root 核对原根及明确未确认状态。
    /// # 返回
    /// 用于查询缺失节点及叶子验证的子单切片。
    /// # 错误
    /// 不返回错误，空切片本身不证明拆单完成。
    pub(super) fn child_order_ids(&self) -> &[WireId] {
        self.child_jd_order_id_list.as_deref().unwrap_or_default()
    }

    /// 判断已请求扩展标记中是否包含首期未支持的专门履约流程。
    /// # 参数
    /// 无；请求层须显式查询qcjbylx、qyfwsp、kmjhyy和medicine。
    /// # 返回
    /// 任何非零特殊标记返回真，不静默降级为普通实物。
    /// # 错误
    /// 不返回错误，未知非空标记同样按不支持处理。
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
