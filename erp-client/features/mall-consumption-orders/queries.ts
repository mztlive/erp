"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import {
  createConsumptionOrderExportJob,
  fetchConsumptionOrderDetail,
  fetchConsumptionOrderList,
  fetchSalesOrderConsumptionSummary,
} from "@/features/mall-consumption-orders/api"
import type {
  ExportCommand,
  MallConsumptionOrderListQuery,
} from "@/features/mall-consumption-orders/types"

export const consumptionOrderKeys = {
  all: ["mall-consumption-orders"] as const,
  list: (query: MallConsumptionOrderListQuery) =>
    [...consumptionOrderKeys.all, "list", query] as const,
  detail: (mallOrderId: string) =>
    [...consumptionOrderKeys.all, "detail", mallOrderId] as const,
  salesOrderSummary: (salesOrderId: string) =>
    [...consumptionOrderKeys.all, "sales-order-summary", salesOrderId] as const,
}

export function useSalesOrderConsumptionSummaryQuery(salesOrderId: string) {
  return useQuery({
    queryKey: consumptionOrderKeys.salesOrderSummary(salesOrderId),
    queryFn: () => fetchSalesOrderConsumptionSummary(salesOrderId),
  })
}

export function useConsumptionOrderListQuery(
  query: MallConsumptionOrderListQuery,
  options?: { enabled?: boolean }
) {
  return useQuery({
    queryKey: consumptionOrderKeys.list(query),
    queryFn: () => fetchConsumptionOrderList(query),
    enabled: options?.enabled,
  })
}

export function useConsumptionOrderDetailQuery(mallOrderId: string | null) {
  return useQuery({
    queryKey: consumptionOrderKeys.detail(mallOrderId ?? ""),
    queryFn: () => fetchConsumptionOrderDetail(mallOrderId!),
    enabled: Boolean(mallOrderId),
  })
}

export function useConsumptionOrderExportMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (command: ExportCommand) =>
      createConsumptionOrderExportJob(command),
    onSuccess: async () => {
      await queryClient.invalidateQueries({
        queryKey: consumptionOrderKeys.all,
      })
    },
  })
}
