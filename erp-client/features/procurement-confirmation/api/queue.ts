/** 采购确认待办队列：读取工作项、投影正式任务并组装队列视图。 */

import { listWorkItems, mapWorkItemDto } from "@/features/work-items"
import type { WorkItemProjection } from "@/features/work-items"

import { fetchConfirmationDetail, fetchSalesOrderDetail } from "./details"
import {
    emptyCoverageFromLines,
    filterSummary,
    mapConfirmationLines,
    mapFulfillmentMode,
    priorityToNumber,
    secsToIso,
} from "./mapping"
import { isServerIssuedQueueContextId, type QueueFilters } from "./filters"
import type {
    ProcurementConfirmationTask,
    ProcurementQueueView,
    SubmissionOrigin,
} from "@/features/procurement-confirmation/types"

type WorkItemQueuePage = {
    items: WorkItemProjection[]
    queueContextId?: string
}

/**
 * 拉取 W07 责任队列。
 * URL / W02 带来的 queueContextId 不是本查询的同条件哈希，不能提交；
 * 仅当调用方传入上一次本查询成功响应里的服务端上下文时才回传。
 */
async function fetchWorkItemsForQueue(
    filters: QueueFilters,
    listQueueContextId?: string,
): Promise<WorkItemQueuePage> {
    const page = await listWorkItems({
        scope: filters.scope,
        workItemType: "PROCUREMENT_CONFIRMATION",
        due:
            filters.due === "today" || filters.due === "overdue"
                ? filters.due
                : undefined,
        query: filters.orderNo,
        sort:
            filters.sort === "priority"
                ? "priority_due"
                : filters.sort === "submitted_at"
                  ? "created_desc"
                  : "due_asc",
        queueContextId: isServerIssuedQueueContextId(listQueueContextId)
            ? listQueueContextId
            : undefined,
        currentWorkItemId: filters.currentWorkItemId,
        timezone: Intl.DateTimeFormat().resolvedOptions().timeZone,
        page: 1,
        pageSize: 100,
    })
    return {
        items: page.items.map(mapWorkItemDto),
        queueContextId: page.queue_context_id ?? undefined,
    }
}

