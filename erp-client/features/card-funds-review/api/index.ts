/**
 * W13 卡券票款复核 API：真实 HTTP
 * (/admin/work-items、/admin/receivable-accounts、/admin/receivable-funds-reviews、
 * customer-receipts、invoices)。
 * 队列项由 CARD_FUNDS_REVIEW / CARD_FUNDS_DELTA_REVIEW 任务 + 应收子账详情组装。
 */

import { apiGet, apiPost } from "@/lib/api"
import {
    listWorkItems,
    mapWorkItemDto,
    type WorkItemDto,
    type WorkItemProjection,
} from "@/features/work-items"
import type {
    CardFundsReviewItemView,
    CardFundsReviewQueueQuery,
    CardFundsReviewQueueView,
    CompleteCardFundsReviewCommand,
    FormalActionResponse,
    RegisterFundsResult,
} from "@/features/card-funds-review/types"
// ─── Backend DTOs ──────────────────────────────────────────────────────────

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
    subject_hash_at_review?: string | null
}

type BackendReceiptFact = {
    receipt_id: string
    receipt_no: string
    received_at: string
    gross_amount: string
    allocated_to_account: string
    other_allocation_summary?: string | null
    reversed: boolean
}

type BackendInvoiceFact = {
    invoice_id: string
    invoice_no: string
    direction: "BLUE" | "RED"
    issued_at: string
    gross_amount: string
    net_amount: string
    tax_amount: string
    allocated_to_account: string
    reversed: boolean
}

type BackendReceivableAccount = {
    id: string
    sales_order_id: string
    source_sales_order_revision_id: string
    current_sales_order_revision_id: string
    sales_order_no: string
    sales_order_revision_no: number
    sales_order_snapshot_at: number
    account_seq: number
    customer_id: string
    customer_name: string
    counterparty_party_id: string
    counterparty_party_name?: string | null
    review_status: string
    gross_total: string
    settled_total: string
    open_total: string
    invoiceable_total: string
    invoiced_total: string
    open_invoiceable_total: string
    status: string
    version: number
    account_domain_version: string
    created_at: number
    entries: BackendReceivableEntry[]
    reviews: BackendFundsReview[]
    review_chain_tail_id?: string | null
    review_chain_version?: string | null
    next_review_no?: number | null
    funds_fact_version?: string | null
    receipt_facts?: BackendReceiptFact[] | null
    invoice_facts?: BackendInvoiceFact[] | null
    work_item?: WorkItemDto | null
    active_review_type?: "OPENING" | "SYNC_DELTA" | null
    allowed_actions?: Array<
        | "CONFIRM_ZERO"
        | "APPROVE"
        | "REJECT"
        | "REGISTER_RECEIPT"
        | "REGISTER_INVOICE"
    >
    action_blockers?: Array<{
        action: string
        code: string
        message: string
    }>
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
        q.scope === "mine"
            ? "仅我的"
            : q.scope === "team"
              ? "团队"
              : "处理历史",
        q.type === "opening"
            ? "期初"
            : q.type === "delta"
              ? "同步差额"
              : "全部类型",
        q.status === "COMPLETED"
            ? "已完成"
            : q.status === "CLOSED"
              ? "已关闭"
              : "待处理有效队列",
        q.due === "overdue"
            ? "已超期"
            : q.due === "today"
              ? "今日到期"
              : "全部时限",
    ]
    if (q.q) parts.push(`搜索 ${q.q}`)
    return parts.join(" · ")
}

async function loadWorkItems(
    workItemType: string,
    query: CardFundsReviewQueueQuery,
): Promise<WorkItemProjection[]> {
    const page = await listWorkItems({
        scope: query.scope,
        workItemType,
        status:
            query.scope === "history" && query.status !== "OPEN"
                ? query.status
                : undefined,
        due:
            query.due === "today" || query.due === "overdue"
                ? query.due
                : undefined,
        query: query.q,
        sort: "priority_due",
        queueContextId: query.queueContextId,
        currentWorkItemId: query.currentWorkItemId,
        timezone: Intl.DateTimeFormat().resolvedOptions().timeZone,
        page: 1,
        pageSize: 100,
    })
    return page.items.map(mapWorkItemDto)
}

