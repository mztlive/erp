/**
 * W13 卡券票款复核 API：真实 HTTP
 * (/admin/work-items、/admin/receivable-accounts、/admin/receivable-funds-reviews、
 * customer-receipts、invoices)。
 * 队列项由 CARD_FUNDS_REVIEW / CARD_FUNDS_DELTA_REVIEW 任务 + 应收子账详情组装。
 */

import { apiGet, apiPost } from "@/lib/api"
import type { Page } from "@/lib/api"
import type {
    CardFundsReviewDecision,
    CardFundsReviewItemView,
    CardFundsReviewQueueQuery,
    CardFundsReviewQueueView,
    FormalActionResponse,
    FormalOutcome,
    RegisterFundsResult,
    WorkItemLease,
} from "@/features/card-funds-review/types"
import { REJECT_FOLLOW_UP_COLLABORATION } from "@/features/card-funds-review/types"
// ─── Backend DTOs ──────────────────────────────────────────────────────────

type BackendWorkItem = {
    id: string
    work_item_type: string
    business_object_type: string
    business_object_id: string
    subject_version?: string | null
    status: string
    owner_role?: string | null
    owner_user_id?: string | null
    priority: string | number
    due_at?: number | null
    reason_code?: string | null
    impact_summary?: string | null
    completion_action: string
    completed_at?: number | null
    completed_by?: string | null
    version: number
    created_at: number
}

type BackendReceivableEntry = {
    id: string
    entry_type: string
    direction: string
    amount: string
    due_date: string
    source_document_id: string
    posted_at: number
}

type BackendFundsReview = {
    id: string
    review_no: number
    review_type: string
    review_result: string
    reviewed_by: string
    reviewed_at: number
    evidence_reference?: string | null
}

type BackendReceivableAccount = {
    id: string
    sales_order_id: string
    account_seq: number
    customer_id: string
    counterparty_party_id: string
    review_status: string
    gross_total: string
    settled_total: string
    open_total: string
    invoiceable_total: string
    invoiced_total: string
    open_invoiceable_total: string
    status: string
    version: number
    created_at: number
    entries: BackendReceivableEntry[]
    reviews: BackendFundsReview[]
}

type BackendCustomerReceipt = {
    id: string
    receipt_no: string
    status: string
    received_at: number
    amount: string
    allocated_total: string
    unallocated_amount: string
    allocations: Array<{
        id: string
        receivable_entry_id: string
        allocated_amount: string
        allocation_action: string
    }>
}

type BackendInvoice = {
    id: string
    invoice_no: string
    invoice_kind: "blue" | "red"
    invoice_date: string
    gross_amount: string
    net_amount: string
    tax_amount: string
    allocated_total: string
    status: string
    allocations: Array<{
        id: string
        receivable_account_id?: string
        allocated_gross_amount: string
        allocation_action: string
    }>
}

// ─── Helpers ───────────────────────────────────────────────────────────────

function instantToIso(secs: number | undefined | null): string {
    if (secs == null || !Number.isFinite(Number(secs))) return ""
    return new Date(Number(secs) * 1000).toISOString()
}

function mapWorkItemStatus(
    s: string,
): CardFundsReviewItemView["workItem"]["workItemStatus"] {
    if (s === "COMPLETED" || s === "completed") return "COMPLETED"
    if (s === "IN_PROGRESS" || s === "in_progress") return "IN_PROGRESS"
    // UNCLAIMED / CLOSED / other → PENDING for queue display
    return "PENDING"
}

function mapPriority(p: string | number): number {
    if (typeof p === "number") return p
    switch (p) {
        case "urgent":
            return 100
        case "high":
            return 80
        case "low":
            return 20
        default:
            return 50
    }
}

function reviewTypeFromWorkItem(
    workItemType: string,
): CardFundsReviewItemView["reviewType"] {
    if (
        workItemType === "CARD_FUNDS_DELTA_REVIEW" ||
        workItemType === "card_funds_delta_review"
    ) {
        return "SYNC_DELTA"
    }
    return "OPENING"
}

