/**
 * W27 API 供应商结算 · 真实 HTTP API
 * 路径：/admin/supplier-settlement-statements、items、differences
 */

import { apiGet, apiPost, type Page } from "@/lib/api"
import type { WorkItemAllowedAction } from "@/features/work-items"
import type {
    AppendEvidenceInput,
    CreateDraftInput,
    DifferenceType,
    FormalOutcome,
    RefreshDraftInput,
    ResolveDifferenceInput,
    ReviewDecisionInput,
    SettlementDetailView,
    SettlementListRow,
    SettlementListView,
    SettlementStatus,
    SettlementView,
    SubmitReviewInput,
} from "@/features/supplier-settlements/types"
import {
    DIFF_STATUS_LABEL,
    DIFF_TYPE_LABEL,
    RESOLUTION_TO_STATUS,
    STATUS_LABEL,
    STATUS_TONE,
    VIEW_LABEL,
} from "@/features/supplier-settlements/types"

// ---------------------------------------------------------------------------
// Backend wire types
// ---------------------------------------------------------------------------

type BackendStatement = {
    id: string
    statement_no: string
    supplier_id: string
    period_start: string
    period_end: string
    period_policy_id: string
    period_policy_version: string
    period_timezone: string
    external_bill_no?: string | null
    external_bill_version?: string | null
    erp_amount: string
    supplier_amount: string
    difference_amount: string
    status: string
    prepared_by: string
    reviewed_by?: string | null
    confirmed_at?: number | null
    payable_account_id?: string | null
    subject_hash?: string | null
    source_as_of?: number | null
    source_snapshot_at?: number | null
    source_snapshot_hash?: string | null
    refresh_cutoff_policy_id?: string | null
    refresh_cutoff_policy_version?: string | number | null
    version: number
    created_at: number
}

type BackendItem = {
    id: string
    statement_id: string
    supplier_fulfillment_order_id: string
    supplier_fulfillment_item_id: string
    quantity: string
    order_amount: string
    freight_amount: string
    service_fee_amount: string
    refund_amount: string
    erp_calculated_amount: string
    erp_calculated_net_amount: string
    erp_calculated_tax_amount: string
    supplier_billed_amount: string
    supplier_billed_net_amount: string
    supplier_billed_tax_amount: string
    created_at: number
}

type BackendDifference = {
    id: string
    statement_item_id: string
    difference_type: string
    difference_amount: string
    status: string
    resolution?: string | null
    resolved_by?: string | null
    resolved_at?: number | null
    version: number
    created_at: number
    evidence?: BackendDifferenceEvidence[]
}

type BackendDifferenceEvidence = {
    evidence_id: string
    evidence_reference_ids: string[]
    opinion_code?: string | null
    comment?: string | null
    provided_by: string
    provided_at: number
}

type BackendDetail = {
    statement: BackendStatement
    items: BackendItem[]
    differences: BackendDifference[]
    review_work_item?: BackendReviewWorkItem | null
    review_action_blockers?: BackendReviewActionBlocker[]
    allowed_actions?: string[]
    action_blockers?: BackendReviewActionBlocker[]
    processing_state?: string
    stats?: {
        item_count: number
        difference_count: number
        pending_difference_count: number
        evidenced_difference_count: number
        order_amount: string
        freight_amount: string
        service_fee_amount: string
        refund_amount: string
        erp_amount: string
        supplier_amount: string
        difference_amount: string
    }
}

type BackendSourceEvidence = {
    id: string
    request_id: string
    supplier_id: string
    period_start: string
    period_end: string
    period_policy_id: string
    period_policy_version: string
    timezone: string
    source_version: number
    external_bill_no: string
    external_bill_version: string
    source_as_of: number
    source_hash: string
    line_count: number
}

type BackendDraftCommandResult = {
    result_status: "CREATED" | "REFRESHED" | "UNCHANGED" | "REPLAYED"
    message: string
    request_id: string
    statement: BackendStatement
    item_count: number
    difference_count: number
}

type BackendEvidenceResult = {
    result_status: "RECORDED" | "REPLAYED"
    message: string
    request_id: string
    statement_id: string
    difference_id: string
    evidence: BackendDifferenceEvidence
}

