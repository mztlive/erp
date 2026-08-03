/**
 * W09 演示角色。
 *
 * 沿用本仓库既有的 `demoRole` URL 参数模式（见 supplier-catalog / supplier-settlements）：
 * 接真实登录后只需把 `resolveRole` 的取值来源从 URL 换成会话身份，下游不用动。
 *
 * 类型可见性由 mock api 按角色过滤（不是前端拿到全量再隐藏），对齐工作面文档 §2.2。
 */

import type { FulfillmentOperationType } from "@/features/fulfillment-operations/types"

export type FulfillmentRole =
  | "warehouse"
  | "procurement"
  | "sales"
  | "finance"

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
    userLabel: "仓储 · 周航",
    types: ["RECEIPT", "WAREHOUSE_SHIP"],
    canExecute: true,
  },
  procurement: {
    value: "procurement",
    label: "采购经办",
    userLabel: "采购 · 李采",
    types: ["SUPPLIER_DIRECT", "ELECTRONIC", "SERVICE"],
    canExecute: true,
  },
  sales: {
    value: "sales",
    label: "销售（只读）",
    types: ["RECEIPT", "WAREHOUSE_SHIP", "SUPPLIER_DIRECT", "ELECTRONIC", "SERVICE"],
    canExecute: false,
  },
  finance: {
    value: "finance",
    label: "财务（只读）",
    types: ["RECEIPT", "WAREHOUSE_SHIP", "SUPPLIER_DIRECT", "ELECTRONIC", "SERVICE"],
    canExecute: false,
  },
}

export const DEFAULT_FULFILLMENT_ROLE: FulfillmentRole = "warehouse"

export function resolveRole(raw: string | null): FulfillmentRoleDef {
  if (raw && raw in FULFILLMENT_ROLES) {
    return FULFILLMENT_ROLES[raw as FulfillmentRole]
  }
  return FULFILLMENT_ROLES[DEFAULT_FULFILLMENT_ROLE]
}

export const ROLE_OPTIONS = (
  Object.keys(FULFILLMENT_ROLES) as FulfillmentRole[]
).map((value) => ({ value, label: FULFILLMENT_ROLES[value].label }))

/** 角色能不能看这个类型 */
export function roleCanSee(
  role: FulfillmentRoleDef,
  type: FulfillmentOperationType
): boolean {
  return role.types.includes(type)
}
