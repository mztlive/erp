/**
 * W11 客户往来 session mock（草稿 / 过账 / 冲正 / 幂等）。
 * 不进 query 响应的 claim 类令牌；正式余额由 post 后投影更新。
 */

import {
  W11_INVOICES,
  W11_RECEIPTS,
  W11_RECEIVABLES,
  W11_COUNTERPARTIES,
  type SeedAllocation,
  type SeedInvoice,
  type SeedReceipt,
  type SeedReceivable,
} from "@/mock/customer-receivables"
import type {
  AllocationDraftLine,
  AllocationMode,
  AllocationSessionView,
  CreateSessionInput,
  PostAllocationInput,
  PostAllocationResult,
  ReverseFactInput,
  ReverseFactResult,
  SaveAllocationDraftInput,
} from "@/features/customer-receivables/types"

function money(n: number): string {
  return n.toFixed(2)
}

function parseMoney(v: string | undefined): number {
  const n = Number(v)
  return Number.isFinite(n) ? n : 0
}

function cloneReceivables(): SeedReceivable[] {
  return W11_RECEIVABLES.map((r) => ({
    ...r,
    entries: r.entries.map((e) => ({ ...e })),
  }))
}

function cloneReceipts(): SeedReceipt[] {
  return W11_RECEIPTS.map((r) => ({
    ...r,
    allocations: r.allocations.map((a) => ({ ...a })),
  }))
}

function cloneInvoices(): SeedInvoice[] {
  return W11_INVOICES.map((i) => ({
    ...i,
    allocations: i.allocations.map((a) => ({ ...a })),
  }))
}

const liveReceivables = cloneReceivables()
let liveReceipts = cloneReceipts()
let liveInvoices = cloneInvoices()

const sessions = new Map<string, AllocationSessionView>()
const postIdempotency = new Map<string, PostAllocationResult>()
const reverseIdempotency = new Map<string, ReverseFactResult>()
const inFlightKeys = new Set<string>()

let permissionRevoked = false
let sessionSeq = 100
let receiptSeq = 200
let invoiceSeq = 300
let reverseSeq = 400
let operationSeq = 500

export function isW11PermissionRevoked(): boolean {
  return permissionRevoked
}

export function revokeW11Permission(): void {
  permissionRevoked = true
}

export function restoreW11Permission(): void {
  permissionRevoked = false
}

export function listW11LiveReceivables(): readonly SeedReceivable[] {
  return liveReceivables
}

export function listW11LiveReceipts(): readonly SeedReceipt[] {
  return liveReceipts
}

export function listW11LiveInvoices(): readonly SeedInvoice[] {
  return liveInvoices
}

export function getW11Receivable(accountId: string): SeedReceivable | null {
  return liveReceivables.find((r) => r.accountId === accountId) ?? null
}

export function getW11Receipt(receiptId: string): SeedReceipt | null {
  return liveReceipts.find((r) => r.receiptId === receiptId) ?? null
}

export function getW11Invoice(invoiceId: string): SeedInvoice | null {
  return liveInvoices.find((i) => i.invoiceId === invoiceId) ?? null
}

function counterpartyOf(id: string) {
  return W11_COUNTERPARTIES.find((c) => c.counterpartyPartyId === id)
}

function buildPool(
  mode: AllocationMode,
  counterpartyPartyId: string
): AllocationSessionView["pool"] {
  const rows = liveReceivables.filter(
    (r) => r.counterpartyPartyId === counterpartyPartyId
  )
  if (mode === "receipt") {
    // 回款分配目标：receivable_entry
    return rows.flatMap((r) =>
      r.entries
        .filter((e) => e.direction === "increase")
        .map((e) => ({
          targetId: e.entryId,
          targetKind: "receivable_entry" as const,
          label: `${r.salesOrderNo} · ${e.entryType}`,
          salesOrderNo: r.salesOrderNo,
          // 简化：按账户开放余额均摊展示在主增分录上
          openAmount: r.openTotal,
          dueDate: e.dueDate,
          counterpartyPartyId: r.counterpartyPartyId,
          baselineVersion: r.baselineVersion,
        }))
        .filter((t) => parseMoney(t.openAmount) > 0)
    )
  }
  // 发票分配目标：receivable_account
  return rows
    .filter((r) => parseMoney(r.openInvoiceableTotal) > 0)
    .map((r) => ({
      targetId: r.accountId,
      targetKind: "receivable_account" as const,
      label: `应收子账 #${r.accountSeq} · ${r.salesOrderNo}`,
      salesOrderNo: r.salesOrderNo,
      openAmount: r.openInvoiceableTotal,
      dueDate: r.dueDate,
      counterpartyPartyId: r.counterpartyPartyId,
      baselineVersion: r.baselineVersion,
    }))
}

