/**
 * W09 队列筛选的 URL ↔ 值互转。
 * 只保留服务端真正参与过滤的维度，避免 URL 里出现界面改不动、查询也不认的隐形状态。
 */

import type { FulfillmentOperationType } from "@/features/fulfillment-operations/types"
import { SLUG_TO_TYPE, TYPE_SLUG } from "@/features/fulfillment-operations/types"

const ALL_TYPES: FulfillmentOperationType[] = [
  "RECEIPT",
  "WAREHOUSE_SHIP",
  "SUPPLIER_DIRECT",
  "ELECTRONIC",
  "SERVICE",
]

export type DueFilter = "today" | "overdue"
export type GateFilter = "blocked" | "satisfied"

export function parseTypeParam(
  raw: string | null
): FulfillmentOperationType[] | undefined {
  if (!raw || raw === "all") return undefined
  const parts = raw.split(",").map((p) => p.trim()).filter(Boolean)
  const types = parts
    .map(
      (p) =>
        SLUG_TO_TYPE[p] ??
        (ALL_TYPES.includes(p as FulfillmentOperationType)
          ? (p as FulfillmentOperationType)
          : null)
    )
    .filter((t): t is FulfillmentOperationType => t != null)
  return types.length > 0 ? types : undefined
}

export function typeParamValue(
  types: FulfillmentOperationType[] | undefined
): string {
  if (!types || types.length === 0) return "all"
  if (types.length === 1) return TYPE_SLUG[types[0]]
  return types.map((t) => TYPE_SLUG[t]).join(",")
}

export function parseDueParam(raw: string | null): DueFilter | undefined {
  return raw === "today" || raw === "overdue" ? raw : undefined
}

export function parseGateParam(raw: string | null): GateFilter | undefined {
  return raw === "blocked" || raw === "satisfied" ? raw : undefined
}

export const DUE_FILTER_OPTIONS = [
  { value: "today", label: "今日到期" },
  { value: "overdue", label: "已超期" },
]

export const GATE_FILTER_OPTIONS = [
  { value: "satisfied", label: "货款已到，可以收货" },
  { value: "blocked", label: "先款未到，暂时不能收货" },
]