function mapFundsReviewType(
    t: CardFundsReviewItemView["reviewType"],
): "opening" | "sync_delta" {
    return t === "SYNC_DELTA" ? "sync_delta" : "opening"
}

function mapReviewResultBackend(
    r: "APPROVED" | "REJECTED",
): "passed" | "rejected" {
    return r === "APPROVED" ? "passed" : "rejected"
}

function mapReviewResultFrontend(r: string): "APPROVED" | "REJECTED" {
    return r === "passed" || r === "APPROVED" ? "APPROVED" : "REJECTED"
}

function mapReviewTypeFrontend(
    t: string,
): CardFundsReviewItemView["reviewType"] {
    if (t === "sync_delta" || t === "SYNC_DELTA") return "SYNC_DELTA"
    return "OPENING"
}

function filterSummary(q: CardFundsReviewQueueQuery): string {
    const parts = [
        q.scope === "mine" ? "仅我的" : "团队",
        q.type === "opening"
            ? "期初"
            : q.type === "delta"
              ? "同步差额"
              : "全部类型",
        q.status === "held" ? "已跳过" : "待处理有效队列",
        q.due === "overdue"
            ? "已超期"
            : q.due === "today"
              ? "今日到期"
              : "全部时限",
    ]
    if (q.q) parts.push(`搜索 ${q.q}`)
    return parts.join(" · ")
}

async function loadWorkItems(workItemType: string): Promise<BackendWorkItem[]> {
    const page = await apiGet<Page<BackendWorkItem>>("/admin/work-items", {
        work_item_type: workItemType,
        page: 1,
        page_size: 100,
        sort_by: "created_at",
        sort_dir: "desc",
    })
    return page.items ?? []
}

async function loadAccount(
    id: string,
): Promise<BackendReceivableAccount | null> {
    try {
        return await apiGet<BackendReceivableAccount>(
            `/admin/receivable-accounts/${encodeURIComponent(id)}`,
        )
    } catch {
        return null
    }
}

