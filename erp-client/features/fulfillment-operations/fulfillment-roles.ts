/**
 * W09 岗位角色：仓储经办 / 采购经办。
 *
 * 角色取值来自岗位通道（lane），不做 URL 角色切换；可见作业类型由角色在
 * `api.ts` 收敛（见工作面文档 §2.2），队列视图的 roleLabel / viewerLabel /
 * canExecute 以接口返回为准。
 */

import type { FulfillmentOperationType } from "@/features/fulfillment-operations/types"

export type FulfillmentRole = "warehouse" | "procurement"

export type FulfillmentRoleDef = {
  value: FulfillmentRole
  /** 角色名 */
  label: string
  /**
   * 当前登录人的显示名，用于「仅我的」匹配任务的 responsibleLabel。
   * 只读角色不负责具体任务，故为 undefined。
   */
  userLabel?: string
  /** 该角色在 W09 能看到的作业类型 */
  types: readonly FulfillmentOperationType[]
  /** 能否执行（确认/保存/跳过）。false = 只读 */
  canExecute: boolean
}

export const FULFILLMENT_ROLES: Record<FulfillmentRole, FulfillmentRoleDef> = {
  warehouse: {
    value: "warehouse",
    label: "仓储经办",
    types: ["RECEIPT", "WAREHOUSE_SHIP"],
    canExecute: true,
  },
  procurement: {
    value: "procurement",
    label: "采购经办",
    types: ["SUPPLIER_DIRECT", "ELECTRONIC", "SERVICE"],
    canExecute: true,
  },
}

export const DEFAULT_FULFILLMENT_ROLE: FulfillmentRole = "warehouse"

export function resolveRole(raw: FulfillmentRole): FulfillmentRoleDef {
  return FULFILLMENT_ROLES[raw]
}
