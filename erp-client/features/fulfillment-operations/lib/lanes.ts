/**
 * W09 岗位通道（lane）：侧栏两个业务入口，底层同一作业引擎。
 *
 * - warehouse → 收货与发货（入库 + 公司仓发）
 * - procurement → 交付与代发（直发 + 电子 + 服务）
 *
 * lane 只决定**标题和说明**。可见作业类型仍由角色在服务端收敛
 * （见 `fulfillment-roles.ts` 与工作面文档 §2.2）—— 这里不再放第二份类型清单，
 * 否则前端会多出一套可能与服务端对不上的可见性口径。
 */

export type FulfillmentLane = "warehouse" | "procurement"