async function projectItem(
    wi: BackendWorkItem,
): Promise<CardFundsReviewItemView | null> {
    if (wi.status === "COMPLETED" || wi.status === "CLOSED") return null

    const accountId = wi.business_object_id
    const account = await loadAccount(accountId)
    if (!account) return null

    const reviewType = reviewTypeFromWorkItem(wi.work_item_type)
    const workItemType =
        reviewType === "SYNC_DELTA"
            ? ("CARD_FUNDS_DELTA_REVIEW" as const)
            : ("CARD_FUNDS_REVIEW" as const)

    const settled = account.settled_total
    const invoiced = account.invoiced_total
    const canConfirmZero =
        reviewType === "OPENING" &&
        (settled === "0" || settled === "0.00") &&
        (invoiced === "0" || invoiced === "0.00")

    const status = mapWorkItemStatus(wi.status)
    const held = false

    const allowedActions: Array<
        CardFundsReviewItemView["workItem"]["allowedActions"][number]
    > = [
        "CLAIM",
        "APPROVE",
        "REJECT",
        "HOLD",
        "REGISTER_RECEIPT",
        "REGISTER_INVOICE",
    ]
    if (canConfirmZero) {
        allowedActions.push("CONFIRM_ZERO")
    }

    const actionBlockers: Array<{
        action: string
        code: string
        message: string
    }> = []
    if (!canConfirmZero) {
        actionBlockers.push({
            action: "CONFIRM_ZERO",
            code:
                reviewType !== "OPENING"
                    ? "NOT_OPENING"
                    : "SETTLED_OR_INVOICED_NOT_ZERO",
            message:
                reviewType !== "OPENING"
                    ? "「从 0 起」仅适用于期初复核任务"
                    : "净已收或净已开不为 0，不能使用「从 0 起」结论",
        })
    }

    const chainItems = (account.reviews ?? []).map((r) => ({
        reviewId: r.id,
        reviewNo: r.review_no,
        reviewType: mapReviewTypeFrontend(r.review_type),
        reviewResult: mapReviewResultFrontend(r.review_result),
        conclusion:
            mapReviewResultFrontend(r.review_result) === "REJECTED"
                ? ("REJECTED" as const)
                : ("RECORDED_FACTS_RECONCILED" as const),
        reviewerLabel: r.reviewed_by,
        completedAt: instantToIso(r.reviewed_at),
        subjectHashAtReview: wi.subject_version ?? String(account.version),
        readOnly: true as const,
    }))

    const subjectHash =
        wi.subject_version ?? `acct:${account.id}:v${account.version}`
    const fundsFactVersion = `ffv:${account.id}:v${account.version}`

    // receipt/invoice facts: not linked by work-item; leave empty unless we can filter by party (gap)
    const receiptFacts: CardFundsReviewItemView["receiptFacts"] = []
    const invoiceFacts: CardFundsReviewItemView["invoiceFacts"] = []

    return {
        workItem: {
            workItemId: wi.id,
            workItemType,
            completionAction: wi.completion_action,
            subjectVersion: String(wi.version),
            subjectHash,
            workItemStatus: status,
            dueAt: wi.due_at ? instantToIso(wi.due_at) : undefined,
            claimedBy: wi.owner_user_id
                ? { userId: wi.owner_user_id, displayName: wi.owner_user_id }
                : undefined,
            allowedActions,
            actionBlockers,
            held,
            reason: wi.reason_code ?? "卡券票款复核",
            impact: wi.impact_summary ?? "",
            priority: mapPriority(wi.priority),
        },
        salesOrder: {
            id: account.sales_order_id,
            orderNo: account.sales_order_id,
            revisionNo: 1,
            snapshotAt: instantToIso(account.created_at),
        },
        account: {
            id: account.id,
            accountSeq: account.account_seq,
            domainVersion: String(account.version),
            customerId: account.customer_id,
            customerName: account.customer_id,
            counterpartyPartyId: account.counterparty_party_id,
            counterpartyPartyName: account.counterparty_party_id,
            mallName: "",
            reviewStatus: account.review_status,
            grossTotal: account.gross_total,
            settledTotal: account.settled_total,
            openTotal: account.open_total,
            invoicedTotal: account.invoiced_total,
            openInvoiceableTotal: account.open_invoiceable_total,
            syncedGrossAmount: account.gross_total,
            fundsReliability:
                account.review_status === "reviewed"
                    ? "VERIFIED"
                    : "UNRELIABLE_PENDING_REVIEW",
            reliabilityNote:
                account.review_status === "reviewed"
                    ? "卡券票款复核已通过"
                    : "卡券票款待复核，指标暂不可靠",
        },
        reviewChain: {
            tailReviewId:
                chainItems.length > 0
                    ? chainItems[chainItems.length - 1]!.reviewId
                    : undefined,
            chainVersion: `cv:${chainItems.length}`,
            nextReviewNo:
                chainItems.length > 0
                    ? chainItems[chainItems.length - 1]!.reviewNo + 1
                    : 1,
            items: chainItems,
        },
        currentSalesOrderRevisionId: account.sales_order_id,
        fundsFactVersion,
        receiptFacts,
        invoiceFacts,
        reviewType,
        fingerprintStatus: {
            label: "数据版本",
            tone: "neutral",
            detail: `subject=${subjectHash}`,
        },
        currentEvidence: {
            evidenceDocumentIds: [],
            evidenceReferences: [],
            comment: undefined,
        },
    }
}

// ─── Public API ────────────────────────────────────────────────────────────

