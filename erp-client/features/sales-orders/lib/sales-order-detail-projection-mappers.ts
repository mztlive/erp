import type {
    BackendActiveCardSalesApproval,
    BackendActiveLowMarginManagerConfirmation,
    BackendOpenProcurementRejection,
    BackendProcurementConfirmation,
    BackendSalesChangeOrder,
    BackendSalesOrderDetail,
    BackendWorkingCopyLine,
} from "@/features/sales-orders/api/contracts"
import type {
    ActiveLowMarginManagerConfirmation,
    CardSalesApproval,
    ProcurementRejectionResolution,
    SalesChangeOrderSummary,
    SalesOrderListItem,
    SalesOrderNature,
} from "@/features/sales-orders/types"
import { personDisplayName } from "@/features/sales-orders/lib/labels"
import {
    formatEpochDate,
    formatInstant,
    mapListItemFromBackend,
    mapRevisions,
    mapWorkingCopyLines,
    toneForStatus,
} from "@/features/sales-orders/lib/sales-order-detail-mappers"

/**
 * 详情商业内容优先：可编辑草稿 → 最新提交快照。
 * 提交后 working_copy 为空时必须从 submission 回填明细与表头。
 */
function pickCommercialContent(detail: BackendSalesOrderDetail): {
    lines: BackendWorkingCopyLine[]
    amountGross?: string
    amountNet?: string
    taxAmount?: string
    ownerUserId: string
    customerName?: string
    contractNo?: string
    settlementPartyName?: string
    paymentTerms: string
    welfareScene: string
    fulfillmentDeadline: string
    remark?: string
} {
    const wc = detail.working_copy
    const submissions = [...(detail.submissions ?? [])].sort(
        (a, b) => (b.submission_no ?? 0) - (a.submission_no ?? 0),
    )
    const latestSubmission = submissions[0]

    if (wc?.lines?.length) {
        return {
            lines: wc.lines,
            amountGross: wc.gross_amount,
            amountNet: wc.net_amount,
            taxAmount: wc.tax_amount,
            ownerUserId:
                wc.editor_user_id || latestSubmission?.submitted_by || "",
            customerName: wc.customer_name || undefined,
            contractNo: wc.contract_no || undefined,
            settlementPartyName: wc.settlement_party_name || undefined,
            paymentTerms: wc.payment_term_name || wc.payment_term_code || "",
            welfareScene: wc.project_name || "",
            fulfillmentDeadline: formatEpochDate(wc.voucher_expiry_at),
            remark: wc.business_remark || undefined,
        }
    }

    if (latestSubmission) {
        return {
            lines: latestSubmission.lines ?? [],
            amountGross: latestSubmission.gross_amount,
            amountNet: latestSubmission.net_amount,
            taxAmount: latestSubmission.tax_amount,
            ownerUserId: latestSubmission.submitted_by || "",
            customerName: latestSubmission.customer_name || undefined,
            contractNo: latestSubmission.contract_no || undefined,
            settlementPartyName:
                latestSubmission.settlement_party_name || undefined,
            paymentTerms:
                latestSubmission.payment_term_name ||
                latestSubmission.payment_term_code ||
                "",
            welfareScene: latestSubmission.project_name || "",
            fulfillmentDeadline: formatEpochDate(
                latestSubmission.voucher_expiry_at,
            ),
            remark: latestSubmission.business_remark || undefined,
        }
    }

    return {
        lines: [],
        ownerUserId: "",
        paymentTerms: "",
        welfareScene: "",
        fulfillmentDeadline: "",
    }
}

