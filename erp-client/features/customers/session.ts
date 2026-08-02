/**
 * W03 session-only mutable state (create/revise customer).
 * Keeps form retention semantics for conflict/unknown without a real server.
 */

import type {
  CreateCustomerInput,
  CustomerCenterView,
  CustomerDirectoryItem,
  CustomerMutationResult,
  SaveCustomerRevisionInput,
} from "@/features/customers/types"
import {
  MOCK_CUSTOMER_DETAILS,
  MOCK_CUSTOMER_DIRECTORY,
} from "@/mock/customers"

type RevisionOverlay = {
  lockVersion: number
  revisionId: string
  revisionNo: number
  legalName: string
  shortName?: string
  unifiedCreditCode?: string
  effectiveFrom: string
  changeReason: string
  actor: string
}

type CreatedCustomer = {
  directory: CustomerDirectoryItem
  detail: CustomerCenterView
}

const revisionOverlays = new Map<string, RevisionOverlay>()
const createdCustomers = new Map<string, CreatedCustomer>()
const idempotencyResults = new Map<string, CustomerMutationResult>()

/** Demo toggle: when true, directory reports no customer data scope. */
let noCustomerScope = false

export function setW03NoCustomerScope(value: boolean): void {
  noCustomerScope = value
}

export function getW03NoCustomerScope(): boolean {
  return noCustomerScope
}

export function listW03DirectoryBaseline(): CustomerDirectoryItem[] {
  const created = [...createdCustomers.values()].map((c) => c.directory)
  const base = MOCK_CUSTOMER_DIRECTORY.map((item) => {
    const overlay = revisionOverlays.get(item.id)
    if (!overlay) return item
    return {
      ...item,
      legalName: overlay.legalName,
      shortName: overlay.shortName ?? item.shortName,
      updatedAt: overlay.effectiveFrom.slice(0, 10),
    }
  })
  return [...created, ...base]
}

export function getW03DetailBaseline(
  customerId: string
): CustomerCenterView | null {
  const created = createdCustomers.get(customerId)
  if (created) return applyRevisionOverlay(created.detail)
  const base = MOCK_CUSTOMER_DETAILS[customerId]
  if (!base) return null
  return applyRevisionOverlay(base)
}

function applyRevisionOverlay(
  detail: CustomerCenterView
): CustomerCenterView {
  const overlay = revisionOverlays.get(detail.customerId)
  if (!overlay) return detail

  return {
    ...detail,
    lockVersion: overlay.lockVersion,
    currentRevision: {
      ...detail.currentRevision,
      revisionId: overlay.revisionId,
      revisionNo: overlay.revisionNo,
      legalName: overlay.legalName,
      shortName: overlay.shortName,
      unifiedCreditCode: overlay.unifiedCreditCode,
      effectiveFrom: overlay.effectiveFrom,
    },
    revisionTimeline: [
      {
        id: overlay.revisionId,
        revisionNo: overlay.revisionNo,
        actor: overlay.actor,
        effectiveAt: overlay.effectiveFrom,
        reason: overlay.changeReason,
        isCurrent: true,
      },
      ...detail.revisionTimeline.map((entry) => ({
        ...entry,
        isCurrent: false,
      })),
    ],
  }
}

export function saveW03CustomerRevision(
  input: SaveCustomerRevisionInput
): CustomerMutationResult {
  const existing = idempotencyResults.get(input.idempotencyKey)
  if (existing) return existing

  if (input.simulate === "unknown") {
    const result: CustomerMutationResult = {
      outcome: "unknown",
      message:
        "提交结果不确定：未确认服务端是否已生成新版本。请查询最终结果后再决定是否重试（同一任务号）。",
      idempotencyKey: input.idempotencyKey,
    }
    // Do not cache unknown as terminal success — allow re-query.
    return result
  }

  const detail = getW03DetailBaseline(input.customerId)
  if (!detail) {
    const result: CustomerMutationResult = {
      outcome: "unknown",
      message: "客户不存在或无权访问。",
      idempotencyKey: input.idempotencyKey,
    }
    return result
  }

  if (
    input.simulate === "conflict" ||
    input.expectedLockVersion !== detail.lockVersion
  ) {
    const result: CustomerMutationResult = {
      outcome: "conflict",
      message: "主数据版本已变化，禁止静默覆盖。请查看服务端新版本后重做。",
      serverLockVersion: detail.lockVersion,
      serverRevisionNo: detail.currentRevision.revisionNo,
      serverLegalName: detail.currentRevision.legalName,
      serverShortName: detail.currentRevision.shortName,
      serverUnifiedCreditCode: detail.currentRevision.unifiedCreditCode,
      actor: "王敏",
      changedAt: detail.currentRevision.effectiveFrom,
    }
    idempotencyResults.set(input.idempotencyKey, result)
    return result
  }

  const now = new Date().toISOString()
  const nextRevisionNo = detail.currentRevision.revisionNo + 1
  const nextLock = detail.lockVersion + 1
  const revisionId = `rev_${input.customerId}_${nextRevisionNo}`

  revisionOverlays.set(input.customerId, {
    lockVersion: nextLock,
    revisionId,
    revisionNo: nextRevisionNo,
    legalName: input.legalName.trim(),
    shortName: input.shortName?.trim() || undefined,
    unifiedCreditCode: input.unifiedCreditCode?.trim() || undefined,
    effectiveFrom: now,
    changeReason: input.changeReason.trim(),
    actor: "当前用户",
  })

  const result: CustomerMutationResult = {
    outcome: "succeeded",
    customerId: input.customerId,
    customerNo: detail.customerNo,
    revisionNo: nextRevisionNo,
    lockVersion: nextLock,
    occurredAt: now,
    reference: `CUST-REV-${nextRevisionNo}-${Date.now().toString(36).toUpperCase()}`,
  }
  idempotencyResults.set(input.idempotencyKey, result)
  return result
}