export async function fetchCardFundsReviewQueue(
    query: CardFundsReviewQueueQuery,
): Promise<CardFundsReviewQueueView> {
    const types: string[] = []
    if (query.type === "opening") types.push("CARD_FUNDS_REVIEW")
    else if (query.type === "delta") types.push("CARD_FUNDS_DELTA_REVIEW")
    else types.push("CARD_FUNDS_REVIEW", "CARD_FUNDS_DELTA_REVIEW")

    const allItems = (
        await Promise.all(types.map((t) => loadWorkItems(t)))
    ).flat()

    let tasks = (
        await Promise.all(allItems.map((wi) => projectItem(wi)))
    ).filter((t): t is CardFundsReviewItemView => t != null)

    if (query.status === "held") {
        tasks = tasks.filter((t) => t.workItem.held)
    }

    if (query.q?.trim()) {
        const q = query.q.trim().toUpperCase()
        tasks = tasks.filter(
            (t) =>
                t.salesOrder.orderNo.toUpperCase().includes(q) ||
                t.account.customerName.toUpperCase().includes(q) ||
                t.account.counterpartyPartyName.toUpperCase().includes(q) ||
                t.account.id.toUpperCase().includes(q),
        )
    }

    // due filters require client clock for "overdue"/"today" — use server due_at only when present
    if (query.due === "overdue") {
        const now = Date.now()
        tasks = tasks.filter(
            (t) =>
                t.workItem.dueAt && new Date(t.workItem.dueAt).getTime() < now,
        )
    } else if (query.due === "today") {
        const today = new Date().toISOString().slice(0, 10)
        tasks = tasks.filter((t) => t.workItem.dueAt?.startsWith(today))
    }

    tasks = [...tasks].sort((a, b) => b.workItem.priority - a.workItem.priority)

    const queueContextId =
        query.queueContextId ?? `queue:card-funds-review:${query.scope}`

    let position = 0
    let current = tasks[0]
    if (query.currentWorkItemId) {
        const idx = tasks.findIndex(
            (t) => t.workItem.workItemId === query.currentWorkItemId,
        )
        if (idx >= 0) {
            position = idx
            current = tasks[idx]
        }
    }

    return {
        preferences: { autoNextDefault: true },
        context: {
            queueContextId,
            position: tasks.length === 0 ? 0 : position + 1,
            total: tasks.length,
            currentWorkItemId: current?.workItem.workItemId,
            previousWorkItemId: tasks[position - 1]?.workItem.workItemId,
            nextWorkItemId: tasks[position + 1]?.workItem.workItemId,
            filterSummary: filterSummary(query),
            queueContextUpdatedAt: new Date().toISOString(),
        },
        tasks,
        current,
        emptyReason: tasks.length === 0 ? "NO_TASKS" : undefined,
    }
}

export async function claimCardFundsReviewWorkItem(
    workItemId: string,
): Promise<WorkItemLease> {
    const detail = await apiGet<BackendWorkItem>(
        `/admin/work-items/${encodeURIComponent(workItemId)}`,
    )
    const claimed = await apiPost<BackendWorkItem>(
        `/admin/work-items/${encodeURIComponent(workItemId)}/claim`,
        { version: detail.version },
    )
    return {
        workItemId,
        claimedByLabel: claimed.owner_user_id ?? "当前用户",
        subjectVersion: String(claimed.version),
    }
}