export function mapDetailToListItem(
    detail: BackendSalesOrderDetail,
    extras?: {
        customerName?: string
        contractNumber?: string
        ownerName?: string
        procurementRejection?: ProcurementRejectionResolution | null
        activeCardSalesApproval?: CardSalesApproval | null
        activeChangeOrder?: SalesChangeOrderSummary | null
        customerContact?: string
    },
): SalesOrderListItem {
    const commercial = pickCommercialContent(detail)
    const openRejectionSubmissionNo = detail.submissions.find(
        (s) => s.id === detail.open_procurement_rejection?.submission_id,
    )?.submission_no
    const procurementRejection =
        extras?.procurementRejection ??
        mapOpenProcurementRejection(
            detail.open_procurement_rejection,
            openRejectionSubmissionNo,
        )
    const activeCardSalesApproval =
        extras?.activeCardSalesApproval ??
        mapActiveCardSalesApproval(detail.active_card_sales_approval)
    const activeLowMarginManagerConfirmation =
        mapActiveLowMarginManagerConfirmation(
            detail.active_low_margin_manager_confirmation,
        )
    const approvalProjectionInvalid = Boolean(
        detail.active_card_sales_approval && !activeCardSalesApproval,
    )
    return mapListItemFromBackend(
        {
            id: detail.id,
            order_no: detail.order_no,
            business_type: detail.business_type,
            origin_system: detail.origin_system,
            customer_id: detail.customer_id,
            contract_id: detail.contract_id,
            commercial_status: detail.commercial_status,
            review_status: detail.review_status,
            fulfillment_progress: detail.fulfillment_progress,
            collection_progress: detail.collection_progress,
            invoice_progress: detail.invoice_progress,
            close_status: detail.close_status,
            effective_at: detail.effective_at,
            version: detail.version,
            created_at: detail.created_at,
            updated_at: detail.created_at,
            stage: detail.stage,
        },
        {
            customerName:
                extras?.customerName ||
                commercial.customerName ||
                detail.customer_id,
            contractNumber:
                extras?.contractNumber || commercial.contractNo || "",
            contractCompanyName:
                extras?.customerName || commercial.customerName || "",
            amountGross: commercial.amountGross,
            amountNet: commercial.amountNet,
            taxAmount: commercial.taxAmount,
            lineItems: mapWorkingCopyLines(commercial.lines),
            ownerName: extras?.ownerName || "",
            customerContact: extras?.customerContact,
            paymentTerms: commercial.paymentTerms,
            welfareScene: commercial.welfareScene,
            fulfillmentDeadline: commercial.fulfillmentDeadline,
            remark: commercial.remark,
            revisions: mapRevisions(detail.revisions),
            procurementRejection,
            activeCardSalesApproval,
            activeLowMarginManagerConfirmation,
            cardApprovalProjectionBlocker: approvalProjectionInvalid
                ? "当前审批进度缺少实例、步骤、任务或业务版本；为避免错批，本页仅供查看。"
                : detail.business_type === "VOUCHER" &&
                    (detail.review_status === "PENDING_SALES_LEADER" ||
                        detail.review_status === "PENDING_OPERATIONS") &&
                    !detail.active_card_sales_approval
                  ? "审批仍在进行，但审批进度和任务版本不完整；为避免错批，本页仅供查看。"
                  : null,
            activeChangeOrder: extras?.activeChangeOrder,
            settlementEntity:
                commercial.settlementPartyName || detail.settlement_party_id,
            closeEligibility: detail.close_eligibility,
            startSalesChange: {
                allowed: detail.can_start_sales_change_order,
                blocker: detail.change_order_blocker,
            },
        },
    )
}

export function mapActiveCardSalesApproval(
    approval?: BackendActiveCardSalesApproval | null,
): CardSalesApproval | null {
    if (!approval) return null
    const hasWorkItem = Boolean(
        approval.work_item_id &&
        approval.task_version != null &&
        approval.work_item_type &&
        approval.work_item_status &&
        approval.assignment_mode,
    )
    if (approval.processing_state === "READY" && !hasWorkItem) {
        return null
    }
    const common = {
        approvalInstanceId: approval.approval_instance_id,
        instanceVersion: String(approval.instance_version),
        approvalStepInstanceId: approval.approval_step_instance_id,
        stepVersion: String(approval.step_version),
        processingBlocker: approval.processing_blocker ?? undefined,
        subjectVersion: approval.subject_version,
        salesOrderSubmissionId: approval.sales_order_submission_id,
        submissionNo: approval.submission_no,
        ownerUser: approval.owner_user
            ? {
                  id: approval.owner_user.id,
                  displayName: approval.owner_user.display_name,
              }
            : undefined,
        frozenSubmissionSummary: approval.frozen_submission_summary,
        expectedReviewStatus: approval.expected_review_status,
        actionBlockers: approval.action_blockers.map((blocker) => ({
            action: blocker.action,
            reason:
                blocker.message ??
                blocker.reason ??
                blocker.code ??
                "当前不可执行",
        })),
    }
    if (approval.processing_state === "APPROVAL_BLOCKED") {
        return {
            ...common,
            processingState: "APPROVAL_BLOCKED",
            workItemId: approval.work_item_id ?? undefined,
            workItemType: approval.work_item_type ?? undefined,
            taskVersion:
                approval.task_version == null
                    ? undefined
                    : String(approval.task_version),
            workItemStatus: approval.work_item_status ?? undefined,
            assignmentMode: approval.assignment_mode ?? undefined,
            allowedActions: approval.allowed_actions.filter(
                (action): action is "CANCEL" => action === "CANCEL",
            ),
        }
    }
    return {
        ...common,
        processingState: "READY",
        workItemId: approval.work_item_id!,
        workItemType: approval.work_item_type!,
        taskVersion: String(approval.task_version!),
        workItemStatus: approval.work_item_status!,
        assignmentMode: approval.assignment_mode!,
        allowedActions: approval.allowed_actions,
    }
}