function recomputeProposed(
  factAmount: string,
  allocations: readonly AllocationDraftLine[]
): { proposedAllocatedTotal: string; proposedUnallocated: string } {
  const allocated = allocations.reduce((s, a) => s + parseMoney(a.amount), 0)
  const total = parseMoney(factAmount)
  return {
    proposedAllocatedTotal: money(allocated),
    proposedUnallocated: money(Math.max(0, total - allocated)),
  }
}

export function getW11AllocationSession(
  draftSessionId: string
): AllocationSessionView | null {
  const s = sessions.get(draftSessionId)
  if (!s) return null
  // 池始终按当前 live 余额刷新（服务端口径）
  const pool = buildPool(s.mode, s.counterpartyPartyId)
  const factAmount =
    s.mode === "receipt"
      ? s.fact.amount ?? "0"
      : s.fact.grossAmount ?? "0"
  const proposed = recomputeProposed(factAmount, s.allocations)
  return {
    ...s,
    pool,
    factAmount,
    ...proposed,
    leaseValid: !permissionRevoked,
  }
}

export function createW11AllocationSession(
  input: CreateSessionInput
): AllocationSessionView {
  if (permissionRevoked) {
    throw new Error("当前账号无客户往来登记/核销权限或权限已被收回。")
  }
  const cp = counterpartyOf(input.counterpartyPartyId)
  if (!cp) {
    throw new Error("往来主体不存在或不在授权范围。")
  }

  let existingFactNo: string | undefined
  let fact: AllocationSessionView["fact"] = {}
  let prefillAllocations: AllocationDraftLine[] = []

  if (input.mode === "receipt" && input.existingFactId) {
    const r = getW11Receipt(input.existingFactId)
    if (!r || r.status !== "posted") {
      throw new Error("只能对已过账且有余额的回款继续核销。")
    }
    if (r.counterpartyPartyId !== input.counterpartyPartyId) {
      throw new Error("回款往来主体与本次核销主体不一致。")
    }
    if (parseMoney(r.unallocatedAmount) <= 0) {
      throw new Error("该回款已无有效未分配余额。")
    }
    existingFactNo = r.receiptNo
    fact = {
      receivedAt: r.receivedAt.slice(0, 16),
      amount: r.unallocatedAmount,
      bankReference: r.bankReferenceMasked,
    }
  } else if (input.mode === "invoice" && input.existingFactId) {
    const inv = getW11Invoice(input.existingFactId)
    if (!inv || inv.status !== "registered" || inv.invoiceKind !== "blue") {
      throw new Error("只能对已登记蓝票且有余额的发票继续核销。")
    }
    if (inv.counterpartyPartyId !== input.counterpartyPartyId) {
      throw new Error("发票往来主体与本次核销主体不一致。")
    }
    if (parseMoney(inv.unallocatedAmount) <= 0) {
      throw new Error("该发票已无有效未分配余额。")
    }
    existingFactNo = inv.invoiceNo
    fact = {
      invoiceCode: inv.invoiceCode,
      invoiceNo: inv.invoiceNo,
      invoiceDate: inv.invoiceDate,
      grossAmount: inv.unallocatedAmount,
      netAmount: inv.netAmount,
      taxAmount: inv.taxAmount,
      invoiceKind: "blue",
    }
  } else {
    const now = new Date()
    const pad = (n: number) => String(n).padStart(2, "0")
    const local = `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}T${pad(now.getHours())}:${pad(now.getMinutes())}`
    if (input.mode === "receipt") {
      fact = { receivedAt: local, amount: "", bankReference: "" }
    } else {
      fact = {
        invoiceDate: local.slice(0, 10),
        invoiceNo: "",
        invoiceCode: "",
        grossAmount: "",
        netAmount: "",
        taxAmount: "",
        invoiceKind: "blue",
      }
    }
  }

  // W05 链入：预选应收
  if (input.receivableAccountId || input.salesOrderId) {
    const pool = buildPool(input.mode, input.counterpartyPartyId)
    const match = pool.find((p) => {
      if (input.receivableAccountId) {
        if (p.targetKind === "receivable_account") {
          return p.targetId === input.receivableAccountId
        }
        const acct = liveReceivables.find((r) =>
          r.entries.some((e) => e.entryId === p.targetId)
        )
        return acct?.accountId === input.receivableAccountId
      }
      if (input.salesOrderId) {
        const acct = liveReceivables.find(
          (r) =>
            r.salesOrderId === input.salesOrderId &&
            r.counterpartyPartyId === input.counterpartyPartyId
        )
        if (!acct) return false
        if (p.targetKind === "receivable_account") {
          return p.targetId === acct.accountId
        }
        return acct.entries.some((e) => e.entryId === p.targetId)
      }
      return false
    })
    if (match) {
      prefillAllocations = [
        {
          lineKey: `line_${match.targetId}`,
          targetId: match.targetId,
          targetKind: match.targetKind,
          label: match.label,
          salesOrderNo: match.salesOrderNo,
          openAmount: match.openAmount,
          amount: "",
          baselineVersion: match.baselineVersion,
        },
      ]
    }
  }

  const draftSessionId = `alloc_cust_${++sessionSeq}`
  const pool = buildPool(input.mode, input.counterpartyPartyId)
  const factAmount =
    input.mode === "receipt" ? fact.amount ?? "0" : fact.grossAmount ?? "0"
  const proposed = recomputeProposed(factAmount, prefillAllocations)

  const view: AllocationSessionView = {
    draftSessionId,
    mode: input.mode,
    counterpartyPartyId: cp.counterpartyPartyId,
    counterpartyPartyName: cp.counterpartyPartyName,
    customerId: cp.customerId,
    customerName: cp.customerName,
    status: "draft",
    existingFactId: input.existingFactId,
    existingFactNo,
    fact,
    pool,
    allocations: prefillAllocations,
    factAmount,
    ...proposed,
    submitPolicy: {
      allowUnallocatedRemainder: true,
      label: "允许保留未分配余额（系统统一判定）",
    },
    returnContext: {
      returnTo: input.returnTo,
      from: input.from,
      salesOrderId: input.salesOrderId,
    },
    leaseValid: true,
    editVersion: 1,
    note:
      "核销严格按 counterparty_party_id 锁定；池中仅同主体开放对象。拟分配合计仅作输入提示，不冒充核销。",
  }
  sessions.set(draftSessionId, view)
  return view
}

