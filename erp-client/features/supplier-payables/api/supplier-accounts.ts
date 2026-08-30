/**
 * W12 供应商往来 · 应付台账相关请求（列表、详情、核销会话）。
 * 会话共享状态见 api/shared；DTO 映射见 api/mappers。
 */

import { apiGet, apiPost } from "@/lib/api"
import type { Page } from "@/lib/api"
import {
    filterSummary,
    instantToIso,
    mapBackendPayableStatus,
    mapBackendSourceType,
    mapPayableStatus,
    mapSourceType,
    payableEntryTypeLabel,
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
} from "@/features/supplier-payables/api/shared"
import type {
    AllocationSessionView,
    AllocationTrack,
    PayableDetailView,
    PayableRow,
    PaymentRow,
    PurchaseInvoiceRow,
    SupplierAccountsListView,
    SupplierAccountsQuery,
    UnallocatedRow,
} from "@/features/supplier-payables/types"
import {
    PAYABLE_STATUS_LABEL,
    SOURCE_TYPE_LABEL,
} from "@/features/supplier-payables/types"
import { fetchPartyOption } from "@/features/entity-selectors/api/parties"
import { fetchSupplierOption } from "@/features/entity-selectors/api/suppliers"
import {
    businessLabelOrPlaceholder,
    MISSING_SUPPLIER_NAME,
} from "@/features/supplier-payables/lib/display-labels"
import {
    missingSourceDocumentNo,
    payablePreviewHref,
} from "@/features/supplier-payables/lib/related-documents"

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
    let payments = (paymentPage.items ?? []).map(projectPayment)
    let invoices = (invPage.items ?? [])
        .filter(
            (i) => i.invoice_direction === "purchase" || !i.invoice_direction,
        )
        .map(projectInvoice)
    payments = await hydratePaymentSupplierNames(payments)
    invoices = await hydrateInvoicePartyNames(invoices)
    invoices = attachPayableSourceToInvoices(invoices, payables)

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
            blockerMessage: "应付优先级策略尚未配置，请显式逐项选择分配目标。",
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
                entryTypeLabel: payableEntryTypeLabel(e.entry_type),
                direction: e.direction,
                amount: e.amount,
                sourceLabel: businessLabelOrPlaceholder(
                    e.source_document_no,
                    e.source_document_id,
                    missingSourceDocumentNo(payable.sourceType),
                ),
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

/** 付款任务内揭示收款账号所需的冻结身份。 */
export type RevealPaymentRecipientInput = Readonly<{
    payableAccountId: string
    workItemId: string
    expectedTaskVersion: string
    expectedBankAccountId: string
    expectedBankAccountVersion: number
}>

/**
 * 在当前付款任务责任校验后读取完整收款账号。
 *
 * 后端同时校验任务版本、当前负责人和页面所见账户身份，并记录敏感数据审计。
 */
export async function revealPaymentRecipient(
    input: RevealPaymentRecipientInput,
): Promise<string> {
    const result = await apiPost<{
        bank_account_id: string
        account_number: string
    }>(
        `/admin/payable-accounts/${encodeURIComponent(input.payableAccountId)}/payment-recipient/reveal`,
        {
            work_item_id: input.workItemId,
            expected_task_version: input.expectedTaskVersion,
            expected_bank_account_id: input.expectedBankAccountId,
            expected_bank_account_version: input.expectedBankAccountVersion,
        },
        { cache: "no-store" },
    )
    if (result.bank_account_id !== input.expectedBankAccountId) {
        throw new Error("收款账户已变化，请刷新付款任务后重新核对")
    }
    return result.account_number
}

