"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import { mockDelay } from "@/lib/mock-delay"
import {
  getW12PayableDetail,
  openW12AllocationSession,
  postW12Invoice,
  postW12Payment,
  queryW12SupplierAccounts,
  resolveW12Unknown,
  reverseW12Invoice,
  reverseW12Payment,
  saveW12AllocationDraft,
  setW12PolicyState,
  revokeW12Permission,
  restoreW12Permission,
} from "@/mock/session-state"
import type {
  AllocationSessionView,
  AllocationTrack,
  FormalSubmitResult,
  PayableDetailView,
  PostInvoiceInput,
  PostPaymentInput,
  ReverseInvoiceInput,
  ReversePaymentInput,
  SaveAllocationDraftInput,
  SupplierAccountsListView,
  SupplierAccountsQuery,
} from "@/features/supplier-payables/types"
import { purchaseOrderKeys } from "@/features/purchase-orders/queries"
import { fulfillmentKeys } from "@/features/fulfillment-operations/queries"

async function fetchSupplierAccounts(
  query: SupplierAccountsQuery
): Promise<SupplierAccountsListView> {
  await mockDelay()
  return queryW12SupplierAccounts(query)
}

async function fetchPayableDetail(
  payableAccountId: string
): Promise<PayableDetailView | null> {
  await mockDelay()
  return getW12PayableDetail(payableAccountId)
}

async function fetchAllocationSession(input: {
  track: AllocationTrack
  supplierId: string
  draftSessionId?: string
  purchaseOrderId?: string
  returnTo?: string
  fromWorkspace?: string
  existingPaymentId?: string
  existingInvoiceId?: string
  preselectPayableAccountId?: string
}): Promise<AllocationSessionView> {
  await mockDelay()
  return openW12AllocationSession(input)
}

async function saveAllocationDraft(
  input: SaveAllocationDraftInput
): Promise<{ savedAt: string }> {
  await mockDelay(40)
  return saveW12AllocationDraft(input)
}

async function submitPayment(input: PostPaymentInput): Promise<FormalSubmitResult> {
  await mockDelay(120)
  return postW12Payment(input)
}

async function submitInvoice(input: PostInvoiceInput): Promise<FormalSubmitResult> {
  await mockDelay(120)
  return postW12Invoice(input)
}

async function reversePayment(
  input: ReversePaymentInput
): Promise<FormalSubmitResult> {
  await mockDelay(100)
  return reverseW12Payment(input)
}

async function reverseInvoice(
  input: ReverseInvoiceInput
): Promise<FormalSubmitResult> {
  await mockDelay(100)
  return reverseW12Invoice(input)
}

async function resolveUnknownResult(
  idempotencyKey: string
): Promise<FormalSubmitResult | null> {
  await mockDelay(80)
  return resolveW12Unknown(idempotencyKey)
}

/** Demo helpers for acceptance of policy/permission states */
async function demoSetPolicyState(
  state: "AVAILABLE" | "MISSING" | "STALE"
): Promise<void> {
  await mockDelay(20)
  setW12PolicyState(state)
}

async function demoRevokePermission(): Promise<void> {
  await mockDelay(20)
  revokeW12Permission()
}

async function demoRestorePermission(): Promise<void> {
  await mockDelay(20)
  restoreW12Permission()
}

export const supplierPayablesKeys = {
  all: ["supplier-payables"] as const,
  list: (query: SupplierAccountsQuery) =>
    [...supplierPayablesKeys.all, "list", query] as const,
  detail: (payableAccountId: string) =>
    [...supplierPayablesKeys.all, "detail", payableAccountId] as const,
  session: (params: {
    track: AllocationTrack
    supplierId: string
    draftSessionId?: string
    purchaseOrderId?: string
    existingPaymentId?: string
    existingInvoiceId?: string
  }) => [...supplierPayablesKeys.all, "session", params] as const,
}

export function useSupplierAccountsQuery(query: SupplierAccountsQuery) {
  return useQuery({
    queryKey: supplierPayablesKeys.list(query),
    queryFn: () => fetchSupplierAccounts(query),
  })
}

export function usePayableDetailQuery(payableAccountId: string | null) {
  return useQuery({
    queryKey: supplierPayablesKeys.detail(payableAccountId ?? ""),
    queryFn: () => fetchPayableDetail(payableAccountId!),
    enabled: Boolean(payableAccountId),
  })
}

export function useAllocationSessionQuery(
  params: {
    track: AllocationTrack
    supplierId: string
    draftSessionId?: string
    purchaseOrderId?: string
    returnTo?: string
    fromWorkspace?: string
    existingPaymentId?: string
    existingInvoiceId?: string
    preselectPayableAccountId?: string
  } | null
) {
  return useQuery({
    queryKey: supplierPayablesKeys.session(
      params
        ? {
            track: params.track,
            supplierId: params.supplierId,
            draftSessionId: params.draftSessionId,
            purchaseOrderId: params.purchaseOrderId,
            existingPaymentId: params.existingPaymentId,
            existingInvoiceId: params.existingInvoiceId,
          }
        : { track: "payment", supplierId: "" }
    ),
    queryFn: () => fetchAllocationSession(params!),
    enabled: Boolean(params?.supplierId && params.track),
  })
}

async function invalidateFinanceAndSources(
  queryClient: ReturnType<typeof useQueryClient>
) {
  await queryClient.invalidateQueries({ queryKey: supplierPayablesKeys.all })
  await queryClient.invalidateQueries({ queryKey: purchaseOrderKeys.all })
  await queryClient.invalidateQueries({ queryKey: fulfillmentKeys.all })
}

export function useSubmitPaymentMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: submitPayment,
    onSuccess: async (result) => {
      if (result.status === "succeeded") {
        await invalidateFinanceAndSources(queryClient)
      }
    },
  })
}

export function useSubmitInvoiceMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: submitInvoice,
    onSuccess: async (result) => {
      if (result.status === "succeeded") {
        await invalidateFinanceAndSources(queryClient)
      }
    },
  })
}

export function useReversePaymentMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: reversePayment,
    onSuccess: async (result) => {
      if (result.status === "succeeded") {
        await invalidateFinanceAndSources(queryClient)
      }
    },
  })
}

export function useReverseInvoiceMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: reverseInvoice,
    onSuccess: async (result) => {
      if (result.status === "succeeded") {
        await invalidateFinanceAndSources(queryClient)
      }
    },
  })
}

export function useSaveAllocationDraftMutation() {
  return useMutation({
    mutationFn: saveAllocationDraft,
  })
}

export function useResolveUnknownMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: resolveUnknownResult,
    onSuccess: async (result) => {
      if (result?.status === "succeeded") {
        await invalidateFinanceAndSources(queryClient)
      }
    },
  })
}

export function useDemoSetPolicyMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: demoSetPolicyState,
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: supplierPayablesKeys.all })
    },
  })
}

export function useDemoPermissionMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (action: "revoke" | "restore") => {
      if (action === "revoke") await demoRevokePermission()
      else await demoRestorePermission()
    },
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: supplierPayablesKeys.all })
    },
  })
}
