//! 配送信息查询的运单元数据；不包含可推定的商品或实发数量。

use serde::{Deserialize, Serialize};

use super::wire::WireId;

#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct DeliveryInfo {
    pub third_order_id: String,
    pub jd_order_id: WireId,
    #[serde(default)]
    pub logistic_info_list: Vec<DeliveryWaybill>,
    #[serde(default)]
    pub track_info_list: Vec<DeliveryTrace>,
}

#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct DeliveryWaybill {
    pub delivery_carrier: Option<String>,
    pub jd_order_id: WireId,
    pub delivery_order_id: String,
    pub parent_jd_order_id: Option<WireId>,
}

#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct DeliveryTrace {
    pub track_msg_time: String,
    pub track_content: String,
    pub track_operator: Option<String>,
}