export async function holdCardFundsReview(input: {
    workItemId: string
    expectedSubjectVersion: string
    reasonCode: string
    note?: string
    nextWorkItemId?: string
}): Promise<FormalActionResponse> {
    try {
        const detail = await apiGet<BackendWorkItem>(
            `/admin/work-items/${encodeURIComponent(input.workItemId)}`,
        )
        const version = Number(input.expectedSubjectVersion) || detail.version
        await apiPost(
            `/admin/work-items/${encodeURIComponent(input.workItemId)}/defer`,
            {
                version,
                comment: input.note ?? input.reasonCode,
            },
        )
        const outcome: FormalOutcome = {
            kind: "HELD",
            workItemId: input.workItemId,
            workItemStatus: "IN_PROGRESS",
            heldAt: new Date().toISOString(),
            resumeHint: "任务已暂挂。未形成复核记录。可在队列中重新领取处理。",
            reference: `W13-HOLD-${input.workItemId.toUpperCase()}`,
            nextWorkItemId: input.nextWorkItemId,
        }
        return { status: "succeeded", outcome }
    } catch (err) {
        const message =
            err && typeof err === "object" && "message" in err
                ? String((err as { message: unknown }).message)
                : "暂挂失败"
        const code =
            err && typeof err === "object" && "status" in err
                ? String((err as { status?: number }).status ?? "HTTP_ERROR")
                : "HTTP_ERROR"
        return { status: "failed", code, message }
    }
}

export async function completeCardFundsReview(input: {
    workItemId: string
    expectedSubjectVersion: string
    decision: CardFundsReviewDecision
}): Promise<FormalActionResponse> {
    try {
        const detail = await apiGet<BackendWorkItem>(
            `/admin/work-items/${encodeURIComponent(input.workItemId)}`,
        )
        const account = await loadAccount(input.decision.receivableAccountId)
        if (!account) {
            return {
                status: "failed",
                code: "NOT_FOUND",
                message: "应收往来子账不存在",
            }
        }

        if (
            input.decision.evidenceDocumentIds.length === 0 &&
            input.decision.evidenceReferences.length === 0
        ) {
            return {
                status: "failed",
                code: "EVIDENCE_REQUIRED",
                message: "完成复核时证据不能为空",
            }
        }

        if (
            input.decision.reviewResult === "APPROVED" &&
            input.decision.conclusion === "NO_HISTORY_FROM_ZERO"
        ) {
            if (input.decision.reviewType !== "OPENING") {
                return {
                    status: "failed",
                    code: "ZERO_ONLY_OPENING",
                    message: "「从 0 起」仅允许 OPENING + APPROVED",
                }
            }
        }

        const nowSecs = Math.floor(Date.now() / 1000)
        const evidenceRef =
            input.decision.evidenceReferences[0] ??
            input.decision.evidenceDocumentIds[0] ??
            ""

        const review = await apiPost<{
            id: string
            review_no: number
            review_type: string
            review_result: string
            reviewed_by: string
            reviewed_at: number
        }>("/admin/receivable-funds-reviews", {
            receivable_account_id: input.decision.receivableAccountId,
            work_item_id: input.workItemId,
            review_type: mapFundsReviewType(input.decision.reviewType),
            review_result: mapReviewResultBackend(input.decision.reviewResult),
            evidence_reference: evidenceRef || undefined,
            reviewed_by: "finance_reviewer",
            reviewed_at: nowSecs,
        })

        const version = Number(input.expectedSubjectVersion) || detail.version
        await apiPost(
            `/admin/work-items/${encodeURIComponent(input.workItemId)}/complete`,
            { version },
        )

        const completedAt = new Date().toISOString()
        const operationId = `op_w13_${input.workItemId.slice(0, 12)}`
        const workflowActionId = `wa_w13_${input.workItemId}_${review.review_no}`

        if (input.decision.reviewResult === "APPROVED") {
            const business = {
                receivableFundsReviewId: review.id,
                receivableAccountId: input.decision.receivableAccountId,
                reviewNo: review.review_no,
                accountReviewStatus:
                    input.decision.reviewType === "OPENING"
                        ? "OPENING_APPROVED"
                        : "DELTA_APPROVED",
                workflowActionId,
                operationId,
                completedAt,
                reviewResult: "APPROVED" as const,
                conclusion: input.decision.conclusion,
                subjectHash:
                    detail.subject_version ??
                    `acct:${account.id}:v${account.version}`,
                reference: `W13-OK-${String(review.review_no).padStart(4, "0")}`,
            }
            return {
                status: "succeeded",
                outcome: { kind: "APPROVED", business },
            }
        }

        const business = {
            receivableFundsReviewId: review.id,
            receivableAccountId: input.decision.receivableAccountId,
            reviewNo: review.review_no,
            accountReviewStatus: "REJECTED",
            workflowActionId,
            operationId,
            completedAt,
            reviewResult: "REJECTED" as const,
            conclusion: "REJECTED" as const,
            subjectHash:
                detail.subject_version ??
                `acct:${account.id}:v${account.version}`,
            reference: `W13-REJ-${String(review.review_no).padStart(4, "0")}`,
            followUpConfiguration: {
                status: "BLOCKED" as const,
                blockerCode:
                    "REJECT_FOLLOW_UP_WORK_ITEM_NOT_REGISTERED" as const,
                collaborationMessage: REJECT_FOLLOW_UP_COLLABORATION,
                requiredRegistration: [
                    "WORK_ITEM_TYPE" as const,
                    "OWNER_POOL" as const,
                    "HANDLER_KEY" as const,
                ],
            },
        }
        return {
            status: "succeeded",
            outcome: { kind: "REJECTED", business },
        }
    } catch (err) {
        const message =
            err && typeof err === "object" && "message" in err
                ? String((err as { message: unknown }).message)
                : "完成复核失败"
        const status =
            err && typeof err === "object" && "status" in err
                ? (err as { status?: number }).status
                : undefined
        return {
            status: "failed",
            code:
                status === 409
                    ? "SUBJECT_HASH_MISMATCH"
                    : String(status ?? "HTTP_ERROR"),
            message,
        }
    }
}

