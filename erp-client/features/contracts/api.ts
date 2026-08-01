import { mockDelay } from "@/features/workspace-kit/delay"
import type {
  ActivateContractResult,
  ContractCenterView,
  ContractExportJob,
  ContractListRow,
  CreateContractDraftResult,
  ReviseContractResult,
} from "@/features/contracts/types"
import {
  activateW04Contract,
  createW04ContractDraft,
  createW04ExportJob,
  getW04ContractCenter,
  listW04Contracts,
  reviseW04Contract,
} from "@/mock/session-state"
import { listContractsSelectableForNewSalesOrder } from "@/mock/contracts"

export const CONTRACT_PERMISSION_VERSION = "pv-w04-demo-1"

export async function fetchContracts(): Promise<ContractListRow[]> {
  await mockDelay(80)
  return listW04Contracts()
}

export async function fetchContractCenter(
  contractId: string
): Promise<ContractCenterView | null> {
  await mockDelay(80)
  return getW04ContractCenter(contractId)
}

/** 新销售单合同选择器：到期/终止/草稿不在候选中。 */
export async function fetchContractsForNewSalesOrder(): Promise<
  ContractListRow[]
> {
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

export async function createContractDraft(input: {
  customerName: string
  settlementPartyName: string
  validFrom: string
  validTo: string
  idempotencyKey: string
}): Promise<CreateContractDraftResult> {
  await mockDelay(160)
  return createW04ContractDraft(input)
}

export async function activateContract(input: {
  contractId: string
  expectedLockVersion: number
  idempotencyKey: string
}): Promise<ActivateContractResult> {
  await mockDelay(200)
  return activateW04Contract(input)
}

export async function reviseContract(input: {
  contractId: string
  expectedLockVersion: number
  idempotencyKey: string
}): Promise<ReviseContractResult> {
  await mockDelay(200)
  return reviseW04Contract(input)
}

export async function createContractExportJob(input: {
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
