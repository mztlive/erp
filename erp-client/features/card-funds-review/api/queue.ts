/**
 * W13 卡券票款复核 API · 队列加载与条目组装
 * (/admin/work-items、/admin/receivable-accounts)。
 * 队列项由 CARD_FUNDS_REVIEW / CARD_FUNDS_DELTA_REVIEW 任务 + 应收子账详情组装。
 */

import { apiGet } from "@/lib/api"
import {
    listWorkItems,
    mapWorkItemDto,
    type WorkItemProjection,
} from "@/features/work-items"
import type {
    CardFundsReviewItemView,
    CardFundsReviewQueueQuery,
    CardFundsReviewQueueView,
} from "@/features/card-funds-review/types"
import {
    filterSummary,
    instantToIso,
    mapPriority,
    mapReviewResultFrontend,
    mapReviewTypeFrontend,
} from "./mappers"
import type { BackendReceivableAccount } from "./dto"

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

export async function loadAccount(
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
        (action): action is "REASSIGN" => action === "REASSIGN",
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
