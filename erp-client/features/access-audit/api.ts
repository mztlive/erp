/**
 * W19 session-mock API：queryFn / mutationFn 纯函数。
 * 有效权限解释与影响数量一律来自服务端投影，禁止前端合并权限集合。
 */

import { mockDelay } from "@/features/workspace-kit/delay"
import type {
  AccessChangeCommand,
  AccessChangeOutcome,
  AccessImpactPreview,
  AccessListQuery,
  AccessListView,
  AuditEventRow,
  EffectiveAccessView,
} from "@/features/access-audit/types"
import {
  buildW19ListView,
  getW19AuditEvent,
  getW19EffectiveAccess,
  previewW19AccessChange,
  queryW19Idempotency,
  setW19AuditAccessPolicyConfigured,
  setW19DemoEmptyReason,
  setW19FieldGranularityConfigured,
  setW19UserRoleTimePolicyConfigured,
  submitW19AccessChange,
  getW19DemoEmptyReason,
  getW19GovernanceFlags,
} from "@/features/access-audit/session"
import type { AccessEmptyReason } from "@/features/access-audit/types"

export async function fetchAccessList(
  query: AccessListQuery
): Promise<AccessListView> {
  await mockDelay(100)
  return buildW19ListView(query)
}

export async function fetchEffectiveAccess(
  subjectType: "ROLE" | "USER",
  subjectId: string
): Promise<EffectiveAccessView | null> {
  await mockDelay(90)
  return getW19EffectiveAccess(subjectType, subjectId)
}

export async function fetchAuditEvent(
  eventId: string
): Promise<AuditEventRow | null> {
  await mockDelay(70)
  return getW19AuditEvent(eventId)
}

export async function previewAccessChange(
  command: AccessChangeCommand
): Promise<AccessImpactPreview> {
  await mockDelay(80)
  return previewW19AccessChange(command)
}

export async function submitAccessChange(
  command: AccessChangeCommand
): Promise<AccessChangeOutcome> {
  await mockDelay(120)
  return submitW19AccessChange(command)
}

export async function resolveAccessChangeUnknown(
  idempotencyKey: string
): Promise<AccessChangeOutcome | null> {
  await mockDelay(60)
  return queryW19Idempotency(idempotencyKey)
}

/** Demo：切换空态 / 策略配置（仅前端会话） */
export async function setAccessDemoFlags(input: {
  emptyReason?: AccessEmptyReason | null
  userRoleTimePolicyConfigured?: boolean
  fieldGranularityConfigured?: boolean
  auditAccessPolicyConfigured?: boolean
}): Promise<{ ok: true }> {
  await mockDelay(40)
  if ("emptyReason" in input) {
    setW19DemoEmptyReason(input.emptyReason ?? null)
  }
  if (typeof input.userRoleTimePolicyConfigured === "boolean") {
    setW19UserRoleTimePolicyConfigured(input.userRoleTimePolicyConfigured)
  }
  if (typeof input.fieldGranularityConfigured === "boolean") {
    setW19FieldGranularityConfigured(input.fieldGranularityConfigured)
  }
  if (typeof input.auditAccessPolicyConfigured === "boolean") {
    setW19AuditAccessPolicyConfigured(input.auditAccessPolicyConfigured)
  }
  return { ok: true }
}

export function readAccessDemoFlags() {
  return {
    emptyReason: getW19DemoEmptyReason(),
    ...getW19GovernanceFlags(),
  }
}