type BackendStatementPage = Page<BackendStatement> & {
    stats: {
        pending_reconciliation_count: number
        has_difference_count: number
        pending_review_count: number
        confirmed_amount: string
    }
    processing_state: "READY" | "EMPTY"
}

type BackendReviewActionBlocker = {
    action: string
    code: string
    message: string
}

type BackendReviewWorkItem = {
    work_item_id: string
    work_item_type: "SUPPLIER_SETTLEMENT_REVIEW"
    task_version: string | number
    subject_version: string
    status: "OPEN" | "COMPLETED" | "CLOSED"
    assignment_mode: "DIRECT" | "POOL"
    owner_role: string
    owner_organization_id: string
    owner_user_id?: string | null
    allowed_actions: WorkItemAllowedAction[]
    action_blockers: BackendReviewActionBlocker[]
}

type BackendDifferenceDecisionResult = {
    result_status: "RESOLVED" | "UNKNOWN"
    message: string
    operation_id: string
    statement_id: string
    statement_lock_version: number
    difference: BackendDifference
}

type BackendReviewSubmissionResult = {
    result_status: "SUBMITTED" | "UNKNOWN"
    message: string
    operation_id: string
    statement: BackendStatement
    work_item_id?: string | null
}

type BackendReviewDecisionResult = {
    result_status: "CONFIRMED" | "REJECTED" | "UNKNOWN"
    message: string
    operation_id: string
    statement: BackendStatement
    work_item_id: string
    work_item_status: "COMPLETED"
    task_version: string | number
    payable_no?: string | null
    payable_account_id?: string | null
    cost_delta_gross?: string | null
}

