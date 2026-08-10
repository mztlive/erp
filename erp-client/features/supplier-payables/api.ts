/**
 * W12 供应商往来 API：真实 HTTP
 * (/admin/payable-accounts、supplier-payments、purchase-invoice-allocations、
 * payment-reversals、invoices?invoice_direction=purchase)。
 * 导出签名与 queries 原内联函数一致。
 */

import { apiGet, apiPost } from "@/lib/api"
import type { Page } from "@/lib/api"
import type {
    AllocationSessionView,
    AllocationTrack,
    FormalSubmitResult,
    PayableDetailView,
    PayableRow,
    PaymentRow,
    PostInvoiceInput,
    PostPaymentInput,
    PurchaseInvoiceRow,
    ReverseInvoiceInput,
    ReversePaymentInput,
    SaveAllocationDraftInput,
    SupplierAccountsListView,
    SupplierAccountsQuery,
    UnallocatedRow,
} from "@/features/supplier-payables/types"
import {
    PAYABLE_STATUS_LABEL,
    PAYABLE_STATUS_TONE,
    SOURCE_TYPE_LABEL,
    VIEW_LABEL,
} from "@/features/supplier-payables/types"

const LIST_PAGE_SIZE = 100

// ─── Backend DTOs ──────────────────────────────────────────────────────────

type BackendPayableEntry = {
    id: string
    entry_type: string
    direction: "increase" | "decrease"
    amount: string
    due_date: string
    source_document_id: string
    source_sequence: number
    posted_at: number
}

type BackendPayableAccount = {
    id: string
    source_document_id: string
    supplier_id: string
    source_type: string
    gross_total: string
    settled_total: string
    open_total: string
    invoiceable_total: string
    invoiced_total: string
    open_invoiceable_total: string
    status: string
    version: number
    created_at: number
    entries: BackendPayableEntry[]
}

type BackendPaymentAllocation = {
    id: string
    allocation_seq: number
    allocation_action: "apply" | "reverse"
    payable_entry_id: string
    allocated_amount: string
    allocated_at: number
    reverses_allocation_id?: string | null
}

type BackendSupplierPayment = {
    id: string
    payment_no: string
    status: string
    supplier_id: string
    paid_at: number
    amount: string
    bank_reference?: string | null
    version: number
    created_at: number
    allocated_total: string
    unallocated_amount: string
    allocations: BackendPaymentAllocation[]
}

type BackendInvoice = {
    id: string
    invoice_direction: string
    invoice_kind: "blue" | "red"
    party_id: string
    invoice_code?: string | null
    invoice_no: string
    invoice_date: string
    gross_amount: string
    net_amount: string
    tax_amount: string
    original_invoice_id?: string | null
    status: string
    version: number
    allocated_total: string
    unallocated_amount: string
    allocations: Array<{
        id: string
        allocation_seq: number
        allocation_action: "apply" | "reverse"
        payable_account_id?: string
        receivable_account_id?: string
        allocated_gross_amount: string
        reverses_allocation_id?: string | null
    }>
}

type BackendPurchaseInvoiceAllocation = {
    id: string
    invoice_id: string
    allocation_seq: number
    allocation_action: "apply" | "reverse"
    payable_account_id: string
    allocated_gross_amount: string
    allocated_net_amount: string
    allocated_tax_amount: string
    reverses_allocation_id?: string | null
}

// ─── Mapping helpers ───────────────────────────────────────────────────────

function instantToIso(secs: number | undefined | null): string {
    if (secs == null || !Number.isFinite(secs)) return ""
    return new Date(secs * 1000).toISOString()
}

function maskBank(raw?: string | null): string {
    const v = (raw ?? "").trim()
    if (!v) return "****"
    if (v.includes("*")) return v
    if (v.length <= 4) return `****${v}`
    return `****${v.slice(-4)}`
}

