"use client"

import { DocumentApprovalArea } from "@/features/approval-workflow/components/document-approval-area"
import type {
    ApprovalCommandView,
    DocumentApprovalView,
} from "@/features/approval-workflow/types"
import { FinancialDraftEditButton } from "@/features/financial-draft-edit/components/financial-draft-edit-button"
import {
    financialDraftEditHref,
    type FinancialDraftKind,
} from "@/features/financial-draft-edit/types"

export type FinancialApprovalPhase = "draft" | "confirm" | "runtime"

/** 逆向财务单据共用审批区；回款单有专用读取，不走这里。 */
export type FinancialApprovalKind = Exclude<
    FinancialDraftKind,
    "customer_receipt"
>

/**
 * 逆向财务单据（退款、冲正）的审批区。
 *
 * 未提交展示绑定卡，提交确认展示固定路线，运行中/终态展示摘要与历史。
 * 动作入口只读 `allowed_actions` 与 `recovery_options`，不复制审批状态推导。
 */
export function FinancialApprovalArea({
    kind,
    documentType,
    phase,
    approval,
    documentId,
    documentVersion,
    workItemId,
    expectedTaskVersion,
    workItemAllowedActions,
    onDecisionApplied,
    surface,
    historyId,
    actionBarId,
}: {
    kind: FinancialApprovalKind
    documentType: string
    phase: FinancialApprovalPhase
    approval?: DocumentApprovalView
    documentId?: string
    documentVersion?: number
    workItemId?: string
    expectedTaskVersion?: string
    workItemAllowedActions?: readonly string[]
    onDecisionApplied?: (view: ApprovalCommandView) => void
    /** 审批查询反馈的稳定区块标识，用于定位失败来源。 */
    surface: string
    historyId?: string
    actionBarId?: string
}) {
    const editDocumentHref = documentId
        ? financialDraftEditHref(kind, documentId)
        : undefined
    const editButton = documentId ? (
        <FinancialDraftEditButton kind={kind} documentId={documentId} />
    ) : null

    return (
        <DocumentApprovalArea
            phase={phase}
            approval={approval}
            surface={surface}
            documentType={documentType}
            documentId={documentId}
            documentVersion={documentVersion}
            editDocumentHref={editDocumentHref}
            actionBarId={actionBarId}
            historyId={historyId}
            workItemId={workItemId}
            expectedTaskVersion={expectedTaskVersion}
            workItemAllowedActions={workItemAllowedActions}
            onDecisionApplied={onDecisionApplied}
            runtimeClassName="space-y-3"
            draftAddon={editButton}
            runtimeHistoryAddon={editButton}
        />
    )
}