export function createW03Customer(
  input: CreateCustomerInput
): CustomerMutationResult {
  const existing = idempotencyResults.get(input.idempotencyKey)
  if (existing) return existing

  if (input.simulate === "unknown") {
    return {
      outcome: "unknown",
      message:
        "创建结果不确定：请按原任务号查询，勿重复生成主体。",
      idempotencyKey: input.idempotencyKey,
    }
  }

  if (input.simulate === "conflict") {
    const result: CustomerMutationResult = {
      outcome: "conflict",
      message:
        "存在相似主体候选（演示冲突），未自动合并。请选择既有客户或提交人工确认。",
      serverLockVersion: 0,
      serverRevisionNo: 0,
      serverLegalName: input.legalName,
      actor: "查重服务",
      changedAt: new Date().toISOString(),
    }
    idempotencyResults.set(input.idempotencyKey, result)
    return result
  }

  const now = new Date().toISOString()
  const seq = 500 + createdCustomers.size + 1
  const customerId = `cust_new_${seq}`
  const customerNo = `KH-${String(seq).padStart(6, "0")}`
  const partyId = `party_new_${seq}`

  const directory: CustomerDirectoryItem = {
    id: customerId,
    partyId,
    customerNo,
    legalName: input.legalName.trim(),
    shortName: input.shortName?.trim() || undefined,
    status: "active",
    statusLabel: { label: "启用", tone: "success" },
    ownerName: input.ownerName,
    collaboratorCount: 0,
    scopeTags: ["mine", "team"],
    metrics: {
      activeContractCount: 0,
      inProgressSalesOrderCount: 0,
      receivableBalance: "0.00",
      overdueAmount: "0.00",
    },
    updatedAt: now.slice(0, 10),
    recentBusinessAt: now,
  }

  const detail: CustomerCenterView = {
    customerId,
    partyId,
    customerNo,
    status: "active",
    statusLabel: { label: "启用", tone: "success" },
    lockVersion: 1,
    currentRevision: {
      revisionId: `rev_${customerId}_1`,
      revisionNo: 1,
      legalName: input.legalName.trim(),
      shortName: input.shortName?.trim() || undefined,
      unifiedCreditCode: input.unifiedCreditCode?.trim() || undefined,
      defaultPaymentTerm: input.defaultPaymentTerm,
      effectiveFrom: now,
    },
    assignments: [
      {
        id: `asg_${customerId}_owner`,
        role: "OWNER",
        userId: input.ownerUserId,
        userName: input.ownerName,
        effectiveFrom: now.slice(0, 10),
        isCurrent: true,
      },
    ],
    contacts: [],
    addresses: [],
    bankAccounts: [],
    metrics: directory.metrics,
    contracts: [],
    salesOrders: [],
    receivableSummary: {
      receivableBalance: "0.00",
      overdueAmount: "0.00",
    },
    freshness: { formalFactsAt: now },
    allowedActions: [
      "EDIT_CUSTOMER",
      "UPLOAD_CONTRACT_PDF",
      "CREATE_SALES_ORDER",
      "OPEN_RECEIVABLE",
    ],
    actionBlockers: [],
    revisionTimeline: [
      {
        id: `rev_${customerId}_1`,
        revisionNo: 1,
        actor: input.ownerName,
        effectiveAt: now,
        reason: "首版建档",
        isCurrent: true,
      },
    ],
    partitions: {
      identity: "ok",
      contacts: "ok",
      related: "ok",
      settlement: "ok",
      quality: "ok",
      audit: "ok",
    },
  }

  createdCustomers.set(customerId, { directory, detail })

  const result: CustomerMutationResult = {
    outcome: "succeeded",
    customerId,
    customerNo,
    revisionNo: 1,
    lockVersion: 1,
    occurredAt: now,
    reference: `CUST-NEW-${customerNo}`,
  }
  idempotencyResults.set(input.idempotencyKey, result)
  return result
}

export function queryW03Idempotency(
  idempotencyKey: string
): CustomerMutationResult | null {
  return idempotencyResults.get(idempotencyKey) ?? null
}