function mapPayableStatus(s: string): PayableRow["status"] {
    if (s === "settled") return "SETTLED"
    if (s === "partially_settled" || s === "partial") return "PARTIAL"
    return "OPEN"
}

function mapSourceType(s: string): PayableRow["sourceType"] {
    if (s === "supplier_settlement" || s === "SUPPLIER_SETTLEMENT") {
        return "SUPPLIER_SETTLEMENT"
    }
    return "PURCHASE_ORDER"
}

function mapPaymentStatus(s: string): PaymentRow["status"] {
    if (s === "reversed") return "REVERSED"
    if (s === "posted") return "POSTED"
    return "DRAFT"
}

function mapInvoiceStatus(s: string): PurchaseInvoiceRow["status"] {
    if (s === "reversed" || s === "red_invoiced") return "REVERSED"
    return "POSTED"
}

function mapBackendPayableStatus(status?: string): string | undefined {
    if (!status) return undefined
    const u = status.toUpperCase()
    if (u === "OPEN") return "open"
    if (u === "PARTIAL" || u === "PARTIALLY_SETTLED") return "partially_settled"
    if (u === "SETTLED") return "settled"
    return status.toLowerCase()
}

function mapBackendSourceType(t?: string): string | undefined {
    if (!t) return undefined
    if (t === "PURCHASE_ORDER" || t === "purchase_order")
        return "purchase_order"
    if (t === "SUPPLIER_SETTLEMENT" || t === "supplier_settlement") {
        return "supplier_settlement"
    }
    return t
}

function projectPayable(a: BackendPayableAccount): PayableRow {
    const status = mapPayableStatus(a.status)
    const sourceType = mapSourceType(a.source_type)
    const primary = (a.entries ?? []).find((e) => e.direction === "increase")
    return {
        payableAccountId: a.id,
        supplierId: a.supplier_id,
        supplierName: a.supplier_id,
        sourceType,
        sourceTypeLabel: SOURCE_TYPE_LABEL[sourceType],
        sourceDocumentId: a.source_document_id,
        sourceDocumentNo: a.source_document_id,
        primaryEntryId: primary?.id ?? a.entries?.[0]?.id ?? a.id,
        entryLockVersion: a.version,
        accountLockVersion: a.version,
        grossTotal: a.gross_total,
        settledTotal: a.settled_total,
        openTotal: a.open_total,
        invoicedTotal: a.invoiced_total,
        openInvoiceableTotal: a.open_invoiceable_total,
        dueDate: primary?.due_date ?? a.entries?.[0]?.due_date ?? "",
        dueState: "not_due",
        dueStateLabel: "未到期",
        status,
        statusLabel: PAYABLE_STATUS_LABEL[status],
        statusTone: PAYABLE_STATUS_TONE[status],
        allowedActions: ["VIEW_DETAIL", "REGISTER_PAYMENT", "REGISTER_INVOICE"],
        actionBlockers: [],
    }
}

function projectPayment(p: BackendSupplierPayment): PaymentRow {
    const status = mapPaymentStatus(p.status)
    return {
        paymentId: p.id,
        paymentNo: p.payment_no,
        supplierId: p.supplier_id,
        supplierName: p.supplier_id,
        paidAt: instantToIso(p.paid_at),
        amount: p.amount,
        bankReferenceMasked: maskBank(p.bank_reference),
        allocatedTotal: p.allocated_total,
        unallocatedAmount: p.unallocated_amount,
        status,
        statusLabel:
            p.status === "pending_review"
                ? "待复核"
                : status === "DRAFT"
                  ? "草稿"
                  : status === "REVERSED"
                    ? "已冲正"
                    : "已确认",
        statusTone:
            status === "REVERSED"
                ? "destructive"
                : status === "POSTED"
                  ? "success"
                  : "neutral",
        allocations: (p.allocations ?? []).map((a) => ({
            allocationId: a.id,
            action: a.allocation_action === "reverse" ? "REVERSE" : "APPLY",
            payableAccountId: "",
            payableEntryId: a.payable_entry_id,
            sourceType: "PURCHASE_ORDER" as const,
            sourceDocumentNo: a.payable_entry_id,
            amount: a.allocated_amount,
            occurredAt: instantToIso(a.allocated_at),
            reverseOfAllocationId: a.reverses_allocation_id ?? undefined,
        })),
        allowedActions:
            status === "POSTED"
                ? ["VIEW_DETAIL", "CONTINUE_ALLOCATE", "REVERSE_PAYMENT"]
                : ["VIEW_DETAIL"],
        actionBlockers: [],
    }
}