/** 将服务端 actor-specific 低毛利确认投影原样收敛为页面工作面。 */
export function mapActiveLowMarginManagerConfirmation(
    confirmation?: BackendActiveLowMarginManagerConfirmation | null,
): ActiveLowMarginManagerConfirmation | null {
    if (!confirmation) return null
    return {
        confirmationId: confirmation.confirmation_id,
        workItemId: confirmation.work_item_id,
        taskVersion: String(confirmation.task_version),
        subjectVersion: confirmation.subject_version,
        lowMarginSubmissionId: confirmation.low_margin_submission_id,
        rejectedProcurementConfirmationId:
            confirmation.rejected_procurement_confirmation_id,
        acceptanceReason: confirmation.acceptance_reason,
        evidenceReferenceIds: confirmation.evidence_reference_ids,
        ownerUser: confirmation.owner_user
            ? {
                  id: confirmation.owner_user.id,
                  displayName: confirmation.owner_user.display_name,
              }
            : undefined,
        allowedActions: confirmation.allowed_actions,
        actionBlockers: confirmation.action_blockers,
    }
}

export function mapRejectedProcurement(
    conf: BackendProcurementConfirmation,
): ProcurementRejectionResolution | null {
    if (conf.status !== "REJECTED") return null
    return {
        rejectedProcurementConfirmationId: conf.id,
        rejectedProcurementWorkItemId: "",
        rejectedSubmissionId: conf.submission_id,
        rejectedSubmissionNo: 0,
        rejectedSubjectHash: conf.submission_id,
        rejectReasonCode: "",
        rejectComment: "",
        rejectedByLabel: personDisplayName(conf.handled_by),
        rejectedAt: formatInstant(conf.handled_at),
        reviewStatus: "REJECTED",
        draftDifference: {
            changedItemOrService: false,
            changedSalesPrice: false,
            commercialTermsUnchanged: true,
            diffSummary: [],
        },
        fixedResolutions: [
            "RESUBMIT_CHANGED_TERMS",
            "REQUEST_LOW_MARGIN_ACCEPTANCE",
            "VOID_AFTER_REJECTION",
        ],
        allowedActions: [],
        actionBlockers: [],
    }
}

/**
 * 将销售单详情内嵌的开放采购驳回映射为前端处理卡契约。
 * 权威来源为 `GET /admin/sales-orders/{id}`，不依赖采购队列 list 权限。
 */
export function mapOpenProcurementRejection(
    open: BackendOpenProcurementRejection | null | undefined,
    submissionNo?: number,
): ProcurementRejectionResolution | null {
    if (!open) return null
    return {
        rejectedProcurementConfirmationId: open.procurement_confirmation_id,
        rejectedProcurementWorkItemId: "",
        rejectedSubmissionId: open.submission_id,
        rejectedSubmissionNo: submissionNo ?? 0,
        rejectedSubjectHash: open.submission_id,
        rejectReasonCode: open.reject_reason_code ?? "",
        rejectComment: open.comment ?? "",
        rejectedByLabel: personDisplayName(open.handled_by_name),
        rejectedAt: formatInstant(open.handled_at),
        reviewStatus: "REJECTED",
        draftDifference: {
            changedItemOrService: false,
            changedSalesPrice: false,
            commercialTermsUnchanged: true,
            diffSummary: [],
        },
        fixedResolutions: [
            "RESUBMIT_CHANGED_TERMS",
            "REQUEST_LOW_MARGIN_ACCEPTANCE",
            "VOID_AFTER_REJECTION",
        ],
        allowedActions: open.allowed_actions,
        actionBlockers: [],
    }
}

export function mapChangeOrder(
    row: BackendSalesChangeOrder,
    nature: SalesOrderNature,
): SalesChangeOrderSummary {
    const impactPath = nature === "card_voucher" ? "operations" : "procurement"
    const statusLabel =
        row.status === "PENDING_IMPACT_CONFIRMATION"
            ? impactPath === "operations"
                ? "待运营执行影响确认"
                : "待采购履约影响确认"
            : row.status === "PENDING_FINANCE_REVIEW"
              ? "待财务复核"
              : row.status === "DRAFT"
                ? "草稿"
                : row.status === "EFFECTIVE"
                  ? "已生效"
                  : row.status === "VOIDED"
                    ? "已作废"
                    : row.status
    return {
        id: row.id,
        statusLabel,
        statusTone: toneForStatus(statusLabel),
        baseRevisionNo: 0,
        createdAt: new Date(row.created_at * 1000).toISOString(),
        impactPath,
    }
}
