"use client"

import {
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query"

import { mockDelay } from "@/lib/mock-delay"
import { filterCustomerDirectory } from "@/features/customers/filter-customers"
import {
  createW03Customer,
  getW03DetailBaseline,
  getW03NoCustomerScope,
  getW03SensitiveReveal,
  listW03DirectoryBaseline,
  queryW03Idempotency,
  saveW03CustomerDetails,
  saveW03CustomerRevision,
} from "@/features/customers/session"
import type {
  CreateCustomerInput,
  CustomerCenterView,
  CustomerDirectoryQuery,
  CustomerDirectoryResult,
  CustomerMutationResult,
  SaveCustomerDetailsInput,
  SaveCustomerRevisionInput,
} from "@/features/customers/types"

async function fetchCustomerDirectory(
  query: CustomerDirectoryQuery
): Promise<CustomerDirectoryResult> {
  await mockDelay(90)

  if (getW03NoCustomerScope()) {
    return {
      hasCustomerScope: false,
      items: [],
      totalInScope: 0,
      queriedAt: new Date().toISOString(),
    }
  }

  const all = listW03DirectoryBaseline()
  const inScope = all.filter((item) => item.scopeTags.includes(query.scope))
  const items = filterCustomerDirectory(all, query)

  return {
    hasCustomerScope: true,
    items,
    totalInScope: inScope.length,
    queriedAt: new Date().toISOString(),
  }
}

async function fetchCustomerCenter(
  customerId: string
): Promise<CustomerCenterView | null> {
  await mockDelay(100)
  return getW03DetailBaseline(customerId)
}

async function saveCustomerRevision(
  input: SaveCustomerRevisionInput
): Promise<CustomerMutationResult> {
  await mockDelay(120)
  return saveW03CustomerRevision(input)
}

async function saveCustomerDetails(
  input: SaveCustomerDetailsInput
): Promise<CustomerMutationResult> {
  await mockDelay(140)
  return saveW03CustomerDetails(input)
}

async function createCustomer(
  input: CreateCustomerInput
): Promise<CustomerMutationResult> {
  await mockDelay(140)
  return createW03Customer(input)
}

async function queryCustomerMutationByIdempotency(
  idempotencyKey: string
): Promise<CustomerMutationResult | null> {
  await mockDelay(60)
  return queryW03Idempotency(idempotencyKey)
}

/** Short-lived reveal; plaintext never appears in directory/detail payloads. */
export async function revealCustomerSensitiveField(
  revealToken: string
): Promise<string> {
  await mockDelay(80)
  const value = getW03SensitiveReveal(revealToken)
  if (!value) {
    throw new Error("无权查看或权限已失效")
  }
  return value
}

export const customerKeys = {
  all: ["customers"] as const,
  directory: (query: CustomerDirectoryQuery) =>
    [...customerKeys.all, "directory", query] as const,
  detail: (customerId: string) =>
    [...customerKeys.all, "detail", customerId] as const,
}

export function useCustomerDirectoryQuery(query: CustomerDirectoryQuery) {
  return useQuery({
    queryKey: customerKeys.directory(query),
    queryFn: () => fetchCustomerDirectory(query),
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