/**
 * 登记历史回款：create + post customer receipt with entry allocations.
 */
export async function registerHistoricalReceipt(input: {
    workItemId: string
    expectedSubjectVersion: string
    receiptNo: string
    receivedAt: string
    grossAmount: string
    allocations: readonly {
        lineId: string
        targetAccountId: string
        targetLabel: string
        amount: string
    }[]
    evidenceReference: string
}): Promise<RegisterFundsResult> {
    if (!input.grossAmount || Number(input.grossAmount) <= 0) {
        return Promise.reject({
            kind: "Validation",
            message: "禁止创建 0 元或负金额回款；无历史票款请使用「从 0 起」",
        })
    }

    const wi = await apiGet<BackendWorkItem>(
        `/admin/work-items/${encodeURIComponent(input.workItemId)}`,
    )
    const account = await loadAccount(wi.business_object_id)
    if (!account) {
        return Promise.reject({
            kind: "Http",
            message: "应收往来子账不存在",
            status: 404,
        })
    }

    // Prefer increase entries as allocation targets
    const increaseEntry = (account.entries ?? []).find(
        (e) => e.direction === "increase",
    )
    const receivedAtSecs = input.receivedAt
        ? Math.floor(new Date(input.receivedAt).getTime() / 1000)
        : Math.floor(Date.now() / 1000)

    const created = await apiPost<BackendCustomerReceipt>(
        "/admin/customer-receipts",
        {
            receipt_no: input.receiptNo,
            counterparty_party_id: account.counterparty_party_id,
            customer_id: account.customer_id,
            received_at: receivedAtSecs,
            amount: input.grossAmount,
            bank_reference: input.evidenceReference || undefined,
        },
    )

    const entryId = increaseEntry?.id
    let posted = created
    if (entryId) {
        posted = await apiPost<BackendCustomerReceipt>(
            `/admin/customer-receipts/${encodeURIComponent(created.id)}/post`,
            {
                allocations: [
                    {
                        receivable_entry_id: entryId,
                        allocated_amount: input.grossAmount,
                    },
                ],
            },
        )
    }

    const refreshed = await loadAccount(account.id)
    const subjectHash = `acct:${account.id}:v${refreshed?.version ?? account.version}`
    return {
        fundsFactVersion: `ffv:${account.id}:v${refreshed?.version ?? account.version}`,
        subjectHash,
        settledTotal: refreshed?.settled_total ?? account.settled_total,
        invoicedTotal: refreshed?.invoiced_total ?? account.invoiced_total,
        openTotal: refreshed?.open_total ?? account.open_total,
        openInvoiceableTotal:
            refreshed?.open_invoiceable_total ?? account.open_invoiceable_total,
        receiptFacts: [
            {
                receiptId: posted.id,
                receiptNo: posted.receipt_no,
                receivedAt: instantToIso(posted.received_at),
                grossAmount: posted.amount,
                allocatedToAccount: posted.allocated_total,
                reversed: posted.status === "reversed",
            },
        ],
        invoiceFacts: [],
    }
}

