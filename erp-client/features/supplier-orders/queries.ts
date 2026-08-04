"use client"

import {
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query"

import {
  addCollaborationNote,
  deferSupplierOrderTask,
  fetchSupplierOrderDetail,
  fetchSupplierOrders,
  querySupplierResult,
  replaySupplierOrder,
  revealSupplierOrderAddress,
  submitAfterSalesAction,
} from "@/features/supplier-orders/api"
import type {
  DemoRole,
  SupplierOrderListQuery,
} from "@/features/supplier-orders/types"

export const supplierOrderKeys = {
  all: ["supplier-orders"] as const,
  list: (query: SupplierOrderListQuery) =>
    [...supplierOrderKeys.all, "list", query] as const,
  detail: (orderId: string, role: DemoRole) =>
    [
      ...supplierOrderKeys.all,
      "detail",
      orderId,
      role,
    ] as const,
}

export function useSupplierOrdersQuery(query: SupplierOrderListQuery) {
  return useQuery({
    queryKey: supplierOrderKeys.list(query),
    queryFn: () => fetchSupplierOrders(query),
  })
}

export function useSupplierOrderDetailQuery(input: {
  orderId: string
  role?: DemoRole
  enabled?: boolean
}) {
  const role = input.role ?? "procurement"
  return useQuery({
    queryKey: supplierOrderKeys.detail(input.orderId, role),
    queryFn: () =>
      fetchSupplierOrderDetail({
        orderId: input.orderId,
        role,
      }),
    enabled: input.enabled !== false && Boolean(input.orderId),
  })
}

function useInvalidateOrders() {
  const queryClient = useQueryClient()
  return async () => {
    await queryClient.invalidateQueries({
      queryKey: supplierOrderKeys.all,
    })
  }
}

export function useQueryResultMutation() {
  const invalidate = useInvalidateOrders()
  return useMutation({
    mutationFn: querySupplierResult,
    onSuccess: async (result) => {
      if (result.status === "succeeded" || result.status === "unknown") {
        await invalidate()
      }
    },
  })
}

export function useReplayOrderMutation() {
  const invalidate = useInvalidateOrders()
  return useMutation({
    mutationFn: replaySupplierOrder,
    onSuccess: async (result) => {
      if (result.status === "succeeded") await invalidate()
    },
  })
}

export function useDeferOrderTaskMutation() {
  const invalidate = useInvalidateOrders()
  return useMutation({
    mutationFn: deferSupplierOrderTask,
    onSuccess: async (result) => {
      if (result.status === "succeeded") await invalidate()
    },
  })
}

export function useAfterSalesActionMutation() {
  const invalidate = useInvalidateOrders()
  return useMutation({
    mutationFn: submitAfterSalesAction,
    onSuccess: async (result) => {
      if (result.status === "succeeded") await invalidate()
    },
  })
}

export function useRevealAddressMutation() {
  const invalidate = useInvalidateOrders()
  return useMutation({
    mutationFn: revealSupplierOrderAddress,
    onSuccess: async (result) => {
      if (result.status === "succeeded") await invalidate()
    },
  })
}

export function useAddNoteMutation() {
  const invalidate = useInvalidateOrders()
  return useMutation({
    mutationFn: addCollaborationNote,
    onSuccess: async (result) => {
      if (result.status === "succeeded") await invalidate()
    },
  })
}
