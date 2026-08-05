"use client"

import {
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query"

import { mockDelay } from "@/lib/mock-delay"
import {
  createW04ExportJob,
  getW04ContractCenter,
  listW04Contracts,
  uploadW04ContractPdf,
} from "@/mock/session-state"
import { listContractsSelectableForNewSalesOrder } from "@/mock/contracts"
import { contractPdfError } from "@/features/contracts/pdf"
import type {
  ContractCenterView,
  ContractExportJob,
  ContractListRow,
  UploadContractPdfInput,
  UploadContractPdfResult,
} from "@/features/contracts/types"

export const CONTRACT_PERMISSION_VERSION = "pv-w04-demo-1"

async function fetchContracts(): Promise<ContractListRow[]> {
  await mockDelay(80)
  return listW04Contracts()
}

async function fetchContractCenter(
  contractId: string
): Promise<ContractCenterView | null> {
  await mockDelay(80)
  return getW04ContractCenter(contractId)
}

/** 新销售单合同选择器：到期/终止/草稿不在候选中。 */
async function fetchContractsForNewSalesOrder(): Promise<ContractListRow[]> {
  await mockDelay(60)
  // 会话覆盖后以 list + center 判定为准
  const rows = listW04Contracts()
  return rows.filter((row) => {
    const center = getW04ContractCenter(row.contractId)
    if (center) return center.selectableForNewSalesOrder
    return listContractsSelectableForNewSalesOrder().some(
      (r) => r.contractId === row.contractId
    )
  })
}

async function uploadContractPdf(
  input: UploadContractPdfInput
): Promise<UploadContractPdfResult> {
  await mockDelay(160)
  const fileError = contractPdfError(input.pdfFile)
  if (fileError) throw new Error(fileError)
  return uploadW04ContractPdf(input)
}

async function createContractExportJob(input: {
  rowCount: number
  filterSnapshotLabel: string
}): Promise<ContractExportJob> {
  await mockDelay(180)
  return createW04ExportJob({
    rowCount: input.rowCount,
    permissionVersion: CONTRACT_PERMISSION_VERSION,
    filterSnapshotLabel: input.filterSnapshotLabel,
  })
}

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
