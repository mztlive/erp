"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import {
  fetchAccessList,
  fetchAuditEvent,
  fetchEffectiveAccess,
  previewAccessChange,
  submitAccessChange,
} from "@/features/access-audit/api"
import type {
  AccessChangeCommand,
  AccessListQuery,
} from "@/features/access-audit/types"

const accessAuditKeys = {
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