export function saveW11AllocationDraft(
  input: SaveAllocationDraftInput
): AllocationSessionView {
  if (permissionRevoked) {
    throw new Error("权限已收回，禁止保存草稿。")
  }
  const s = sessions.get(input.draftSessionId)
  if (!s || s.status !== "draft") {
    throw new Error("草稿已不存在或已过账。")
  }
  if (input.editVersion !== s.editVersion) {
    throw new Error("草稿数据已更新，请刷新后重试。")
  }

  // 拒绝跨主体目标进入分配（双重校验）
  for (const line of input.allocations) {
    const inPool = buildPool(s.mode, s.counterpartyPartyId).some(
      (p) => p.targetId === line.targetId
    )
    if (!inPool) {
      throw new Error(
        `目标 ${line.label} 不属于当前往来主体，已拒绝写入草稿。`
      )
    }
  }

  const next: AllocationSessionView = {
    ...s,
    fact: { ...input.fact },
    allocations: input.allocations.map((a) => ({ ...a })),
    editVersion: s.editVersion + 1,
    savedAt: new Date().toISOString(),
  }
  sessions.set(input.draftSessionId, next)
  return getW11AllocationSession(input.draftSessionId)!
}

export function postW11Allocation(
  input: PostAllocationInput
): PostAllocationResult {
  const cached = postIdempotency.get(input.idempotencyKey)
  if (cached) {
    inFlightKeys.delete(input.idempotencyKey)
    return cached
  }

  if (input.forceUnknown || inFlightKeys.has(input.idempotencyKey)) {
    inFlightKeys.add(input.idempotencyKey)
    const unknown: PostAllocationResult = {
      status: "unknown",
      message:
        "提交结果不确定。请按原任务号查询最终结果，勿重复过账。",
      idempotencyKey: input.idempotencyKey,
      operationId: `op_pending_${input.idempotencyKey.slice(-8)}`,
    }
    postIdempotency.set(input.idempotencyKey, unknown)
    return unknown
  }

  if (permissionRevoked) {
    const failed: PostAllocationResult = {
      status: "failed",
      code: "PERMISSION_REVOKED",
      message: "权限已收回，不能提交。",
    }
    postIdempotency.set(input.idempotencyKey, failed)
    return failed
  }

  const s = sessions.get(input.draftSessionId)
  if (!s || s.status !== "draft") {
    const failed: PostAllocationResult = {
      status: "failed",
      code: "SESSION_INVALID",
      message: "本次核销已不存在或已提交。",
    }
    postIdempotency.set(input.idempotencyKey, failed)
    return failed
  }
  if (input.editVersion !== s.editVersion) {
    const failed: PostAllocationResult = {
      status: "failed",
      code: "VERSION_CONFLICT",
      message: "草稿数据已更新，请保存或刷新后重试。",
    }
    return failed
  }

  if (input.forceCrossParty) {
    const failed: PostAllocationResult = {
      status: "failed",
      code: "CROSS_PARTY",
      message:
        "跨 counterparty_party_id 分配被拒绝。核销池不会返回跨主体目标，提交时再次校验。",
    }
    postIdempotency.set(input.idempotencyKey, failed)
    return failed
  }

  // 校验分配目标均属同主体
  const pool = buildPool(s.mode, s.counterpartyPartyId)
  for (const line of s.allocations) {
    if (parseMoney(line.amount) <= 0) continue
    const target = pool.find((p) => p.targetId === line.targetId)
    if (!target) {
      const failed: PostAllocationResult = {
        status: "failed",
        code: "CROSS_PARTY",
        message: `目标 ${line.label} 不在同主体待核销池中，提交已拒绝。`,
      }
      postIdempotency.set(input.idempotencyKey, failed)
      return failed
    }
    if (line.baselineVersion !== target.baselineVersion) {
      const failed: PostAllocationResult = {
        status: "failed",
        code: "BALANCE_CONFLICT",
        message: `目标 ${line.label} 开放余额已变化，请刷新后重新确认。`,
        refreshedTargets: pool.map((p) => ({
          targetId: p.targetId,
          openAmount: p.openAmount,
        })),
      }
      // 不缓存失败，允许用户刷新后重试
      return failed
    }
    if (parseMoney(line.amount) - parseMoney(target.openAmount) > 1e-9) {
      const failed: PostAllocationResult = {
        status: "failed",
        code: "OVER_ALLOCATE",
        message: `分配金额超过目标开放余额 ${target.openAmount}。`,
        refreshedTargets: [
          { targetId: target.targetId, openAmount: target.openAmount },
        ],
      }
      return failed
    }
  }

  const factAmount =
    s.mode === "receipt"
      ? parseMoney(s.fact.amount)
      : parseMoney(s.fact.grossAmount)
  if (factAmount <= 0 && !s.existingFactId) {
    return {
      status: "failed",
      code: "INVALID_AMOUNT",
      message: "记录金额必须为正数。",
    }
  }

  const allocSum = s.allocations.reduce(
    (sum, a) => sum + parseMoney(a.amount),
    0
  )
  if (allocSum - factAmount > 1e-9) {
    return {
      status: "failed",
      code: "OVER_ALLOCATE_FACT",
      message: "拟分配合计超过记录金额。",
    }
  }

  if (s.mode === "invoice" && !s.existingFactId) {
    const invNo = (s.fact.invoiceNo ?? "").trim()
    if (!invNo) {
      return {
        status: "failed",
        code: "INVOICE_NO_REQUIRED",
        message: "请填写发票号码。",
      }
    }
    const dup = liveInvoices.find(
      (i) =>
        i.invoiceNo === invNo &&
        (i.invoiceCode ?? "") === (s.fact.invoiceCode ?? "").trim() &&
        i.invoiceKind === (s.fact.invoiceKind ?? "blue")
    )
    if (dup) {
      const failed: PostAllocationResult = {
        status: "failed",
        code: "DUPLICATE_INVOICE",
        message: `发票已存在（${dup.invoiceNo}），不创建副本。可打开原票继续分配。`,
        existingInvoiceId: dup.invoiceId,
        existingInvoiceNo: dup.invoiceNo,
      }
      postIdempotency.set(input.idempotencyKey, failed)
      return failed
    }
  }

  const opId = `op_w11_${++operationSeq}`
  const now = new Date().toISOString()
  let factId = s.existingFactId ?? ""
  let factNo = s.existingFactNo ?? ""
  let unallocated = money(factAmount - allocSum)
  let allocatedTotal = money(allocSum)

  if (s.mode === "receipt") {
    if (s.existingFactId) {
      const idx = liveReceipts.findIndex((r) => r.receiptId === s.existingFactId)
      const receipt = liveReceipts[idx]
      if (!receipt) {
        return { status: "failed", code: "NOT_FOUND", message: "回款不存在" }
      }
      const newAllocs: SeedAllocation[] = s.allocations
        .filter((a) => parseMoney(a.amount) > 0)
        .map((a, i) => ({
          allocationId: `rall_sess_${receiptSeq}_${i}`,
          action: "APPLY" as const,
          amountGross: money(parseMoney(a.amount)),
          targetLabel: a.label,
          targetId: a.targetId,
          occurredAt: now,
        }))
      const nextAllocated =
        parseMoney(receipt.allocatedTotal) + allocSum
      const nextUnalloc = parseMoney(receipt.amount) - nextAllocated
      liveReceipts[idx] = {
        ...receipt,
        allocatedTotal: money(nextAllocated),
        unallocatedAmount: money(Math.max(0, nextUnalloc)),
        baselineVersion: receipt.baselineVersion + 1,
        allocations: [...receipt.allocations, ...newAllocs],
      }
      factId = receipt.receiptId
      factNo = receipt.receiptNo
      allocatedTotal = money(nextAllocated)
      unallocated = money(Math.max(0, nextUnalloc))
    } else {
      receiptSeq += 1
      factId = `rcpt_sess_${receiptSeq}`
      factNo = `SK-${now.slice(0, 10).replaceAll("-", "")}-${String(receiptSeq).padStart(3, "0")}`
      const newAllocs: SeedAllocation[] = s.allocations
        .filter((a) => parseMoney(a.amount) > 0)
        .map((a, i) => ({
          allocationId: `rall_sess_${receiptSeq}_${i}`,
          action: "APPLY" as const,
          amountGross: money(parseMoney(a.amount)),
          targetLabel: a.label,
          targetId: a.targetId,
          occurredAt: now,
        }))
      liveReceipts = [
        {
          receiptId: factId,
          receiptNo: factNo,
          counterpartyPartyId: s.counterpartyPartyId,
          counterpartyPartyName: s.counterpartyPartyName,
          customerId: s.customerId,
          customerName: s.customerName,
          receivedAt: (s.fact.receivedAt ?? now).replace("T", "T"),
          amount: money(factAmount),
          bankReferenceMasked: maskBank(s.fact.bankReference),
          allocatedTotal,
          unallocatedAmount: unallocated,
          status: "posted",
          baselineVersion: 1,
          allocations: newAllocs,
        },
        ...liveReceipts,
      ]
    }
    // 更新应收 settled / open（服务端事务投影）
    applyReceiptSettlements(s.allocations)
  } else {
    if (s.existingFactId) {
      const idx = liveInvoices.findIndex((i) => i.invoiceId === s.existingFactId)
      const inv = liveInvoices[idx]
      if (!inv) {
        return { status: "failed", code: "NOT_FOUND", message: "发票不存在" }
      }
      const newAllocs: SeedAllocation[] = s.allocations
        .filter((a) => parseMoney(a.amount) > 0)
        .map((a, i) => ({
          allocationId: `iall_sess_${invoiceSeq}_${i}`,
          action: "APPLY" as const,
          amountGross: money(parseMoney(a.amount)),
          targetLabel: a.label,
          targetId: a.targetId,
          occurredAt: now,
        }))
      const nextAllocated = parseMoney(inv.allocatedTotal) + allocSum
      const nextUnalloc = parseMoney(inv.grossAmount) - nextAllocated
      liveInvoices[idx] = {
        ...inv,
        allocatedTotal: money(nextAllocated),
        unallocatedAmount: money(Math.max(0, nextUnalloc)),
        baselineVersion: inv.baselineVersion + 1,
        allocations: [...inv.allocations, ...newAllocs],
      }
      factId = inv.invoiceId
      factNo = inv.invoiceNo
      allocatedTotal = money(nextAllocated)
      unallocated = money(Math.max(0, nextUnalloc))
    } else {
      invoiceSeq += 1
      factId = `inv_sess_${invoiceSeq}`
      factNo = (s.fact.invoiceNo ?? "").trim()
      const newAllocs: SeedAllocation[] = s.allocations
        .filter((a) => parseMoney(a.amount) > 0)
        .map((a, i) => ({
          allocationId: `iall_sess_${invoiceSeq}_${i}`,
          action: "APPLY" as const,
          amountGross: money(parseMoney(a.amount)),
          targetLabel: a.label,
          targetId: a.targetId,
          occurredAt: now,
        }))
      liveInvoices = [
        {
          invoiceId: factId,
          invoiceCode: s.fact.invoiceCode?.trim() || undefined,
          invoiceNo: factNo,
          invoiceKind: s.fact.invoiceKind ?? "blue",
          counterpartyPartyId: s.counterpartyPartyId,
          counterpartyPartyName: s.counterpartyPartyName,
          customerId: s.customerId,
          customerName: s.customerName,
          invoiceDate: s.fact.invoiceDate ?? now.slice(0, 10),
          grossAmount: money(factAmount),
          netAmount: s.fact.netAmount || money(factAmount / 1.13),
          taxAmount: s.fact.taxAmount || money(factAmount - factAmount / 1.13),
          allocatedTotal,
          unallocatedAmount: unallocated,
          status: "registered",
          originalInvoiceId: s.fact.originalInvoiceId,
          baselineVersion: 1,
          allocations: newAllocs,
        },
        ...liveInvoices,
      ]
    }
    applyInvoiceAllocations(s.allocations)
  }

  sessions.set(input.draftSessionId, { ...s, status: "posted" })

  const result: PostAllocationResult = {
    status: "succeeded",
    mode: s.mode,
    factId,
    factNo,
    allocatedTotal,
    unallocatedAmount: unallocated,
    operationId: opId,
    watermark: now,
    returnTo: s.returnContext?.returnTo,
  }
  postIdempotency.set(input.idempotencyKey, result)
  return result
}

