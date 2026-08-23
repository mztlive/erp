/**
 * W27 API 供应商结算 · wire → 视图 映射纯函数
 * 从 api/settlements.ts 拆出；映射结果形状与标签口径保持不变。
 */

import type {
    DifferenceType,
    SettlementDetailView,
    SettlementListRow,
    SettlementStatus,
} from "@/features/supplier-settlements/types"
import {
    DIFF_STATUS_LABEL,
    DIFF_TYPE_LABEL,
    STATUS_LABEL,
    STATUS_TONE,
} from "@/features/supplier-settlements/types"
import type {
    BackendDetail,
    BackendReviewWorkItem,
    BackendStatement,
} from "@/features/supplier-settlements/api/settlements-wire"

function tsToIso(secs: number | null | undefined): string {
    if (secs == null || !Number.isFinite(Number(secs)) || Number(secs) <= 0)
        return ""
    return new Date(Number(secs) * 1000).toISOString()
}

export function asStatus(raw: string): SettlementStatus {
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

export function toListRow(s: BackendStatement): SettlementListRow {
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

export function mapFormalReviewTask(
    item: BackendReviewWorkItem,
    statementId: string,
) {
    return {
        workItemId: item.work_item_id,
        taskVersion: String(item.task_version),
        workItemType: "SUPPLIER_SETTLEMENT_REVIEW" as const,
        businessObjectType: "SUPPLIER_SETTLEMENT_STATEMENT" as const,
        businessObjectId: statementId,
        subjectVersion: item.subject_version,
        processingState: "READY" as const,
        ownerUser: item.owner_user_id
            ? {
                  id: item.owner_user_id,
                  displayName: item.owner_user_id,
              }
            : undefined,
        status: item.status,
        actionBlockers: item.action_blockers.map((blocker) => blocker.message),
    }
}

export function toDetail(
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
