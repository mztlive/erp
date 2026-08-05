"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import { mockDelay } from "@/lib/mock-delay"
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
} from "@/features/access-audit/session"
import type {
  AccessChangeCommand,
  AccessChangeOutcome,
  AccessImpactPreview,
  AccessListQuery,
  AccessListView,
  AuditEventRow,
  EffectiveAccessView,
} from "@/features/access-audit/types"
import type { AccessEmptyReason } from "@/features/access-audit/types"

async function fetchAccessList(query: AccessListQuery): Promise<AccessListView> {
  await mockDelay(100)
  return buildW19ListView(query)
}

async function fetchEffectiveAccess(
  subjectType: "ROLE" | "USER",
  subjectId: string
): Promise<EffectiveAccessView | null> {
  await mockDelay(90)
  return getW19EffectiveAccess(subjectType, subjectId)
}

async function fetchAuditEvent(
  eventId: string
): Promise<AuditEventRow | null> {
  await mockDelay(70)
  return getW19AuditEvent(eventId)
}

async function previewAccessChange(
  command: AccessChangeCommand
): Promise<AccessImpactPreview> {
  await mockDelay(80)
  return previewW19AccessChange(command)
}

async function submitAccessChange(
  command: AccessChangeCommand
): Promise<AccessChangeOutcome> {
  await mockDelay(120)
  return submitW19AccessChange(command)
}

async function resolveAccessChangeUnknown(
  idempotencyKey: string
): Promise<AccessChangeOutcome | null> {
  await mockDelay(60)
  return queryW19Idempotency(idempotencyKey)
}

/** Demo：切换空态 / 策略配置（仅前端会话） */
async function setAccessDemoFlags(input: {
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

export const accessAuditKeys = {
  all: ["access-audit"] as const,
  list: (query: AccessListQuery) =>
    [...accessAuditKeys.all, "list", query] as const,
  effective: (subjectType: "ROLE" | "USER", subjectId: string) =>
    [...accessAuditKeys.all, "effective", subjectType, subjectId] as const,
  event: (eventId: string) =>
    [...accessAuditKeys.all, "event", eventId] as const,
}

export function useAccessListQuery(query: AccessListQuery) {
  return useQuery({
    queryKey: accessAuditKeys.list(query),
    queryFn: () => fetchAccessList(query),
  })
}

export function useEffectiveAccessQuery(
  subjectType: "ROLE" | "USER" | null,
  subjectId: string | null
) {
  return useQuery({
    queryKey: accessAuditKeys.effective(
      subjectType ?? "ROLE",
      subjectId ?? ""
    ),
    queryFn: () => fetchEffectiveAccess(subjectType!, subjectId!),
    enabled: Boolean(subjectType && subjectId),
  })
}

export function useAuditEventQuery(eventId: string | null) {
  return useQuery({
    queryKey: accessAuditKeys.event(eventId ?? ""),
    queryFn: () => fetchAuditEvent(eventId!),
    enabled: Boolean(eventId),
  })
}

export function usePreviewAccessChangeMutation() {
  return useMutation({
    mutationFn: (command: AccessChangeCommand) => previewAccessChange(command),
  })
}

export function useSubmitAccessChangeMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (command: AccessChangeCommand) => submitAccessChange(command),
    onSuccess: async (result) => {
      if (result.outcome === "CONFIRMED") {
        await queryClient.invalidateQueries({ queryKey: accessAuditKeys.all })
      }
    },
  })
}

export function useResolveAccessUnknownMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (idempotencyKey: string) =>
      resolveAccessChangeUnknown(idempotencyKey),
    onSuccess: async (result) => {
      if (result?.outcome === "CONFIRMED") {
        await queryClient.invalidateQueries({ queryKey: accessAuditKeys.all })
      }
    },
  })
}

export function useSetAccessDemoFlagsMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: setAccessDemoFlags,
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: accessAuditKeys.all })
    },
  })
}