export function resolveW11PostUnknown(
  idempotencyKey: string
): PostAllocationResult | null {
  const entry = postIdempotency.get(idempotencyKey)
  if (!entry) return null
  if (entry.status === "unknown") {
    // 演示：二次查询转为成功路径不可用时保留 unknown；若会话仍在则真正过账
    const sessionIdMatch = [...sessions.entries()].find(
      ([, s]) => s.status === "draft"
    )
    if (sessionIdMatch) {
      inFlightKeys.delete(idempotencyKey)
      postIdempotency.delete(idempotencyKey)
      return postW11Allocation({
        draftSessionId: sessionIdMatch[0],
        editVersion: sessionIdMatch[1].editVersion,
        idempotencyKey,
      })
    }
  }
  return entry
}

function maskBank(raw?: string): string {
  const v = (raw ?? "").trim()
  if (!v) return "****0000"
  if (v.includes("*")) return v
  if (v.length <= 4) return `****${v}`
  return `****${v.slice(-4)}`
}

function applyReceiptSettlements(
  allocations: readonly AllocationDraftLine[]
): void {
  for (const line of allocations) {
    const amt = parseMoney(line.amount)
    if (amt <= 0) continue
    // entry → account
    const idx = liveReceivables.findIndex((r) =>
      r.entries.some((e) => e.entryId === line.targetId)
    )
    if (idx < 0) continue
    const r = liveReceivables[idx]
    const settled = parseMoney(r.settledTotal) + amt
    const open = Math.max(0, parseMoney(r.grossTotal) - settled)
    liveReceivables[idx] = {
      ...r,
      settledTotal: money(settled),
      openTotal: money(open),
      status: open <= 0 ? "settled" : settled > 0 ? "partial" : "open",
      baselineVersion: r.baselineVersion + 1,
    }
  }
}