async function loadAccount(
    id: string,
    workItemId?: string,
): Promise<BackendReceivableAccount | null> {
    try {
        return await apiGet<BackendReceivableAccount>(
            `/admin/receivable-accounts/${encodeURIComponent(id)}`,
            { work_item_id: workItemId },
        )
    } catch (error) {
        if (
            error &&
            typeof error === "object" &&
            "status" in error &&
            (error as { status?: number }).status === 404
        ) {
            return null
        }
        throw error
    }
}

async function projectItem(
    wi: WorkItemProjection,
): Promise<CardFundsReviewItemView | null> {
    const accountId = wi.businessObjectId
    const account = await loadAccount(accountId, wi.workItemId)
    if (!account) {
        throw new Error(
            `任务 ${wi.workItemId} 绑定的应收子账 ${accountId} 不存在；已禁止隐藏任务或继续复核`,
        )
    }

    if (!account.work_item) {
        throw new Error(
            `应收子账 ${accountId} 未返回 actor-specific 正式任务；已禁止从队列待办推导领域动作`,
        )
    }
    const projectedWorkItem = mapWorkItemDto(account.work_item)
    if (
        projectedWorkItem.workItemId !== wi.workItemId ||
        projectedWorkItem.businessObjectId !== accountId
    ) {
        throw new Error("服务端返回的 W13 正式任务与队列身份不一致")
    }
    const reviewType = account.active_review_type
    if (!reviewType) {
        throw new Error("服务端未返回 W13 复核类型")
    }
    const workItemType =
        reviewType === "SYNC_DELTA"
            ? ("CARD_FUNDS_DELTA_REVIEW" as const)
            : ("CARD_FUNDS_REVIEW" as const)

    const responsibilityActions = projectedWorkItem.allowedActions.filter(
        (
            action,
        ): action is "START_PROCESSING" | "RELEASE_TO_TEAM" | "REASSIGN" =>
            ["START_PROCESSING", "RELEASE_TO_TEAM", "REASSIGN"].includes(
                action,
            ),
    )
    const allowedActions: CardFundsReviewItemView["workItem"]["allowedActions"] =
        [...responsibilityActions, ...(account.allowed_actions ?? [])]
    const actionBlockers: Array<{
        action: string
        code: string
        message: string
    }> = [
        ...(account.action_blockers ?? []),
        ...projectedWorkItem.actionBlockers.map((message) => ({
            action: "PROCESS_TASK",
            code: "WORK_ITEM_ACTION_BLOCKED",
            message,
        })),
    ]

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
        subjectHashAtReview: r.subject_hash_at_review ?? "",
        readOnly: true as const,
    }))

    const receiptFacts: CardFundsReviewItemView["receiptFacts"] = (
        account.receipt_facts ?? []
    ).map((fact) => ({
        receiptId: fact.receipt_id,
        receiptNo: fact.receipt_no,
        receivedAt: fact.received_at,
        grossAmount: fact.gross_amount,
        allocatedToAccount: fact.allocated_to_account,
        otherAllocationSummary: fact.other_allocation_summary ?? undefined,
        reversed: fact.reversed,
    }))
    const invoiceFacts: CardFundsReviewItemView["invoiceFacts"] = (
        account.invoice_facts ?? []
    ).map((fact) => ({
        invoiceId: fact.invoice_id,
        invoiceNo: fact.invoice_no,
        direction: fact.direction,
        issuedAt: fact.issued_at,
        grossAmount: fact.gross_amount,
        netAmount: fact.net_amount,
        taxAmount: fact.tax_amount,
        allocatedToAccount: fact.allocated_to_account,
        reversed: fact.reversed,
    }))

    return {
        workItem: {
            workItemId: projectedWorkItem.workItemId,
            taskVersion: projectedWorkItem.taskVersion,
            workItemType,
            subjectVersion: projectedWorkItem.subjectVersion,
            workItemStatus: projectedWorkItem.status,
            assignmentMode: projectedWorkItem.assignmentMode,
            dueAt: projectedWorkItem.dueAt
                ? instantToIso(projectedWorkItem.dueAt)
                : undefined,
            ownerUser: projectedWorkItem.ownerUser,
            allowedActions,
            actionBlockers,
            reason: projectedWorkItem.reasonLabel,
            impact: projectedWorkItem.impactSummary,
            priority: mapPriority(projectedWorkItem.priority),
        },
        salesOrder: {
            id: account.sales_order_id,
            orderNo: account.sales_order_no,
            revisionNo: account.sales_order_revision_no,
            snapshotAt: instantToIso(account.sales_order_snapshot_at),
        },
        account: {
            id: account.id,
            accountSeq: account.account_seq,
            domainVersion: account.account_domain_version,
            customerId: account.customer_id,
            customerName: account.customer_name,
            counterpartyPartyId: account.counterparty_party_id,
            counterpartyPartyName: account.counterparty_party_name ?? "",
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
            tailReviewId: account.review_chain_tail_id ?? undefined,
            chainVersion: account.review_chain_version ?? "",
            nextReviewNo: account.next_review_no ?? 0,
            items: chainItems,
        },
        currentSalesOrderRevisionId: account.current_sales_order_revision_id,
        fundsFactVersion: account.funds_fact_version ?? "",
        receiptFacts,
        invoiceFacts,
        reviewType,
        fingerprintStatus: {
            label: "数据版本",
            tone: "neutral",
            detail: `subject=${projectedWorkItem.subjectVersion}`,
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
    if (
        (query.scope === "history" && query.status === "OPEN") ||
        (query.scope !== "history" && query.status !== "OPEN")
    ) {
        throw {
            kind: "Validation",
            status: 400,
            message: "当前责任范围与任务状态不兼容，请重新选择筛选条件",
        }
    }
    const types: string[] = []
    if (query.type === "opening") types.push("CARD_FUNDS_REVIEW")
    else if (query.type === "delta") types.push("CARD_FUNDS_DELTA_REVIEW")
    else types.push("CARD_FUNDS_REVIEW", "CARD_FUNDS_DELTA_REVIEW")

    const allItems = (
        await Promise.all(types.map((t) => loadWorkItems(t, query)))
    ).flat()

    const tasks = (
        await Promise.all(allItems.map((wi) => projectItem(wi)))
    ).filter((t): t is CardFundsReviewItemView => t != null)

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

export async function completeCardFundsReview(
    input: CompleteCardFundsReviewCommand,
): Promise<FormalActionResponse> {
    try {
        const result = await apiPost<{
            work_item_id: string
            work_item_status: "COMPLETED"
            business_result: {
                receivable_funds_review_id: string
                receivable_account_id: string
                review_no: number
                account_review_status: string
                workflow_action_id: string
                operation_id: string
                completed_at: string
                review_result: "APPROVED" | "REJECTED"
                conclusion:
                    | "NO_HISTORY_FROM_ZERO"
                    | "RECORDED_FACTS_RECONCILED"
                    | "REJECTED"
                follow_up_configuration?: {
                    status: "BLOCKED"
                    blocker_code: "REJECT_FOLLOW_UP_WORK_ITEM_NOT_REGISTERED"
                    collaboration_message: string
                    required_registration: readonly (
                        | "WORK_ITEM_TYPE"
                        | "OWNER_POOL"
                        | "HANDLER_KEY"
                    )[]
                }
            }
        }>("/admin/receivable-funds-reviews", {
            work_item_id: input.workItemId,
            expected_task_version: input.expectedTaskVersion,
            expected_subject_version: input.expectedSubjectVersion,
            decision: {
                review_result: input.decision.reviewResult,
                conclusion: input.decision.conclusion,
                reason_code:
                    input.decision.reviewResult === "REJECTED"
                        ? input.decision.reasonCode
                        : undefined,
                receivable_account_id: input.decision.receivableAccountId,
                expected_account_seq: input.decision.expectedAccountSeq,
                expected_account_domain_version:
                    input.decision.expectedAccountDomainVersion,
                expected_review_chain_tail_id:
                    input.decision.expectedReviewChainTailId,
                expected_review_chain_version:
                    input.decision.expectedReviewChainVersion,
                expected_next_review_no: input.decision.expectedNextReviewNo,
                expected_sales_order_revision_id:
                    input.decision.expectedSalesOrderRevisionId,
                expected_funds_fact_version:
                    input.decision.expectedFundsFactVersion,
                review_type: input.decision.reviewType,
                evidence_document_ids: input.decision.evidenceDocumentIds,
                evidence_references: input.decision.evidenceReferences,
                comment: input.decision.comment,
            },
            idempotency_key: input.idempotencyKey,
        })
        const row = result.business_result
        if (
            result.work_item_id !== input.workItemId ||
            result.work_item_status !== "COMPLETED" ||
            !row?.receivable_funds_review_id ||
            !row.workflow_action_id ||
            !row.operation_id
        ) {
            return {
                status: "failed",
                code: "INCOMPLETE_FORMAL_RESULT",
                message: "任务、复核记录或操作号不完整；当前结果不能按成功展示",
            }
        }
        const businessBase = {
            receivableFundsReviewId: row.receivable_funds_review_id,
            receivableAccountId: row.receivable_account_id,
            reviewNo: row.review_no,
            accountReviewStatus: row.account_review_status,
            workflowActionId: row.workflow_action_id,
            operationId: row.operation_id,
            completedAt: row.completed_at,
        }
        if (row.review_result === "APPROVED") {
            if (
                row.conclusion !== "NO_HISTORY_FROM_ZERO" &&
                row.conclusion !== "RECORDED_FACTS_RECONCILED"
            ) {
                return {
                    status: "failed",
                    code: "INCOMPLETE_FORMAL_RESULT",
                    message: "通过复核的记录不完整，请刷新任务核对处理结果",
                }
            }
            return {
                status: "succeeded",
                outcome: {
                    kind: "APPROVED",
                    business: {
                        ...businessBase,
                        reviewResult: "APPROVED",
                        conclusion: row.conclusion,
                    },
                },
            }
        }
        if (
            row.review_result !== "REJECTED" ||
            row.conclusion !== "REJECTED" ||
            row.follow_up_configuration?.status !== "BLOCKED" ||
            row.follow_up_configuration.blocker_code !==
                "REJECT_FOLLOW_UP_WORK_ITEM_NOT_REGISTERED"
        ) {
            return {
                status: "failed",
                code: "INCOMPLETE_FORMAL_RESULT",
                message: "驳回后的处理规则不完整；当前结果不能按成功展示",
            }
        }
        return {
            status: "succeeded",
            outcome: {
                kind: "REJECTED",
                business: {
                    ...businessBase,
                    reviewResult: "REJECTED",
                    conclusion: "REJECTED",
                    followUpConfiguration: {
                        status: row.follow_up_configuration.status,
                        blockerCode: row.follow_up_configuration.blocker_code,
                        collaborationMessage:
                            row.follow_up_configuration.collaboration_message,
                        requiredRegistration:
                            row.follow_up_configuration.required_registration,
                    },
                },
            },
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
        const kind =
            err && typeof err === "object" && "kind" in err
                ? (err as { kind?: unknown }).kind
                : undefined
        if (kind === "Network" || kind === "Parse") {
            return {
                status: "unknown",
                idempotencyKey: input.idempotencyKey,
                message:
                    "请求结果尚未确认；请按操作号查询处理结果，确认前不得再次推进任务",
            }
        }
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

    const workItems = await listWorkItems({
        scope: "mine",
        workItemType: "CARD_FUNDS_REVIEW",
        currentWorkItemId: input.workItemId,
        timezone: Intl.DateTimeFormat().resolvedOptions().timeZone,
        page: 1,
        pageSize: 1,
    })
    const workItem = workItems.items.find(
        (item) => item.id === input.workItemId,
    )
    const account = workItem
        ? await loadAccount(workItem.business_object_id)
        : null
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

    const workItems = await listWorkItems({
        scope: "mine",
        workItemType: "CARD_FUNDS_REVIEW",
        currentWorkItemId: input.workItemId,
        timezone: Intl.DateTimeFormat().resolvedOptions().timeZone,
        page: 1,
        pageSize: 1,
    })
    const workItem = workItems.items.find(
        (item) => item.id === input.workItemId,
    )
    const account = workItem
        ? await loadAccount(workItem.business_object_id)
        : null
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
