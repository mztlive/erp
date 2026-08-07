"use client"

import {
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query"

import {
  createCustomer,
  fetchCustomerCenter,
  fetchCustomerDirectory,
  queryCustomerMutationByIdempotency,
  revealCustomerSensitiveField,
  saveCustomerDetails,
  saveCustomerRevision,
} from "@/features/customers/api"
import type {
  CreateCustomerInput,
  CustomerDirectoryQuery,
  SaveCustomerDetailsInput,
  SaveCustomerRevisionInput,
} from "@/features/customers/types"

export { revealCustomerSensitiveField }

export const customerKeys = {
  all: ["customers"] as const,
  directory: (query: CustomerDirectoryQuery) =>
    [...customerKeys.all, "directory", query] as const,
  detail: (customerId: string) =>
    [...customerKeys.all, "detail", customerId] as const,
}

export function useCustomerDirectoryQuery(
  query: CustomerDirectoryQuery,
  options?: { enabled?: boolean }
) {
  return useQuery({
    queryKey: customerKeys.directory(query),
    queryFn: () => fetchCustomerDirectory(query),
    // 切换筛选时保留上一批结果渲染，避免整卡闪烁（数据表骨架只出现于首载）。
    placeholderData: (previous) => previous,
    enabled: options?.enabled ?? true,
  })
}

export function useCustomerCenterQuery(customerId: string) {
  return useQuery({
    queryKey: customerKeys.detail(customerId),
    queryFn: () => fetchCustomerCenter(customerId),
    enabled: Boolean(customerId),
  })
}

export function useSaveCustomerRevisionMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (input: SaveCustomerRevisionInput) =>
      saveCustomerRevision(input),
    onSuccess: async (result) => {
      if (result.outcome === "succeeded") {
        await queryClient.invalidateQueries({ queryKey: customerKeys.all })
      }
    },
  })
}

export function useSaveCustomerDetailsMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (input: SaveCustomerDetailsInput) =>
      saveCustomerDetails(input),
    onSuccess: async (result) => {
      if (result.outcome === "succeeded") {
        await queryClient.invalidateQueries({ queryKey: customerKeys.all })
      }
    },
  })
}

export function useCreateCustomerMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (input: CreateCustomerInput) => createCustomer(input),
    onSuccess: async (result) => {
      if (result.outcome === "succeeded") {
        await queryClient.invalidateQueries({ queryKey: customerKeys.all })
      }
    },
  })
}

export function useQueryCustomerIdempotencyMutation() {
  return useMutation({
    mutationFn: (idempotencyKey: string) =>
      queryCustomerMutationByIdempotency(idempotencyKey),
  })
}

export function useRevealCustomerSensitiveMutation() {
  return useMutation({
    mutationFn: (revealToken: string) =>
      revealCustomerSensitiveField(revealToken),
  })
}
