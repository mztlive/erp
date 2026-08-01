"use client"

import {
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query"

import {
  activateContract,
  createContractDraft,
  createContractExportJob,
  fetchContractCenter,
  fetchContracts,
  fetchContractsForNewSalesOrder,
  reviseContract,
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

export function useCreateContractDraftMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: createContractDraft,
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: contractKeys.list() })
    },
  })
}

export function useActivateContractMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: activateContract,
    onSuccess: async (data) => {
      await queryClient.invalidateQueries({
        queryKey: contractKeys.detail(data.contractId),
      })
      await queryClient.invalidateQueries({ queryKey: contractKeys.list() })
      await queryClient.invalidateQueries({
        queryKey: contractKeys.selectable(),
      })
    },
  })
}

export function useReviseContractMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: reviseContract,
    onSuccess: async (data) => {
      await queryClient.invalidateQueries({
        queryKey: contractKeys.detail(data.contractId),
      })
      await queryClient.invalidateQueries({ queryKey: contractKeys.list() })
    },
  })
}

export function useCreateContractExportJobMutation() {
  return useMutation({
    mutationFn: createContractExportJob,
  })
}
