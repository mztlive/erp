/**
 * W12 供应商往来 · 应付台账相关请求（列表、详情、核销会话）。
 * 会话共享状态见 api/shared；DTO 映射见 api/mappers。
 */

import { apiGet } from "@/lib/api"
import type { Page } from "@/lib/api"
import {
    filterSummary,
    instantToIso,
    mapBackendPayableStatus,
    mapBackendSourceType,
    mapPayableStatus,
    mapSourceType,
    projectInvoice,
    projectPayable,
    projectPayment,
} from "@/features/supplier-payables/api/mappers"
import type {
    BackendInvoice,
    BackendPayableAccount,
    BackendPurchaseInvoiceAllocation,
    BackendSupplierPayment,
} from "@/features/supplier-payables/api/mappers"
import {
    LIST_PAGE_SIZE,
    nextSessionId,
    sessions,
} from "@/features/supplier-payables/api/shared"
import type {
    AllocationSessionView,
    AllocationTrack,
    PayableDetailView,
    PaymentRow,
    SupplierAccountsListView,
    SupplierAccountsQuery,
    UnallocatedRow,
} from "@/features/supplier-payables/types"
import {
    PAYABLE_STATUS_LABEL,
    SOURCE_TYPE_LABEL,
} from "@/features/supplier-payables/types"
import { fetchSupplierOption } from "@/features/entity-selectors/api/suppliers"

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

/**
 * 读取供应商付款详情，含只读审批投影。缺失时返回 null。
 *
 * @param paymentId 付款主键。
 */
export async function fetchSupplierPayment(
    paymentId: string,
): Promise<PaymentRow | null> {
    try {
        const payment = await apiGet<BackendSupplierPayment>(
            `/admin/supplier-payments/${encodeURIComponent(paymentId)}`,
        )
        return projectPayment(payment)
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
            sourceDocumentNo: a.source_document_no || a.source_document_id,
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
    let existingPaymentVersion: number | undefined
    let approval: AllocationSessionView["approval"]

    if (input.existingPaymentId) {
        const p = await apiGet<BackendSupplierPayment>(
            `/admin/supplier-payments/${encodeURIComponent(input.existingPaymentId)}`,
        )
        existingAmount = p.amount
        existingUnallocated = p.unallocated_amount
        existingDocumentNo = p.payment_no
        existingPaymentVersion = p.version
        approval = projectPayment(p).approval
    } else if (input.existingInvoiceId) {
        const inv = await apiGet<BackendInvoice>(
            `/admin/invoices/${encodeURIComponent(input.existingInvoiceId)}`,
        )
        existingAmount = inv.gross_amount
        existingUnallocated = inv.unallocated_amount
        existingDocumentNo = inv.invoice_no
    }

    const draftSessionId = input.draftSessionId ?? nextSessionId()
    // 会话内供应商名以主数据实时解析（缺失时回退供应商 ID，不阻断核销）
    const supplierOption = await fetchSupplierOption(input.supplierId).catch(
        () => null,
    )
    const view: AllocationSessionView = {
        draftSessionId,
        track: input.track,
        supplierId: input.supplierId,
        supplierName: supplierOption?.supplierName ?? input.supplierId,
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
        existingPaymentVersion,
        approval,
    }
    sessions.set(draftSessionId, view)
    return view
}