async function projectTask(
    workItem: WorkItemProjection,
    scope: QueueFilters["scope"],
): Promise<ProcurementConfirmationTask | null> {
    if (workItem.status !== "OPEN") {
        return null
    }

    const confirmationId = workItem.businessObjectId
    const detail = await fetchConfirmationDetail(
        confirmationId,
        workItem.workItemId,
    )
    if (!detail) {
        throw new Error(
            `任务 ${workItem.workItemId} 绑定的采购确认 ${confirmationId} 不存在；已禁止隐藏任务或继续处理`,
        )
    }
    if (!detail.work_item) {
        throw new Error(
            `采购确认 ${confirmationId} 未返回 actor-specific 正式任务；已禁止从队列待办推导领域动作`,
        )
    }
    const projectedWorkItem = mapWorkItemDto(detail.work_item)
    if (
        projectedWorkItem.workItemId !== workItem.workItemId ||
        projectedWorkItem.businessObjectId !== confirmationId
    ) {
        throw new Error("服务端返回的 W07 正式任务与队列身份不一致")
    }
    if (detail.status === "APPROVED" || detail.status === "REJECTED") {
        return null
    }

    const sales = await fetchSalesOrderDetail(detail.sales_order_id)
    const submission = sales.submissions?.find(
        (row) => row.id === detail.submission_id,
    )
    if (!submission) {
        throw new Error(
            `采购确认 ${detail.id} 未取得其绑定的不可变销售提交 ${detail.submission_id}；已禁止使用其它提交继续处理`,
        )
    }

    const confLines = mapConfirmationLines(detail.lines ?? [])

    const submissionLines = submission.lines.map((line) => ({
        submissionLineId: line.id,
        itemName: line.item_name_snapshot ?? `行 ${line.line_no}`,
        itemSku: line.sku_id ?? "",
        specification: line.spec_snapshot ?? undefined,
        committedQuantity: String(line.quantity ?? "0"),
        unit: line.base_unit_code ?? line.unit_snapshot ?? "",
        requestedDeliveryDate:
            secsToIso(line.fulfillment_due_at).slice(0, 10) || "—",
        unitPriceGross: line.unit_price_gross ?? undefined,
        fulfillmentMode: line.fulfillment_mode
            ? mapFulfillmentMode(line.fulfillment_mode)
            : undefined,
        salesTaxRate: line.sales_tax_rate ?? undefined,
        salesAmountGross: String(line.gross_amount ?? "0"),
    }))

    const coverage = emptyCoverageFromLines(
        confLines,
        submissionLines.map((s) => ({
            id: s.submissionLineId,
            name: s.itemName,
            required: s.committedQuantity,
        })),
    )

    const origin: SubmissionOrigin = "INITIAL"

    return {
        workItemId: projectedWorkItem.workItemId,
        taskVersion: projectedWorkItem.taskVersion,
        responsibilityScope: scope,
        status: projectedWorkItem.status,
        assignmentMode: projectedWorkItem.assignmentMode,
        ownerUser: projectedWorkItem.ownerUser,
        priority: priorityToNumber(projectedWorkItem.priority),
        dueAt:
            secsToIso(projectedWorkItem.dueAt) ||
            secsToIso(projectedWorkItem.createdAt),
        impactSummary: projectedWorkItem.impactSummary,
        subjectVersion: projectedWorkItem.subjectVersion,
        subjectHash: detail.submission_id,
        salesSubmission: {
            salesOrderId: detail.sales_order_id,
            salesOrderNo: sales.order_no,
            submissionId: detail.submission_id,
            submissionNo: submission.submission_no,
            subjectHash: detail.submission_id,
            subjectHashSummary: (detail.submission_id ?? "").slice(0, 12),
            submittedAt:
                secsToIso(submission.submitted_at) ||
                secsToIso(detail.created_at),
            submittedByLabel:
                submission.submitted_by === sales.owner_user_id
                    ? (sales.owner_user_name ?? "销售提交人")
                    : "销售提交人",
            customerSnapshot: submission.customer_name,
            contractId: sales.contract_id ?? undefined,
            contractSnapshot: submission.contract_no ?? undefined,
            settlementPartySnapshot:
                submission.settlement_party_name ?? undefined,
            paymentTermLabel: submission.payment_term_name,
            projectName: submission.project_name ?? undefined,
            businessRemark: submission.business_remark ?? undefined,
            grossAmount: String(
                submission.gross_amount ??
                    sales.working_copy?.gross_amount ??
                    "0",
            ),
            netAmount: submission.net_amount,
            taxAmount: submission.tax_amount,
            origin,
            lines: submissionLines,
        },
        confirmation: {
            confirmationId: detail.id,
            status: "PENDING",
            editVersion: detail.version,
            lines: confLines,
        },
        decisionSummary: {
            coverageByLine: coverage.coverageByLine,
            estimatedPurchaseGross: coverage.estimatedPurchaseGross,
            estimatedMargin: undefined,
            marginDelta: undefined,
            blockingIssues: coverage.blockingIssues,
            warnings: coverage.warnings,
        },
        allowedActions: [
            ...projectedWorkItem.allowedActions.filter((action) =>
                ["START_PROCESSING", "RELEASE_TO_TEAM", "REASSIGN"].includes(
                    action,
                ),
            ),
            ...(detail.allowed_actions ?? []),
        ],
        actionBlockers: [
            ...(detail.action_blockers ?? []),
            ...projectedWorkItem.actionBlockers.map((message) => ({
                action: "PROCESS_TASK",
                code: "WORK_ITEM_ACTION_BLOCKED",
                message,
            })),
        ],
        riskLabel: projectedWorkItem.ownerUser ? "处理中" : "团队待处理",
        riskTone: projectedWorkItem.ownerUser ? "info" : "warning",
        riskDescription: projectedWorkItem.impactSummary,
    }
}

/**
 * 组装 W07 队列视图。
 * `listQueueContextId` 只能是同一组筛选上次列表响应里的服务端上下文。
 */
export async function fetchProcurementQueue(
    filters: QueueFilters,
    listQueueContextId?: string,
): Promise<ProcurementQueueView> {
    const { items: workItems, queueContextId: issuedQueueContextId } =
        await fetchWorkItemsForQueue(filters, listQueueContextId)

    const projected = (
        await Promise.all(workItems.map((wi) => projectTask(wi, filters.scope)))
    ).filter((t): t is ProcurementConfirmationTask => t != null)

    const tasks = projected

    const queueContextId =
        issuedQueueContextId ??
        (isServerIssuedQueueContextId(listQueueContextId)
            ? listQueueContextId
            : undefined) ??
        `queue:procurement-confirmation:${filters.scope}`

    let position = 0
    let current = tasks[0]
    if (filters.currentWorkItemId) {
        const idx = tasks.findIndex(
            (t) => t.workItemId === filters.currentWorkItemId,
        )
        if (idx >= 0) {
            position = idx
            current = tasks[idx]
        }
    }

    const emptyReason =
        tasks.length === 0
            ? projected.length === 0 && workItems.length === 0
                ? "NO_TASKS"
                : "FILTER_NO_RESULT"
            : undefined

    return {
        preferences: {
            autoNextDefault: true,
        },
        context: {
            queueContextId,
            position: tasks.length === 0 ? 0 : position + 1,
            total: tasks.length,
            currentWorkItemId: current?.workItemId,
            previousWorkItemId: tasks[position - 1]?.workItemId,
            nextWorkItemId: tasks[position + 1]?.workItemId,
            filterSummary: filterSummary(filters),
            queueContextUpdatedAt: new Date().toISOString(),
        },
        tasks,
        current,
        emptyReason,
    }
}