export async function registerHistoricalInvoice(input: {
    workItemId: string
    expectedSubjectVersion: string
    invoiceNo: string
    issuedAt: string
    grossAmount: string
    netAmount: string
    taxAmount: string
    allocations: readonly {
        lineId: string
        targetAccountId: string
        targetLabel: string
        amount: string
    }[]
    evidenceReference: string
}): Promise<RegisterFundsResult> {
    if (!input.grossAmount || Number(input.grossAmount) <= 0) {
        return Promise.reject({
            kind: "Validation",
            message: "禁止创建 0 元或负金额发票；无历史票款请使用「从 0 起」",
        })
    }

    const wi = await apiGet<BackendWorkItem>(
        `/admin/work-items/${encodeURIComponent(input.workItemId)}`,
    )
    const account = await loadAccount(wi.business_object_id)
    if (!account) {
        return Promise.reject({
            kind: "Http",
            message: "应收往来子账不存在",
            status: 404,
        })
    }

    const created = await apiPost<BackendInvoice>("/admin/invoices", {
        invoice_direction: "sales",
        invoice_kind: "blue",
        party_id: account.counterparty_party_id,
        invoice_no: input.invoiceNo,
        invoice_date: input.issuedAt.slice(0, 10),
        gross_amount: input.grossAmount,
        net_amount: input.netAmount,
        tax_amount: input.taxAmount,
    })

    const posted = await apiPost<BackendInvoice>(
        `/admin/invoices/${encodeURIComponent(created.id)}/post`,
        {
            allocations: [
                {
                    receivable_account_id: account.id,
                    allocated_gross_amount: input.grossAmount,
                    allocated_net_amount: input.netAmount,
                    allocated_tax_amount: input.taxAmount,
                },
            ],
        },
    )

    const refreshed = await loadAccount(account.id)
    const subjectHash = `acct:${account.id}:v${refreshed?.version ?? account.version}`
    return {
        fundsFactVersion: `ffv:${account.id}:v${refreshed?.version ?? account.version}`,
        subjectHash,
        settledTotal: refreshed?.settled_total ?? account.settled_total,
        invoicedTotal: refreshed?.invoiced_total ?? account.invoiced_total,
        openTotal: refreshed?.open_total ?? account.open_total,
        openInvoiceableTotal:
            refreshed?.open_invoiceable_total ?? account.open_invoiceable_total,
        receiptFacts: [],
        invoiceFacts: [
            {
                invoiceId: posted.id,
                invoiceNo: posted.invoice_no,
                direction: "BLUE",
                issuedAt: posted.invoice_date,
                grossAmount: posted.gross_amount,
                netAmount: posted.net_amount,
                taxAmount: posted.tax_amount,
                allocatedToAccount: posted.allocated_total,
                reversed: false,
            },
        ],
    }
}

export async function saveCardFundsEvidence(input: {
    workItemId: string
    expectedSubjectVersion: string
    evidenceDocumentIds: string[]
    evidenceReferences: string[]
    comment?: string
}): Promise<{ ok: true }> {
    // Evidence draft has no dedicated backend endpoint; accepted client-side only.
    // Formal evidence is submitted with complete (append_funds_review).
    void input
    return { ok: true }
}
