//! 域 D16 `fulfillment`：purchase_receipt(+_line)、delivery(+_line)、electronic_delivery、service_fulfillment、customer_acceptance(+_line)、acceptance_fulfillment_allocation（页面：W06、W09）。P0 骨架占位；P3 填充服务与事务（Service 不跨域依赖，跨域只调对方 Repository）。

pub mod dto;