function applyInvoiceAllocations(
  allocations: readonly AllocationDraftLine[]
): void {
  for (const line of allocations) {
    const amt = parseMoney(line.amount)
    if (amt <= 0) continue
    const idx = liveReceivables.findIndex((r) => r.accountId === line.targetId)
    if (idx < 0) continue
    const r = liveReceivables[idx]
    const invoiced = parseMoney(r.invoicedTotal) + amt
    const openInv = Math.max(0, parseMoney(r.grossTotal) - invoiced)
    liveReceivables[idx] = {
      ...r,
      invoicedTotal: money(invoiced),
      openInvoiceableTotal: money(openInv),
      baselineVersion: r.baselineVersion + 1,
    }
  }
}

export function reverseW11Fact(input: ReverseFactInput): ReverseFactResult {
  const cached = reverseIdempotency.get(input.idempotencyKey)
  if (cached) return cached

  if (permissionRevoked) {
    const failed: ReverseFactResult = {
      status: "failed",
      code: "PERMISSION_REVOKED",
      message: "权限已收回，禁止发起纠错。",
    }
    reverseIdempotency.set(input.idempotencyKey, failed)
    return failed
  }

  reverseSeq += 1
  const opId = `op_w11_rev_${++operationSeq}`
  const now = new Date().toISOString()

  if (input.kind === "receipt_reverse" || input.kind === "refund") {
    const idx = liveReceipts.findIndex((r) => r.receiptId === input.sourceFactId)
    const receipt = liveReceipts[idx]
    if (!receipt || receipt.status !== "posted") {
      return {
        status: "failed",
        code: "INVALID_SOURCE",
        message: "仅可对已过账回款发起冲正/退款，原记录不可编辑删除。",
      }
    }
    // 追加反向分配，不改原 APPLY 行
    const reverseAllocs: SeedAllocation[] = receipt.allocations
      .filter((a) => a.action === "APPLY")
      .map((a, i) => ({
        allocationId: `rall_rev_${reverseSeq}_${i}`,
        action: "REVERSE" as const,
        amountGross: a.amountGross,
        targetLabel: a.targetLabel,
        targetId: a.targetId,
        occurredAt: now,
        reverseOfAllocationId: a.allocationId,
      }))
    // 反向影响应收
    for (const a of reverseAllocs) {
      const rIdx = liveReceivables.findIndex((r) =>
        r.entries.some((e) => e.entryId === a.targetId)
      )
      if (rIdx < 0) continue
      const r = liveReceivables[rIdx]
      const settled = Math.max(
        0,
        parseMoney(r.settledTotal) - parseMoney(a.amountGross)
      )
      const open = Math.max(0, parseMoney(r.grossTotal) - settled)
      liveReceivables[rIdx] = {
        ...r,
        settledTotal: money(settled),
        openTotal: money(open),
        status: open <= 0 ? "settled" : settled > 0 ? "partial" : "open",
        baselineVersion: r.baselineVersion + 1,
      }
    }
    liveReceipts[idx] = {
      ...receipt,
      status: "reversed",
      allocatedTotal: "0.00",
      unallocatedAmount: "0.00",
      baselineVersion: receipt.baselineVersion + 1,
      allocations: [...receipt.allocations, ...reverseAllocs],
    }
    // 追加反向回款记录（退款/冲正单）
    const reverseId = `rcpt_rev_${reverseSeq}`
    const reverseNo =
      input.kind === "refund"
        ? `TK-${String(reverseSeq).padStart(4, "0")}`
        : `CZ-${String(reverseSeq).padStart(4, "0")}`
    liveReceipts = [
      {
        receiptId: reverseId,
        receiptNo: reverseNo,
        counterpartyPartyId: receipt.counterpartyPartyId,
        counterpartyPartyName: receipt.counterpartyPartyName,
        customerId: receipt.customerId,
        customerName: receipt.customerName,
        receivedAt: now,
        amount: money(-parseMoney(receipt.amount)),
        bankReferenceMasked: receipt.bankReferenceMasked,
        allocatedTotal: money(-parseMoney(receipt.amount)),
        unallocatedAmount: "0.00",
        status: "posted",
        baselineVersion: 1,
        allocations: reverseAllocs.map((a) => ({
          ...a,
          allocationId: `${a.allocationId}_hdr`,
        })),
      },
      ...liveReceipts,
    ]
    const result: ReverseFactResult = {
      status: "succeeded",
      reverseFactId: reverseId,
      reverseFactNo: reverseNo,
      operationId: opId,
      message:
        input.kind === "refund"
          ? "已追加退款记录与反向分配，原回款保留。"
          : "已追加回款冲正记录与反向分配，原回款保留。",
    }
    reverseIdempotency.set(input.idempotencyKey, result)
    return result
  }

  // red invoice
  const idx = liveInvoices.findIndex((i) => i.invoiceId === input.sourceFactId)
  const inv = liveInvoices[idx]
  if (!inv || inv.status !== "registered" || inv.invoiceKind !== "blue") {
    return {
      status: "failed",
      code: "INVALID_SOURCE",
      message: "仅可对已登记蓝票发起红票，原票不可编辑删除。",
    }
  }
  const redAmount = parseMoney(input.amount ?? inv.allocatedTotal)
  if (redAmount <= 0 || redAmount - parseMoney(inv.allocatedTotal) > 1e-9) {
    return {
      status: "failed",
      code: "RED_AMOUNT",
      message: "红票金额须为正且不超过原票有效可红冲分配。",
    }
  }
  const reverseId = `inv_red_${reverseSeq}`
  const reverseNo = `R${inv.invoiceNo}`
  const reverseAllocs: SeedAllocation[] = inv.allocations
    .filter((a) => a.action === "APPLY")
    .map((a, i) => ({
      allocationId: `iall_red_${reverseSeq}_${i}`,
      action: "REVERSE" as const,
      amountGross: a.amountGross,
      targetLabel: a.targetLabel,
      targetId: a.targetId,
      occurredAt: now,
      reverseOfAllocationId: a.allocationId,
    }))
  for (const a of reverseAllocs) {
    const rIdx = liveReceivables.findIndex((r) => r.accountId === a.targetId)
    if (rIdx < 0) continue
    const r = liveReceivables[rIdx]
    const invoiced = Math.max(
      0,
      parseMoney(r.invoicedTotal) - parseMoney(a.amountGross)
    )
    const openInv = Math.max(0, parseMoney(r.grossTotal) - invoiced)
    liveReceivables[rIdx] = {
      ...r,
      invoicedTotal: money(invoiced),
      openInvoiceableTotal: money(openInv),
      baselineVersion: r.baselineVersion + 1,
    }
  }
  liveInvoices[idx] = {
    ...inv,
    allocations: [...inv.allocations, ...reverseAllocs],
    baselineVersion: inv.baselineVersion + 1,
  }
  liveInvoices = [
    {
      invoiceId: reverseId,
      invoiceCode: inv.invoiceCode,
      invoiceNo: reverseNo,
      invoiceKind: "red",
      counterpartyPartyId: inv.counterpartyPartyId,
      counterpartyPartyName: inv.counterpartyPartyName,
      customerId: inv.customerId,
      customerName: inv.customerName,
      invoiceDate: now.slice(0, 10),
      grossAmount: money(-redAmount),
      netAmount: money(-parseMoney(inv.netAmount)),
      taxAmount: money(-parseMoney(inv.taxAmount)),
      allocatedTotal: money(-redAmount),
      unallocatedAmount: "0.00",
      status: "registered",
      originalInvoiceId: inv.invoiceId,
      baselineVersion: 1,
      allocations: reverseAllocs,
    },
    ...liveInvoices,
  ]
  const result: ReverseFactResult = {
    status: "succeeded",
    reverseFactId: reverseId,
    reverseFactNo: reverseNo,
    operationId: opId,
    message: "已登记独立红票并追加反向分配，原蓝票保留。",
  }
  reverseIdempotency.set(input.idempotencyKey, result)
  return result
}

/** 演示：抬高某应收 baseline，制造并发余额冲突 */
export function bumpW11ReceivableBaseline(accountId: string): number {
  const idx = liveReceivables.findIndex((r) => r.accountId === accountId)
  if (idx < 0) return 0
  const r = liveReceivables[idx]
  const next = r.baselineVersion + 1
  liveReceivables[idx] = { ...r, baselineVersion: next }
  return next
}