/**
 * 读取供应商付款详情。缺失时返回 null。
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
    existingInvoiceId?: string
    preselectPayableAccountId?: string
}): Promise<AllocationSessionView> {
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
            sourceDocumentNo: businessLabelOrPlaceholder(
                a.source_document_no,
                a.source_document_id,
                missingSourceDocumentNo(sourceType),
            ),
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
    if (input.existingInvoiceId) {
        const inv = await apiGet<BackendInvoice>(
            `/admin/invoices/${encodeURIComponent(input.existingInvoiceId)}`,
        )
        existingAmount = inv.gross_amount
        existingUnallocated = inv.unallocated_amount
        existingDocumentNo = inv.invoice_no
    }

    const draftSessionId = input.draftSessionId ?? nextSessionId()
    // 会话内供应商名以主数据实时解析；缺失时用业务占位，不上屏供应商 ID。
    const supplierOption = await fetchSupplierOption(input.supplierId).catch(
        () => null,
    )
    const view: AllocationSessionView = {
        draftSessionId,
        track: input.track,
        supplierId: input.supplierId,
        supplierName: businessLabelOrPlaceholder(
            supplierOption?.supplierName,
            input.supplierId,
            MISSING_SUPPLIER_NAME,
        ),
        pool,
        payablePriorityPolicy: {
            state: "MISSING",
            mixedAutoAllocationAllowed: false,
            blockerMessage: "应付优先级策略尚未配置，请显式逐项选择分配目标。",
        },
        preselectedPayableAccountIds: input.preselectPayableAccountId
            ? [input.preselectPayableAccountId]
            : [],
        purchaseOrderId: input.purchaseOrderId,
        returnTo: input.returnTo,
        fromWorkspace: input.fromWorkspace,
        dataWatermark: `wm-sess-${pool.length}`,
        queriedAt: new Date().toISOString(),
        existingInvoiceId: input.existingInvoiceId,
        existingAmount,
        existingUnallocated,
        existingDocumentNo,
    }
    return view
}

/**
 * 付款列表若仍缺供应商名称，则按供应商主数据补全；失败时保留业务占位。
 *
 * @param payments 已投影的付款行。
 */
async function hydratePaymentSupplierNames(
    payments: PaymentRow[],
): Promise<PaymentRow[]> {
    const missingIds = [
        ...new Set(
            payments
                .filter((row) => row.supplierName === MISSING_SUPPLIER_NAME)
                .map((row) => row.supplierId)
                .filter(Boolean),
        ),
    ]
    if (missingIds.length === 0) return payments
    const resolved = new Map<string, string>()
    await Promise.all(
        missingIds.map(async (supplierId) => {
            const option = await fetchSupplierOption(supplierId)
            const name = businessLabelOrPlaceholder(
                option?.supplierName,
                supplierId,
                MISSING_SUPPLIER_NAME,
            )
            if (name !== MISSING_SUPPLIER_NAME) {
                resolved.set(supplierId, name)
            }
        }),
    )
    if (resolved.size === 0) return payments
    return payments.map((row) => {
        const supplierName = resolved.get(row.supplierId)
        return supplierName ? { ...row, supplierName } : row
    })
}

/**
 * 进项发票往来主体按主体主数据补全名称；失败时保留业务占位。
 *
 * @param invoices 已投影的进项发票行。
 */
async function hydrateInvoicePartyNames(
    invoices: PurchaseInvoiceRow[],
): Promise<PurchaseInvoiceRow[]> {
    const missingIds = [
        ...new Set(
            invoices
                .filter((row) => row.supplierName === MISSING_SUPPLIER_NAME)
                .map((row) => row.supplierId)
                .filter(Boolean),
        ),
    ]
    if (missingIds.length === 0) return invoices
    const resolved = new Map<string, string>()
    await Promise.all(
        missingIds.map(async (partyId) => {
            const option = await fetchPartyOption(partyId)
            const name = businessLabelOrPlaceholder(
                option?.displayName,
                partyId,
                MISSING_SUPPLIER_NAME,
            )
            if (name !== MISSING_SUPPLIER_NAME) {
                resolved.set(partyId, name)
            }
        }),
    )
    if (resolved.size === 0) return invoices
    return invoices.map((row) => {
        const supplierName = resolved.get(row.supplierId)
        return supplierName ? { ...row, supplierName } : row
    })
}

/**
 * 用本页已加载的应付台账补全进项票核销来源单号与跳转。
 *
 * @param invoices 进项发票行。
 * @param payables 应付台账行。
 */
function attachPayableSourceToInvoices(
    invoices: PurchaseInvoiceRow[],
    payables: readonly PayableRow[],
): PurchaseInvoiceRow[] {
    if (invoices.length === 0 || payables.length === 0) return invoices
    const byId = new Map(
        payables.map((payable) => [payable.payableAccountId, payable]),
    )
    return invoices.map((invoice) => ({
        ...invoice,
        allocations: invoice.allocations.map((allocation) => {
            const payable = byId.get(allocation.payableAccountId)
            if (!payable) return allocation
            return {
                ...allocation,
                sourceType: payable.sourceType,
                sourceDocumentNo: payable.sourceDocumentNo,
                sourceHref: payable.sourceHref,
                payableHref: payablePreviewHref(payable.payableAccountId),
            }
        }),
    }))
}