export type ListQueryInput = {
    view: SettlementView
    supplierId?: string
    periodFrom?: string
    periodTo?: string
    status?: string
    differenceType?: DifferenceType
    q?: string
    page: number
    pageSize?: number
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function tsToIso(secs: number | null | undefined): string {
    if (secs == null || !Number.isFinite(Number(secs)) || Number(secs) <= 0)
        return ""
    return new Date(Number(secs) * 1000).toISOString()
}

function asStatus(raw: string): SettlementStatus {
    if (raw.toUpperCase() === "PENDING_RECONCILIATION") {
        return "PENDING_RECONCILE"
    }
    const u = raw.toUpperCase() as SettlementStatus
    const allowed: SettlementStatus[] = [
        "DRAFT",
        "PENDING_RECONCILE",
        "HAS_DIFFERENCE",
        "PENDING_REVIEW",
        "CONFIRMED",
        "VOIDED",
    ]
    return allowed.includes(u) ? u : "DRAFT"
}

function directionLabel(diff?: string): string | undefined {
    if (diff == null) return undefined
    const n = Number(diff)
    if (!Number.isFinite(n) || n === 0) return "无差异"
    if (n > 0) return "供应商账单高于 ERP"
    return "ERP 高于供应商账单"
}

function toListRow(s: BackendStatement): SettlementListRow {
    const status = asStatus(s.status)
    const allowed = ["OPEN_CENTER", "VIEW", "OPEN_PREVIEW"]
    if (status !== "CONFIRMED" && status !== "VOIDED") {
        allowed.push("RESOLVE_DIFFERENCE")
    }
    return {
        statementId: s.id,
        statementNo: s.statement_no,
        supplierId: s.supplier_id,
        supplierName: s.supplier_id,
        periodStart: s.period_start,
        periodEnd: s.period_end,
        periodLabel: s.period_start.slice(0, 7),
        status,
        statusLabel: STATUS_LABEL[status],
        statusTone: STATUS_TONE[status],
        erpAmountGross: String(s.erp_amount),
        supplierAmountGross: String(s.supplier_amount),
        differenceAmountGross: String(s.difference_amount),
        differenceDirectionLabel: directionLabel(String(s.difference_amount)),
        unresolvedDifferenceCount: 0,
        preparedBy: s.prepared_by
            ? { userId: s.prepared_by, displayName: s.prepared_by }
            : undefined,
        reviewedBy: s.reviewed_by
            ? { userId: s.reviewed_by, displayName: s.reviewed_by }
            : undefined,
        preparedByLabel: s.prepared_by || "—",
        reviewedByLabel: s.reviewed_by || "待复核人",
        updatedAt: tsToIso(s.created_at),
        allowedActions: allowed,
        actionBlockers: [],
    }
}

function mapFormalReviewTask(item: BackendReviewWorkItem, statementId: string) {
    return {
        workItemId: item.work_item_id,
        taskVersion: String(item.task_version),
        workItemType: "SUPPLIER_SETTLEMENT_REVIEW" as const,
        businessObjectType: "SUPPLIER_SETTLEMENT_STATEMENT" as const,
        businessObjectId: statementId,
        subjectVersion: item.subject_version,
        assignmentMode: item.assignment_mode,
        processingState: "READY" as const,
        ownerUser: item.owner_user_id
            ? {
                  id: item.owner_user_id,
                  displayName: item.owner_user_id,
              }
            : undefined,
        status: item.status,
        allowedTaskActions: item.allowed_actions,
        actionBlockers: item.action_blockers.map((blocker) => blocker.message),
    }
}

function toDetail(
    d: BackendDetail,
    formalTask?: ReturnType<typeof mapFormalReviewTask>,
    workItemBlocker?: SettlementDetailView["workItemBlocker"],
): SettlementDetailView {
    const s = d.statement
    const status = asStatus(s.status)
    const diffs = (d.differences ?? []).map((diff) => {
        const rawStatus = diff.status?.toUpperCase() || "PENDING"
        const diffStatus = (
            rawStatus === "SUPPLIER_ACKNOWLEDGED"
                ? "SUPPLIER_ACCEPTED"
                : rawStatus === "ERP_ACKNOWLEDGED"
                  ? "ERP_ACCEPTED"
                  : rawStatus
        ) as SettlementDetailView["differences"][number]["status"]
        const rawType = diff.difference_type?.toUpperCase() || "AMOUNT"
        const type = (
            rawType === "MISSING" ? "MISSING_ORDER" : rawType
        ) as DifferenceType
        return {
            differenceId: diff.id,
            type: DIFF_TYPE_LABEL[type] ? type : ("AMOUNT" as DifferenceType),
            typeLabel: DIFF_TYPE_LABEL[type] ?? diff.difference_type,
            status: DIFF_STATUS_LABEL[diffStatus] ? diffStatus : "PENDING",
            statusLabel: DIFF_STATUS_LABEL[diffStatus] ?? diff.status,
            statusTone:
                diffStatus === "PENDING"
                    ? ("warning" as const)
                    : diffStatus === "CLOSED"
                      ? ("success" as const)
                      : ("info" as const),
            blocking: diffStatus === "PENDING",
            erpSideLabel: "ERP 试算",
            supplierSideLabel: "供应商账单",
            amountDirectionLabel:
                directionLabel(String(diff.difference_amount)) ?? "—",
            amountGross: String(diff.difference_amount),
            version: diff.version,
            evidence: (diff.evidence ?? []).map((evidence) => ({
                evidenceId: evidence.evidence_id,
                referenceIds: evidence.evidence_reference_ids,
                kind: "TICKET" as const,
                label:
                    evidence.opinion_code ??
                    evidence.evidence_reference_ids.join("、"),
                comment: evidence.comment ?? undefined,
                by: {
                    userId: evidence.provided_by,
                    displayName: evidence.provided_by,
                },
                at: tsToIso(evidence.provided_at),
            })),
            requiresProcurementEvidence: false,
            leftFields: [],
        }
    })

    const open = diffs.filter((x) => x.status === "PENDING").length
    const blocking = diffs.filter((x) => x.blocking).length
    const resolved = diffs.length - open
    const now = new Date().toISOString()
    const allowed = ["OPEN_CENTER", "VIEW", ...(d.allowed_actions ?? [])]
    const canPrepareReview =
        status === "DRAFT" ||
        status === "PENDING_RECONCILE" ||
        status === "HAS_DIFFERENCE"
    const actionBlockers: SettlementDetailView["actionBlockers"] = [
        ...(d.action_blockers ?? []),
    ]
    const reviewSubmissionPolicy =
        s.refresh_cutoff_policy_id && s.refresh_cutoff_policy_version != null
            ? {
                  refreshCutoffPolicyId: s.refresh_cutoff_policy_id,
                  version: String(s.refresh_cutoff_policy_version),
              }
            : undefined
    if (
        canPrepareReview &&
        s.subject_hash &&
        s.source_snapshot_hash &&
        reviewSubmissionPolicy
    ) {
        if (!d.allowed_actions) allowed.push("SUBMIT_REVIEW")
    } else if (canPrepareReview && !d.action_blockers) {
        actionBlockers.push({
            action: "SUBMIT_REVIEW",
            code: "REVIEW_SUBMISSION_CONTRACT_UNAVAILABLE",
            message:
                "复核所需的数据版本、来源依据或截止规则不完整，请刷新后重试。",
        })
    }
    if (status === "PENDING_REVIEW" && formalTask) {
        for (const message of formalTask.actionBlockers) {
            actionBlockers.push({
                action: "REVIEW_DECISION",
                code: "WORK_ITEM_ACTION_BLOCKED",
                message,
            })
        }
    } else if (status === "PENDING_REVIEW") {
        actionBlockers.push(
            workItemBlocker ?? {
                action: "REVIEW_DECISION",
                code: "FORMAL_REVIEW_WORK_ITEM_MISSING",
                message:
                    "未查询到与当前结算单及 W27 路由完全匹配的正式复核任务；禁止按对象状态直接确认或驳回。",
            },
        )
    }

    return {
        statement: {
            id: s.id,
            statementNo: s.statement_no,
            supplierId: s.supplier_id,
            supplierName: s.supplier_id,
            periodStart: s.period_start,
            periodEnd: s.period_end,
            periodLabel: s.period_start.slice(0, 7),
            externalBillNo: s.external_bill_no ?? undefined,
            externalBillVersion: s.external_bill_version ?? undefined,
            erpAmountGross: String(s.erp_amount),
            supplierAmountGross: String(s.supplier_amount),
            differenceAmountGross: String(s.difference_amount),
            differenceDirectionLabel: directionLabel(
                String(s.difference_amount),
            ),
            status,
            statusLabel: STATUS_LABEL[status],
            statusTone: STATUS_TONE[status],
            preparedBy: s.prepared_by
                ? { userId: s.prepared_by, displayName: s.prepared_by }
                : undefined,
            reviewedBy: s.reviewed_by
                ? { userId: s.reviewed_by, displayName: s.reviewed_by }
                : undefined,
            lockVersion: s.version,
            subjectHash: s.subject_hash ?? undefined,
            sourceAsOf: tsToIso(s.source_as_of ?? s.created_at),
            sourceSnapshotAt: tsToIso(s.source_snapshot_at ?? s.created_at),
            sourceSnapshotHash: s.source_snapshot_hash ?? undefined,
        },
        totals: {
            // 分项与总额一律取服务端同水位汇总；前端不汇总当前明细页。
            orderAmountGross: String(d.stats?.order_amount ?? s.erp_amount),
            freightGross: String(d.stats?.freight_amount ?? "0.00"),
            serviceFeeGross: String(d.stats?.service_fee_amount ?? "0.00"),
            refundGross: String(d.stats?.refund_amount ?? "0.00"),
            erpAmountGross: String(d.stats?.erp_amount ?? s.erp_amount),
            supplierAmountGross: String(
                d.stats?.supplier_amount ?? s.supplier_amount,
            ),
            differenceAmountGross: String(
                d.stats?.difference_amount ?? s.difference_amount,
            ),
            differenceDirectionLabel: directionLabel(
                String(s.difference_amount),
            ),
            taxBasisLabel: "含税",
        },
        items: (d.items ?? []).map((it) => ({
            itemId: it.id,
            supplierOrderNo: it.supplier_fulfillment_order_id,
            externalOrderNo: it.supplier_fulfillment_order_id,
            productName: it.supplier_fulfillment_item_id,
            quantity: String(it.quantity),
            factLabel: "履约结算",
            orderAmountGross: String(it.order_amount),
            freightGross: String(it.freight_amount),
            serviceFeeGross: String(it.service_fee_amount),
            refundGross: String(it.refund_amount),
            erpAmountGross: String(it.erp_calculated_amount),
            erpAmountNet: String(it.erp_calculated_net_amount),
            erpTaxAmount: String(it.erp_calculated_tax_amount),
            supplierBillLineGross: String(it.supplier_billed_amount),
            supplierBillLineNet: String(it.supplier_billed_net_amount),
            supplierBillLineTax: String(it.supplier_billed_tax_amount),
            readOnly: true as const,
        })),
        differences: diffs,
        differenceSummary: {
            total: diffs.length,
            open,
            blocking,
            resolved,
        },
        reviewRecords: [],
        payable: s.payable_account_id
            ? {
                  payableAccountId: s.payable_account_id,
                  payableNo: s.payable_account_id,
                  grossAmount: String(s.erp_amount),
                  dueDate: "",
                  statusLabel: "已生成",
                  w12Href: `/finance/supplier-accounts?view=payable&q=${encodeURIComponent(s.payable_account_id)}`,
              }
            : undefined,
        workItem: formalTask,
        workItemBlocker,
        reviewSubmissionPolicy,
        auditEvents: [],
        allowedActions: allowed,
        actionBlockers,
        freshness: {
            immutableFactsAsOf: tsToIso(s.created_at),
            queriedAt: now,
        },
        canEditBillOrOrder: false,
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

export async function fetchSettlementList(
    input: ListQueryInput,
): Promise<SettlementListView> {
    const queriedAt = new Date().toISOString()
    const pageSize = input.pageSize ?? 50
    // Map view → status filter when possible
    let statusFilter = input.status
    if (!statusFilter) {
        if (input.view === "confirmed") statusFilter = "CONFIRMED"
        else if (input.view === "pending") statusFilter = undefined
    }

    const pageRes = await apiGet<BackendStatementPage>(
        "/admin/supplier-settlement-statements",
        {
            page: input.page,
            page_size: pageSize,
            supplier_id: input.supplierId,
            status: statusFilter?.split(",")[0]?.trim() || undefined,
            period_from: input.periodFrom,
            period_to: input.periodTo,
            statement_no: input.q?.trim() || undefined,
            sort_by: "period_end",
            sort_dir: "asc",
        },
    )

    let statements = pageRes.items ?? []

    // Client-side view filters not supported by backend
    if (input.view === "pending") {
        statements = statements.filter((s) => {
            const st = asStatus(s.status)
            return (
                st === "DRAFT" ||
                st === "PENDING_RECONCILE" ||
                st === "HAS_DIFFERENCE" ||
                st === "PENDING_REVIEW"
            )
        })
    }

    const rows = statements.map(toListRow)
    const total = pageRes.total ?? rows.length
    const suppliersMap = new Map<string, string>()
    for (const s of statements) suppliersMap.set(s.supplier_id, s.supplier_id)

    const filterParts = [
        input.view !== "pending" ? `视图=${VIEW_LABEL[input.view]}` : null,
        input.supplierId ? `供应商=${input.supplierId}` : null,
        input.periodFrom || input.periodTo
            ? `期间=${input.periodFrom ?? "…"} ~ ${input.periodTo ?? "…"}`
            : null,
        input.q ? `搜索=${input.q}` : null,
    ].filter(Boolean)

    return {
        view: input.view,
        rows,
        page: pageRes.page ?? input.page,
        pageSize: pageRes.page_size ?? pageSize,
        total,
        totals: {
            pendingReconcile: pageRes.stats.pending_reconciliation_count,
            hasDifference: pageRes.stats.has_difference_count,
            pendingReview: pageRes.stats.pending_review_count,
            confirmedAmountThisPeriod: String(pageRes.stats.confirmed_amount),
        },
        metrics: {
            pending: pageRes.stats.pending_reconciliation_count,
            hasDifference: pageRes.stats.has_difference_count,
            pendingReview: pageRes.stats.pending_review_count,
            confirmedAmount: String(pageRes.stats.confirmed_amount),
        },
        suppliers: Array.from(suppliersMap.entries()).map(
            ([supplierId, supplierName]) => ({ supplierId, supplierName }),
        ),
        emptyReason: total === 0 ? "NO_STATEMENTS" : undefined,
        hasModulePermission: true,
        hasDataScope: true,
        permissionVersion: "server",
        sourceAsOf: queriedAt,
        queriedAt,
        filterSummary: filterParts.length
            ? filterParts.join(" · ")
            : "默认待处理视图",
    }
}

export async function fetchSettlementDetail(input: {
    statementId: string
    workItemId?: string
}): Promise<SettlementDetailView> {
    const detail = await apiGet<BackendDetail>(
        `/admin/supplier-settlement-statements/${encodeURIComponent(input.statementId)}`,
    )
    const embeddedTask = detail.review_work_item
    const formalTask =
        embeddedTask &&
        (!input.workItemId || embeddedTask.work_item_id === input.workItemId)
            ? mapFormalReviewTask(embeddedTask, input.statementId)
            : undefined
    const workItemBlocker =
        input.workItemId && embeddedTask?.work_item_id !== input.workItemId
            ? {
                  action: "REVIEW_DECISION",
                  code: "FORMAL_REVIEW_WORK_ITEM_NOT_ACCESSIBLE",
                  message:
                      "指定任务与当前结算单或 W27 路由不匹配，禁止降级为对象直接确认。",
              }
            : detail.review_action_blockers?.[0]
    return toDetail(detail, formalTask, workItemBlocker)
}

export async function createSettlementDraft(
    input: CreateDraftInput,
): Promise<FormalOutcome> {
    try {
        const source = await apiGet<BackendSourceEvidence>(
            "/admin/supplier-settlement-source-evidence",
            {
                supplier_id: input.supplierId,
                period_start: input.periodStart,
                period_end: input.periodEnd,
            },
        )
        const result = await apiPost<BackendDraftCommandResult>(
            "/admin/supplier-settlement-statements",
            {
                action: "CREATE",
                supplier_id: input.supplierId,
                period_start: input.periodStart,
                period_end: input.periodEnd,
                period_policy_id: source.period_policy_id,
                expected_period_policy_version: source.period_policy_version,
                request_id: input.requestId,
                idempotency_key: input.idempotencyKey,
            },
        )
        const created = result.statement
        return {
            status: "succeeded",
            title: "结算草稿已创建",
            message: result.message,
            reference: created.statement_no,
            statementId: created.id,
            lockVersion: created.version,
            facts: [
                { label: "结算单号", value: created.statement_no },
                { label: "供应商", value: created.supplier_id },
                {
                    label: "期间",
                    value: `${input.periodStart} ~ ${input.periodEnd}`,
                },
                { label: "来源行数", value: String(result.item_count) },
            ],
        }
    } catch (err) {
        const message =
            err && typeof err === "object" && "message" in err
                ? String((err as { message: string }).message)
                : "创建草稿失败"
        if (
            message.includes("SOURCE_EVIDENCE_MISSING") ||
            message.includes("来源证据")
        ) {
            return {
                status: "blocked",
                code: "SOURCE_EVIDENCE_MISSING",
                title: "来源证据尚未完备",
                message:
                    "当前供应商与期间尚无完整来源证据批次，请先通过来源证据录入命令补齐履约、退款、费用与账单行证据。",
            }
        }
        throw err
    }
}

export async function refreshSettlementTrial(
    input: RefreshDraftInput,
): Promise<FormalOutcome> {
    const result = await apiPost<BackendDraftCommandResult>(
        `/admin/supplier-settlement-statements/${encodeURIComponent(input.statementId)}/refreshes`,
        {
            action: "REFRESH",
            statement_id: input.statementId,
            expected_lock_version: input.expectedLockVersion,
            expected_source_snapshot_hash: input.expectedSourceSnapshotHash,
            request_id: input.requestId,
            idempotency_key: input.idempotencyKey,
        },
    )
    return {
        status: "succeeded",
        title:
            result.result_status === "UNCHANGED"
                ? "试算已是最新"
                : "试算已刷新",
        message: result.message,
        reference: result.statement.statement_no,
        statementId: result.statement.id,
        lockVersion: result.statement.version,
        sourceSnapshotHash: result.statement.source_snapshot_hash ?? undefined,
        subjectHash: result.statement.subject_hash ?? undefined,
    }
}

export async function appendDifferenceEvidence(
    input: AppendEvidenceInput,
): Promise<FormalOutcome> {
    const result = await apiPost<BackendEvidenceResult>(
        `/admin/supplier-settlement-differences/${encodeURIComponent(input.differenceId)}/evidence`,
        {
            statement_id: input.statementId,
            difference_id: input.differenceId,
            expected_difference_version: input.expectedDifferenceVersion,
            evidence_reference_ids: input.evidenceReferenceIds,
            opinion_code: input.opinionCode,
            comment: input.comment,
            request_id: input.requestId,
            idempotency_key: input.idempotencyKey,
        },
    )
    return {
        status: "succeeded",
        title: "差异证据已登记",
        message: result.message,
        reference: result.evidence.evidence_id,
        statementId: result.statement_id,
    }
}

export async function resolveDifference(
    input: ResolveDifferenceInput,
): Promise<FormalOutcome> {
    const result = await apiPost<BackendDifferenceDecisionResult>(
        `/admin/supplier-settlement-differences/${encodeURIComponent(input.differenceId)}/decisions`,
        {
            statement_id: input.statementId,
            difference_id: input.differenceId,
            expected_lock_version: input.expectedLockVersion,
            expected_difference_version: input.expectedDifferenceVersion,
            resolution: input.resolution,
            reason_code: input.reasonCode,
            evidence_reference_ids: input.evidenceReferenceIds,
            operation_id: input.operationId,
            idempotency_key: input.idempotencyKey,
        },
    )

    return {
        status: result.result_status === "RESOLVED" ? "succeeded" : "unknown",
        title:
            result.result_status === "RESOLVED"
                ? "差异结论已登记"
                : "差异处理结果待确认",
        message: result.message,
        reference: result.operation_id,
        operationId: result.operation_id,
        statementId: result.statement_id,
        lockVersion: result.statement_lock_version,
        facts: [
            {
                label: "结论",
                value: RESOLUTION_TO_STATUS[input.resolution],
            },
            { label: "原因", value: input.reasonCode },
        ],
    }
}

export async function submitSettlementReview(
    input: SubmitReviewInput,
): Promise<FormalOutcome> {
    const result = await apiPost<BackendReviewSubmissionResult>(
        `/admin/supplier-settlement-statements/${encodeURIComponent(input.statementId)}/review-submissions`,
        {
            action: "SUBMIT_REVIEW",
            statement_id: input.statementId,
            expected_lock_version: input.expectedLockVersion,
            subject_hash: input.subjectHash,
            refresh_cutoff_policy_id: input.refreshCutoffPolicyId,
            expected_refresh_cutoff_policy_version:
                input.expectedRefreshCutoffPolicyVersion,
            operation_id: input.operationId,
            idempotency_key: input.idempotencyKey,
            comment: input.comment,
        },
    )
    return {
        status: result.result_status === "SUBMITTED" ? "succeeded" : "unknown",
        title:
            result.result_status === "SUBMITTED"
                ? "已提交复核"
                : "提交复核结果待确认",
        message: result.message,
        reference: result.work_item_id ?? result.operation_id,
        operationId: result.operation_id,
        statementId: result.statement.id,
        lockVersion: result.statement.version,
    }
}

export async function decideSettlementReview(
    input: ReviewDecisionInput,
): Promise<FormalOutcome> {
    const result = await apiPost<BackendReviewDecisionResult>(
        `/admin/supplier-settlement-statements/${encodeURIComponent(input.statementId)}/review-decisions`,
        {
            work_item_id: input.workItemId,
            expected_task_version: input.expectedTaskVersion,
            expected_subject_version: input.expectedSubjectVersion,
            decision: {
                statement_id: input.statementId,
                expected_lock_version: input.expectedLockVersion,
                action: input.action,
                operation_id: input.operationId,
                reason_code: input.reasonCode,
                comment: input.comment,
            },
            idempotency_key: input.idempotencyKey,
        },
    )
    return {
        status:
            result.result_status === "UNKNOWN"
                ? "unknown"
                : result.result_status === "REJECTED"
                  ? "rejected"
                  : "succeeded",
        title:
            result.result_status === "UNKNOWN"
                ? "复核结果待确认"
                : result.result_status === "REJECTED"
                  ? "结算已驳回"
                  : "结算已确认",
        message: result.message,
        reference: result.payable_no ?? result.operation_id,
        operationId: result.operation_id,
        statementId: result.statement.id,
        payableNo: result.payable_no ?? undefined,
        payableAccountId: result.payable_account_id ?? undefined,
        costDeltaGross: result.cost_delta_gross ?? undefined,
        lockVersion: result.statement.version,
    }
}
