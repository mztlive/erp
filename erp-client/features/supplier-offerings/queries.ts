"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import {
  createSupplierOffering,
  fetchCompanySkuOptions,
  fetchSupplierOfferings,
  reviseSupplierOffering,
  updateSupplierOfferingAvailability,
} from "@/features/supplier-offerings/api"
import type {
  CreateSupplierOfferingInput,
  ReviseSupplierOfferingInput,
  SupplierOfferingListQuery,
  UpdateOfferingAvailabilityInput,
} from "@/features/supplier-offerings/types"

export const supplierOfferingKeys = {
  all: ["supplier-offerings"] as const,
  list: (query: SupplierOfferingListQuery) =>
    [...supplierOfferingKeys.all, "list", query] as const,
  companySkus: () => [...supplierOfferingKeys.all, "company-skus"] as const,
}

export function useSupplierOfferingsQuery(query: SupplierOfferingListQuery) {
  return useQuery({
    queryKey: supplierOfferingKeys.list(query),
    queryFn: () => fetchSupplierOfferings(query),
  })
}

export function useCompanySkuOptionsQuery(enabled = true) {
  return useQuery({
    queryKey: supplierOfferingKeys.companySkus(),
    queryFn: fetchCompanySkuOptions,
    enabled,
  })
}

function useInvalidateOfferingData() {
  const queryClient = useQueryClient()
  return async () => {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: supplierOfferingKeys.all }),
      queryClient.invalidateQueries({ queryKey: ["master-data"] }),
    ])
  }
}

export function useCreateSupplierOfferingMutation() {
  const invalidate = useInvalidateOfferingData()
  return useMutation({
    mutationFn: (input: CreateSupplierOfferingInput) =>
      createSupplierOffering(input),
    onSuccess: invalidate,
  })
}

export function useReviseSupplierOfferingMutation() {
  const invalidate = useInvalidateOfferingData()
  return useMutation({
    mutationFn: (input: ReviseSupplierOfferingInput) =>
      reviseSupplierOffering(input),
    onSuccess: invalidate,
  })
}

export function useUpdateOfferingAvailabilityMutation() {
  const invalidate = useInvalidateOfferingData()
  return useMutation({
    mutationFn: (input: UpdateOfferingAvailabilityInput) =>
      updateSupplierOfferingAvailability(input),
    onSuccess: invalidate,
  })
}
