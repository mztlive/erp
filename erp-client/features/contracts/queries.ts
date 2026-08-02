"use client"

import {
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query"

import {
  createContractExportJob,
  fetchContractCenter,
  fetchContracts,
  fetchContractsForNewSalesOrder,
  uploadContractPdf,
} from "@/features/contracts/api"

export const contractKeys = {
  all: ["contracts"] as const,
  list: () => [...contractKeys.all, "list"] as const,
  detail: (id: string) => [...contractKeys.all, "detail", id] as const,
  selectable: () => [...contractKeys.all, "selectable-for-so"] as const,
}

export function useContractsQuery() {
  return useQuery({
    queryKey: contractKeys.list(),
    queryFn: fetchContracts,
  })
}

export function useContractCenterQuery(contractId: string) {
  return useQuery({
    queryKey: contractKeys.detail(contractId),
    queryFn: () => fetchContractCenter(contractId),
    enabled: Boolean(contractId),
  })
}

export function useContractsForNewSalesOrderQuery(enabled = true) {
  return useQuery({
    queryKey: contractKeys.selectable(),
    queryFn: fetchContractsForNewSalesOrder,
    enabled,
  })
}

export function useUploadContractPdfMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: uploadContractPdf,
    onSuccess: async (data) => {
      await queryClient.invalidateQueries({ queryKey: contractKeys.list() })
      await queryClient.invalidateQueries({
        queryKey: contractKeys.detail(data.contractId),
      })
      await queryClient.invalidateQueries({
        queryKey: contractKeys.selectable(),
      })
    },
  })
}

export function useCreateContractExportJobMutation() {
  return useMutation({
    mutationFn: createContractExportJob,
  })
}
