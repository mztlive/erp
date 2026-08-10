"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import {
  createAllocationSession,
  fetchAllocationSession,
  fetchCustomerAccountsDetail,
  fetchCustomerAccountsList,
  postAllocation,
  resolvePostUnknown,
  reverseFact,
  saveAllocationDraft,
} from "@/features/customer-receivables/api"
import type { CustomerAccountsQuery } from "@/features/customer-receivables/types"

const customerReceivableKeys = {
  all: ["customer-receivables"] as const,
  list: (query: CustomerAccountsQuery) =>
    [...customerReceivableKeys.all, "list", query] as const,
  detail: (kind: string, id: string) =>
    [...customerReceivableKeys.all, "detail", kind, id] as const,
  session: (draftSessionId: string) =>
    [...customerReceivableKeys.all, "session", draftSessionId] as const,
}

export function useCustomerAccountsListQuery(query: CustomerAccountsQuery) {
  return useQuery({
    queryKey: customerReceivableKeys.list(query),
    queryFn: () => fetchCustomerAccountsList(query),
  })
}

export function useCustomerAccountsDetailQuery(
  kind: "receivable" | "receipt" | "invoice" | null,
  id: string | null
) {
  return useQuery({
    queryKey: customerReceivableKeys.detail(kind ?? "", id ?? ""),
    queryFn: () => fetchCustomerAccountsDetail(kind!, id!),
    enabled: Boolean(kind && id),
  })
}

export function useAllocationSessionQuery(draftSessionId: string | null) {
  return useQuery({
    queryKey: customerReceivableKeys.session(draftSessionId ?? ""),
    queryFn: () => fetchAllocationSession(draftSessionId!),
    enabled: Boolean(draftSessionId),
  })
}

export function useCreateAllocationSessionMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: createAllocationSession,
    onSuccess: async (session) => {
      await queryClient.invalidateQueries({
        queryKey: customerReceivableKeys.session(session.draftSessionId),
      })
    },
  })
}

export function useSaveAllocationDraftMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: saveAllocationDraft,
    onSuccess: async (session) => {
      await queryClient.invalidateQueries({
        queryKey: customerReceivableKeys.session(session.draftSessionId),
      })
    },
  })
}

export function usePostAllocationMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: postAllocation,
    onSuccess: async (result) => {
      if (result.status === "succeeded") {
        await queryClient.invalidateQueries({
          queryKey: customerReceivableKeys.all,
        })
      }
    },
  })
}

export function useResolvePostUnknownMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: resolvePostUnknown,
    onSuccess: async (result) => {
      if (result?.status === "succeeded") {
        await queryClient.invalidateQueries({
          queryKey: customerReceivableKeys.all,
        })
      }
    },
  })
}

export function useReverseFactMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: reverseFact,
    onSuccess: async (result) => {
      if (result.status === "succeeded") {
        await queryClient.invalidateQueries({
          queryKey: customerReceivableKeys.all,
        })
      }
    },
  })
}