function projectInvoice(inv: BackendInvoice): PurchaseInvoiceRow {
    const status = mapInvoiceStatus(inv.status)
    return {
        invoiceId: inv.id,
        invoiceCode: inv.invoice_code ?? "",
        invoiceNo: inv.invoice_no,
        invoiceKind: inv.invoice_kind === "red" ? "RED" : "BLUE",
        invoiceKindLabel: inv.invoice_kind === "red" ? "红票" : "蓝票",
        supplierId: inv.party_id,
        supplierName: inv.party_id,
        invoiceDate: inv.invoice_date,
        grossAmount: inv.gross_amount,
        netAmount: inv.net_amount,
        taxAmount: inv.tax_amount,
        allocatedTotal: inv.allocated_total,
        unallocatedAmount: inv.unallocated_amount,
        status,
        statusLabel: status === "REVERSED" ? "已红冲" : "已登记",
        statusTone: inv.invoice_kind === "red" ? "warning" : "success",
        originalInvoiceId: inv.original_invoice_id ?? undefined,
        allocations: (inv.allocations ?? []).map((a) => ({
            allocationId: a.id,
            action: a.allocation_action === "reverse" ? "REVERSE" : "APPLY",
            payableAccountId:
                a.payable_account_id ?? a.receivable_account_id ?? "",
            sourceType: "PURCHASE_ORDER" as const,
            sourceDocumentNo: a.payable_account_id ?? "",
            amountGross: a.allocated_gross_amount,
            occurredAt: "",
            reverseOfAllocationId: a.reverses_allocation_id ?? undefined,
        })),
        allowedActions:
            status === "POSTED" && inv.invoice_kind === "blue"
                ? ["VIEW_DETAIL", "CONTINUE_ALLOCATE", "ISSUE_RED"]
                : ["VIEW_DETAIL"],
        actionBlockers: [],
    }
}

function filterSummary(query: SupplierAccountsQuery): string {
    const parts = [VIEW_LABEL[query.view]]
    if (query.supplierId) parts.push(query.supplierId)
    if (query.sourceType) parts.push(SOURCE_TYPE_LABEL[query.sourceType])
    if (query.q?.trim()) parts.push(`「${query.q.trim()}」`)
    return parts.join(" · ")
}

// ─── Draft sessions (UI-only; pool from HTTP) ──────────────────────────────

const sessions = new Map<string, AllocationSessionView>()
const draftSnapshots = new Map<string, Record<string, unknown>>()
let sessionSeq = 200
const submitIdempotency = new Map<string, FormalSubmitResult>()

// ─── Public API ────────────────────────────────────────────────────────────

