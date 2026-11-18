use erp_core::common::time::Instant;

use super::OrderTracking;

impl OrderTracking {
    /// 判定消息接收之后是否已经取得新观察并完成原业务应用。
    /// 观察身份已应用、无未决错误且查询时间不早于接收时返回true。
    pub fn applied_since(&self, received_at: Instant) -> bool {
        self.latest_observation_id.is_some()
            && self.latest_observation_id == self.applied_observation_id
            && self.last_verified_at.is_some_and(|at| at >= received_at)
            && self.last_error.is_none()
    }
}

#[cfg(test)]
mod tests {
    use entity_core::BaseModel;

    use super::*;
    use crate::order_tracking::TrackingState;

    #[test]
    fn message_completion_requires_new_applied_observation_and_no_error() {
        let mut tracking = OrderTracking {
            base: BaseModel::fake(),
            connection_id: "connection".into(),
            purchase_order_id: "purchase".into(),
            source_lines: vec![],
            state: TrackingState::Polling,
            poll_after: Instant::from_unix_secs(0),
            lease_id: None,
            lease_until: None,
            latest_observation_id: Some("observation".into()),
            applied_observation_id: None,
            last_error: None,
            last_verified_at: Some(Instant::from_unix_secs(20)),
            applied_shipments: vec![],
        };
        assert!(!tracking.applied_since(Instant::from_unix_secs(10)));
        tracking.applied_observation_id = tracking.latest_observation_id.clone();
        assert!(tracking.applied_since(Instant::from_unix_secs(10)));
        assert!(!tracking.applied_since(Instant::from_unix_secs(21)));
        tracking.last_error = Some("pending".into());
        assert!(!tracking.applied_since(Instant::from_unix_secs(10)));
    }
}