export async function fetchSupplierAccounts(
    query: SupplierAccountsQuery,
): Promise<SupplierAccountsListView> {
    const [payPage, paymentPage, invPage] = await Promise.all([
        apiGet<Page<BackendPayableAccount>>("/admin/payable-accounts", {
            supplier_id: query.supplierId,
            source_type: mapBackendSourceType(query.sourceType),
            status: mapBackendPayableStatus(query.status),
            page: 1,
            page_size: LIST_PAGE_SIZE,
            sort_by: "created_at",
            sort_dir: "desc",
        }),
        apiGet<Page<BackendSupplierPayment>>("/admin/supplier-payments", {
            supplier_id: query.supplierId,
            payment_no: query.q?.trim() || undefined,
            page: 1,
            page_size: LIST_PAGE_SIZE,
            sort_by: "paid_at",
            sort_dir: "desc",
        }),
        apiGet<Page<BackendInvoice>>("/admin/invoices", {
            invoice_direction: "purchase",
            party_id: query.supplierId,
            invoice_no: query.q?.trim() || undefined,
            page: 1,
            page_size: LIST_PAGE_SIZE,
            sort_by: "invoice_date",
            sort_dir: "desc",
        }),
    ])

    let payables = (payPage.items ?? []).map(projectPayable)
    const payments = (paymentPage.items ?? []).map(projectPayment)
    const invoices = (invPage.items ?? [])
        .filter(
            (i) => i.invoice_direction === "purchase" || !i.invoice_direction,
        )
        .map(projectInvoice)

    if (query.purchaseOrderId) {
        payables = payables.filter(
            (p) =>
                p.sourceDocumentId === query.purchaseOrderId ||
                p.sourceDocumentNo === query.purchaseOrderId,
        )
    }

    const unallocated: UnallocatedRow[] = [
        ...payments
            .filter(
                (p) =>
                    p.status === "POSTED" &&
                    p.unallocatedAmount &&
                    p.unallocatedAmount !== "0" &&
                    p.unallocatedAmount !== "0.00",
            )
            .map((p) => ({
                id: p.paymentId,
                track: "payment" as const,
                trackLabel: "付款",
                documentNo: p.paymentNo,
                supplierId: p.supplierId,
                supplierName: p.supplierName,
                amount: p.amount,
                unallocatedAmount: p.unallocatedAmount,
                occurredAt: p.paidAt,
                statusLabel: p.statusLabel,
                statusTone: p.statusTone,
            })),
        ...invoices
            .filter(
                (i) =>
                    i.invoiceKind === "BLUE" &&
                    i.unallocatedAmount &&
                    i.unallocatedAmount !== "0" &&
                    i.unallocatedAmount !== "0.00",
            )
            .map((i) => ({
                id: i.invoiceId,
                track: "purchase_invoice" as const,
                trackLabel: "进项发票",
                documentNo: i.invoiceNo,
                supplierId: i.supplierId,
                supplierName: i.supplierName,
                amount: i.grossAmount,
                unallocatedAmount: i.unallocatedAmount,
                occurredAt: i.invoiceDate,
                statusLabel: i.statusLabel,
                statusTone: i.statusTone,
            })),
    ]

    let total = 0
    if (query.view === "payable") total = payPage.total ?? payables.length
    else if (query.view === "payment")
        total = paymentPage.total ?? payments.length
    else if (query.view === "purchase_invoice")
        total = invPage.total ?? invoices.length
    else total = unallocated.length

    const supplierMap = new Map<
        string,
        {
            supplierId: string
            supplierName: string
            openPayableTotal: string
            unallocatedPaymentTotal: string
            unallocatedInvoiceTotal: string
        }
    >()
    for (const p of payables) {
        if (!supplierMap.has(p.supplierId)) {
            supplierMap.set(p.supplierId, {
                supplierId: p.supplierId,
                supplierName: p.supplierName,
                openPayableTotal: "0.00",
                unallocatedPaymentTotal: "0.00",
                unallocatedInvoiceTotal: "0.00",
            })
        }
    }

    const hasFilters = Boolean(
        query.q?.trim() ||
        query.supplierId ||
        query.sourceType ||
        query.status ||
        query.due ||
        query.purchaseOrderId,
    )

    return {
        view: query.view,
        // metrics 后端无汇总端点 — 占位 0
        metrics: {
            openPayableTotal: "0.00",
            overduePayableTotal: "0.00",
            unallocatedPaymentTotal: "0.00",
            unallocatedInvoiceTotal: "0.00",
            prepayGateBlockedCount: 0,
        },
        payables,
        payments,
        invoices,
        unallocated,
        suppliers: [...supplierMap.values()],
        total,
        filterSummary: filterSummary(query),
        permissionVersion: "pv-w12-http-1",
        dataWatermark: `wm-w12-${payPage.total ?? 0}-${paymentPage.total ?? 0}`,
        queriedAt: new Date().toISOString(),
        moduleAllowed: true,
        hasDataScope: true,
        canRegisterPayment: true,
        canRegisterInvoice: true,
        canExport: true,
        emptyReason:
            total === 0
                ? hasFilters
                    ? "FILTER_NO_RESULT"
                    : "NO_DATA"
                : undefined,
        // 应付优先级策略后端缺口
        payablePriorityPolicy: {
            state: "MISSING",
            mixedAutoAllocationAllowed: false,
            blockerMessage:
                "应付优先级策略接口尚未提供，请显式逐项选择分配目标。",
        },
        allowFullBankReveal: false,
    }
}

export async function fetchPayableDetail(
    payableAccountId: string,
): Promise<PayableDetailView | null> {
    try {
        const a = await apiGet<BackendPayableAccount>(
            `/admin/payable-accounts/${encodeURIComponent(payableAccountId)}`,
        )
        const payable = projectPayable(a)
        let invoiceAllocs: PayableDetailView["invoiceAllocations"] = []
        try {
            const allocPage = await apiGet<
                Page<BackendPurchaseInvoiceAllocation>
            >("/admin/purchase-invoice-allocations", {
                payable_account_id: payableAccountId,
                page: 1,
                page_size: LIST_PAGE_SIZE,
            })
            invoiceAllocs = (allocPage.items ?? []).map((row) => ({
                allocationId: row.id,
                action:
                    row.allocation_action === "reverse" ? "REVERSE" : "APPLY",
                payableAccountId: row.payable_account_id,
                sourceType: payable.sourceType,
                sourceDocumentNo: payable.sourceDocumentNo,
                amountGross: row.allocated_gross_amount,
                occurredAt: "",
                reverseOfAllocationId: row.reverses_allocation_id ?? undefined,
            }))
        } catch {
            invoiceAllocs = []
        }

        return {
            payable,
            entries: (a.entries ?? []).map((e) => ({
                entryId: e.id,
                entryTypeLabel: e.entry_type,
                direction: e.direction,
                amount: e.amount,
                sourceLabel: e.source_document_id,
                dueDate: e.due_date,
                occurredAt: instantToIso(e.posted_at),
            })),
            paymentAllocations: [],
            invoiceAllocations: invoiceAllocs,
            dataWatermark: `wm-pa-${a.version}`,
            queriedAt: new Date().toISOString(),
        }
    } catch {
        return null
    }
}

export async function fetchAllocationSession(input: {
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
    if (input.draftSessionId && sessions.has(input.draftSessionId)) {
        return sessions.get(input.draftSessionId)!
    }

    const payPage = await apiGet<Page<BackendPayableAccount>>(
        "/admin/payable-accounts",
        {
            supplier_id: input.supplierId,
            page: 1,
            page_size: LIST_PAGE_SIZE,
            sort_by: "created_at",
            sort_dir: "desc",
        },
    )
    const pool = (payPage.items ?? []).map((a) => {
        const primary = (a.entries ?? []).find(
            (e) => e.direction === "increase",
        )
        const sourceType = mapSourceType(a.source_type)
        return {
            payableAccountId: a.id,
            primaryEntryId: primary?.id ?? a.entries?.[0]?.id ?? a.id,
            entryLockVersion: a.version,
            accountLockVersion: a.version,
            sourceType,
            sourceTypeLabel: SOURCE_TYPE_LABEL[sourceType],
            sourceDocumentNo: a.source_document_id,
            sourceDocumentId: a.source_document_id,
            openTotal: a.open_total,
            openInvoiceableTotal: a.open_invoiceable_total,
            dueDate: primary?.due_date ?? "",
            dueStateLabel: "未到期",
            statusLabel: PAYABLE_STATUS_LABEL[mapPayableStatus(a.status)],
        }
    })

    let existingAmount: string | undefined
    let existingUnallocated: string | undefined
    let existingDocumentNo: string | undefined

    if (input.existingPaymentId) {
        const p = await apiGet<BackendSupplierPayment>(
            `/admin/supplier-payments/${encodeURIComponent(input.existingPaymentId)}`,
        )
        existingAmount = p.amount
        existingUnallocated = p.unallocated_amount
        existingDocumentNo = p.payment_no
    } else if (input.existingInvoiceId) {
        const inv = await apiGet<BackendInvoice>(
            `/admin/invoices/${encodeURIComponent(input.existingInvoiceId)}`,
        )
        existingAmount = inv.gross_amount
        existingUnallocated = inv.unallocated_amount
        existingDocumentNo = inv.invoice_no
    }

    const draftSessionId = input.draftSessionId ?? `alloc_sup_${++sessionSeq}`
    const view: AllocationSessionView = {
        draftSessionId,
        track: input.track,
        supplierId: input.supplierId,
        supplierName: input.supplierId,
        pool,
        payablePriorityPolicy: {
            state: "MISSING",
            mixedAutoAllocationAllowed: false,
            blockerMessage:
                "应付优先级策略接口尚未提供，请显式逐项选择分配目标。",
        },
        preselectedPayableAccountIds: input.preselectPayableAccountId
            ? [input.preselectPayableAccountId]
            : [],
        purchaseOrderId: input.purchaseOrderId,
        returnTo: input.returnTo,
        fromWorkspace: input.fromWorkspace,
        dataWatermark: `wm-sess-${pool.length}`,
        queriedAt: new Date().toISOString(),
        existingPaymentId: input.existingPaymentId,
        existingInvoiceId: input.existingInvoiceId,
        existingAmount,
        existingUnallocated,
        existingDocumentNo,
    }
    sessions.set(draftSessionId, view)
    return view
}

export async function saveAllocationDraft(
    input: SaveAllocationDraftInput,
): Promise<{ savedAt: string }> {
    draftSnapshots.set(input.draftSessionId, input.formSnapshot)
    const savedAt = new Date().toISOString()
    const s = sessions.get(input.draftSessionId)
    if (s) {
        sessions.set(input.draftSessionId, { ...s, draftSavedAt: savedAt })
    }
    return { savedAt }
}

export async function submitPayment(
    input: PostPaymentInput,
): Promise<FormalSubmitResult> {
    const cached = submitIdempotency.get(input.idempotencyKey)
    if (cached) return cached

    try {
        let paymentId = input.existingPaymentId
        let paymentNo = ""

        if (!paymentId) {
            const paidAtSecs = input.paidAt
                ? Math.floor(new Date(input.paidAt).getTime() / 1000)
                : Math.floor(Date.now() / 1000)
            const created = await apiPost<BackendSupplierPayment>(
                "/admin/supplier-payments",
                {
                    payment_no: `FK-${input.idempotencyKey.slice(-8)}`,
                    supplier_id: input.supplierId,
                    paid_at: paidAtSecs,
                    amount: input.amount,
                    bank_reference: input.bankReference || undefined,
                },
            )
            paymentId = created.id
            paymentNo = created.payment_no
        }

        const targets = input.targets.filter(
            (t) => t.amount && Number(t.amount) > 0,
        )
        if (targets.length > 0) {
            const posted = await apiPost<BackendSupplierPayment>(
                `/admin/supplier-payments/${encodeURIComponent(paymentId)}/post`,
                {
                    allocations: targets.map((t) => ({
                        payable_entry_id:
                            t.payableEntryId ?? t.payableAccountId,
                        allocated_amount: t.amount,
                    })),
                },
            )
            paymentNo = posted.payment_no
            const result: FormalSubmitResult = {
                status: "succeeded",
                title: "付款已确认",
                description: "付款与核销已提交。",
                reference: posted.payment_no,
                operationId: input.idempotencyKey,
                documentNo: posted.payment_no,
                unallocatedAmount: posted.unallocated_amount,
                allocatedTotal: posted.allocated_total,
                returnTo: sessions.get(input.draftSessionId)?.returnTo,
            }
            submitIdempotency.set(input.idempotencyKey, result)
            return result
        }

        const result: FormalSubmitResult = {
            status: "succeeded",
            title: "付款草稿已创建",
            description: "未分配核销行；付款草稿已登记。",
            reference: paymentNo || paymentId,
            operationId: input.idempotencyKey,
            documentNo: paymentNo || paymentId,
            unallocatedAmount: input.amount,
            allocatedTotal: "0.00",
        }
        submitIdempotency.set(input.idempotencyKey, result)
        return result
    } catch (err) {
        const message =
            err && typeof err === "object" && "message" in err
                ? String((err as { message: unknown }).message)
                : "付款提交失败"
        return {
            status: "failed",
            title: "付款失败",
            description: message,
            errorCode: "HTTP_ERROR",
        }
    }
}

export async function submitInvoice(
    input: PostInvoiceInput,
): Promise<FormalSubmitResult> {
    const cached = submitIdempotency.get(input.idempotencyKey)
    if (cached) return cached

    try {
        const targets = input.targets.filter(
            (t) => t.amount && Number(t.amount) > 0,
        )
        if (input.existingInvoiceId) {
            // Continue allocate: purchase-invoice-allocations is create+post only;
            // additional allocation on existing invoice is a backend gap if no partial post.
            return {
                status: "failed",
                title: "继续核销不可用",
                description:
                    "进项发票继续分配端点未提供（仅支持登记过账一次）。请使用新票或联系后端补齐。",
                errorCode: "BACKEND_GAP",
                existingDocumentId: input.existingInvoiceId,
            }
        }

        if (targets.length === 0) {
            return {
                status: "failed",
                title: "缺少分配",
                description: "进项发票登记要求至少一条分配行。",
                errorCode: "ALLOCATION_REQUIRED",
            }
        }

        const registered = await apiPost<{
            invoice_id: string
            invoice_no: string
            gross_amount: string
            allocations: BackendPurchaseInvoiceAllocation[]
        }>("/admin/purchase-invoice-allocations", {
            invoice_code: input.invoiceCode || undefined,
            invoice_no: input.invoiceNo,
            invoice_date: input.invoiceDate,
            gross_amount: input.grossAmount,
            net_amount: input.netAmount,
            tax_amount: input.taxAmount,
            supplier_id: input.supplierId,
            allocations: targets.map((t) => ({
                payable_account_id: t.payableAccountId,
                allocated_gross_amount: t.amount,
                allocated_net_amount: input.netAmount,
                allocated_tax_amount: input.taxAmount,
            })),
        })

        const result: FormalSubmitResult = {
            status: "succeeded",
            title: "进项发票已登记",
            description: "进项发票与分配已过账。",
            reference: registered.invoice_no,
            operationId: input.idempotencyKey,
            documentNo: registered.invoice_no,
            allocatedTotal: registered.gross_amount,
            unallocatedAmount: "0.00",
            returnTo: sessions.get(input.draftSessionId)?.returnTo,
        }
        submitIdempotency.set(input.idempotencyKey, result)
        return result
    } catch (err) {
        const message =
            err && typeof err === "object" && "message" in err
                ? String((err as { message: unknown }).message)
                : "进项发票提交失败"
        return {
            status: "failed",
            title: "进项发票失败",
            description: message,
            errorCode: "HTTP_ERROR",
        }
    }
}

export async function reversePayment(
    input: ReversePaymentInput,
): Promise<FormalSubmitResult> {
    const cached = submitIdempotency.get(input.idempotencyKey)
    if (cached) return cached
    try {
        const payment = await apiGet<BackendSupplierPayment>(
            `/admin/supplier-payments/${encodeURIComponent(input.paymentId)}`,
        )
        const nowSecs = Math.floor(Date.now() / 1000)
        const created = await apiPost<{ id: string; reversal_no: string }>(
            "/admin/payment-reversals",
            {
                reversal_no: `PCZ-${input.idempotencyKey.slice(-8)}`,
                original_supplier_payment_id: input.paymentId,
                reason_text: input.reason,
                amount: payment.amount,
                handled_by: "finance_handler",
                reviewed_by: "finance_reviewer",
                occurred_at: nowSecs,
            },
        )
        const posted = await apiPost<{ id: string; reversal_no: string }>(
            `/admin/payment-reversals/${encodeURIComponent(created.id)}/post`,
            {},
        )
        const result: FormalSubmitResult = {
            status: "succeeded",
            title: "付款冲正已完成",
            description: "已追加付款冲正记录，原付款保留。",
            reference: posted.reversal_no,
            operationId: input.idempotencyKey,
            documentNo: posted.reversal_no,
        }
        submitIdempotency.set(input.idempotencyKey, result)
        return result
    } catch (err) {
        const message =
            err && typeof err === "object" && "message" in err
                ? String((err as { message: unknown }).message)
                : "付款冲正失败"
        return {
            status: "failed",
            title: "冲正失败",
            description: message,
            errorCode: "HTTP_ERROR",
        }
    }
}

export async function reverseInvoice(
    input: ReverseInvoiceInput,
): Promise<FormalSubmitResult> {
    const cached = submitIdempotency.get(input.idempotencyKey)
    if (cached) return cached
    try {
        // Purchase red-issue via invoices red-issue (D18) when blue purchase invoice
        const inv = await apiGet<BackendInvoice>(
            `/admin/invoices/${encodeURIComponent(input.invoiceId)}`,
        )
        const applyLines = (inv.allocations ?? []).filter(
            (a) => a.allocation_action === "apply",
        )
        if (applyLines.length === 0) {
            return {
                status: "failed",
                title: "无法红冲",
                description: "原票无有效分配。",
                errorCode: "NO_ALLOCATIONS",
            }
        }
        const red = await apiPost<BackendInvoice>(
            `/admin/invoices/${encodeURIComponent(input.invoiceId)}/red-issue`,
            {
                invoice_no: input.redInvoiceNo,
                invoice_date: new Date().toISOString().slice(0, 10),
                gross_amount: inv.gross_amount,
                net_amount: inv.net_amount,
                tax_amount: inv.tax_amount,
                allocations: applyLines.map((a) => ({
                    reverses_allocation_id: a.id,
                    allocated_gross_amount: a.allocated_gross_amount,
                    allocated_net_amount: a.allocated_gross_amount,
                    allocated_tax_amount: "0",
                })),
            },
        )
        const result: FormalSubmitResult = {
            status: "succeeded",
            title: "红票已登记",
            description: "已登记红票并反向分配，原蓝票保留。",
            reference: red.invoice_no,
            operationId: input.idempotencyKey,
            documentNo: red.invoice_no,
        }
        submitIdempotency.set(input.idempotencyKey, result)
        return result
    } catch (err) {
        const message =
            err && typeof err === "object" && "message" in err
                ? String((err as { message: unknown }).message)
                : "红票失败"
        return {
            status: "failed",
            title: "红票失败",
            description: message,
            errorCode: "HTTP_ERROR",
        }
    }
}

export async function resolveUnknownResult(
    idempotencyKey: string,
): Promise<FormalSubmitResult | null> {
    return submitIdempotency.get(idempotencyKey) ?? null
}
