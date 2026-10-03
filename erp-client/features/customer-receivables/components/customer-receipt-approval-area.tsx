"use client"

import { financialDraftEditHref } from "@/features/financial-draft-edit/types"
import { FinancialDraftEditButton } from "@/features/financial-draft-edit/components/financial-draft-edit-button"
import { useFinancialDraftQuery } from "@/features/financial-draft-edit/hooks/queries"
import { useAccountProfileQuery } from "@/features/auth/hooks/queries"
import { hasPermission } from "@/lib/permissions"
import { ApprovalActionBar } from "@/features/approval-workflow/components/approval-action-bar"
import { DefinitionBindingCard } from "@/features/approval-workflow/components/definition-binding-card"
import { ExecutionHistory } from "@/features/approval-workflow/components/execution-history"
import { RuntimeSummary } from "@/features/approval-workflow/components/runtime-summary"
import { SubmissionRouteConfirmation } from "@/features/approval-workflow/components/submission-route-confirmation"
import {
    useApprovalHistoryInfiniteQuery,
    useRecoveryOptionsQuery,
} from "@/features/approval-workflow/queries"
import type {
    ApprovalCommandView,
    DocumentApprovalView,
} from "@/features/approval-workflow/types"
import { mapDocumentApprovalViewDto } from "@/features/approval-workflow/types"
import {
    CUSTOMER_RECEIPT_DOCUMENT_TYPE,
    mergeCustomerReceiptAllowedActions,
    customerReceiptApprovalPhase,
    type CustomerReceiptApprovalPhase,
} from "@/features/customer-receivables/lib/customer-receipt-approval"

/**
 * 客户回款单审批区。
 *
 * 未提交展示绑定卡，提交确认展示固定路线，运行中/终态展示摘要与历史。
 * 动作入口只读 `allowed_actions` 与 `recovery_options`，不复制审批状态推导。
 */
export function CustomerReceiptApprovalArea({
    phase: providedPhase,
    approval: providedApproval,
    documentId,
    documentVersion: providedDocumentVersion,
    workItemId,
    expectedTaskVersion,
    workItemAllowedActions,
    onDecisionApplied,
}: {
    phase: CustomerReceiptApprovalPhase
    approval?: DocumentApprovalView
    documentId?: string
    documentVersion?: number
    workItemId?: string
    expectedTaskVersion?: string
    workItemAllowedActions?: readonly string[]
    onDecisionApplied?: (view: ApprovalCommandView) => void
}) {
    const profileQuery = useAccountProfileQuery()
    const canReadEditable = hasPermission(
        profileQuery.data?.permissions,
        "customer_receipt:submit",
    )
    // 完整审批结构仅从通过原登记人与完整资金源资格的专用读取取得。
    const editableQuery = useFinancialDraftQuery(
        "customer_receipt",
        documentId ?? "",
        canReadEditable,
    )
    const qualifiedDraft =
        canReadEditable && !editableQuery.isError
            ? editableQuery.data
            : undefined
    const approval = qualifiedDraft?.approval
        ? mapDocumentApprovalViewDto(qualifiedDraft.approval)
        : providedApproval
    const documentVersion = qualifiedDraft?.version ?? providedDocumentVersion
    const phase =
        providedPhase === "confirm"
            ? providedPhase
            : qualifiedDraft
              ? customerReceiptApprovalPhase(approval, qualifiedDraft.status)
              : providedPhase
    const instanceId = approval?.instance?.id
    const recoveryQuery = useRecoveryOptionsQuery(
        instanceId,
        phase === "runtime" && Boolean(instanceId),
    )
    const historyQuery = useApprovalHistoryInfiniteQuery(
        { instanceId: instanceId ?? "" },
        phase === "runtime" && Boolean(instanceId),
    )
    const historyItems = historyQuery.data
        ? historyQuery.data.pages.flatMap((page) => page.items)
        : (approval?.recentHistory ?? [])
    const allowedActions = mergeCustomerReceiptAllowedActions(
        approval?.allowedActions,
        workItemAllowedActions,
    )

    if (phase === "draft") {
        return (
            <div className="space-y-3">
                <DefinitionBindingCard definition={approval?.definition} />
                {documentId ? (
                    <FinancialDraftEditButton
                        kind="customer_receipt"
                        documentId={documentId}
                    />
                ) : null}
                {documentId ? (
                    <ApprovalActionBar
                        id="customer-receivables-receipt-approval-action-bar"
                        allowedActions={allowedActions}
                        definition={approval?.definition}
                        documentType={CUSTOMER_RECEIPT_DOCUMENT_TYPE}
                        documentId={documentId}
                        documentVersion={documentVersion}
                        editDocumentHref={
                            documentId
                                ? financialDraftEditHref(
                                      "customer_receipt",
                                      documentId,
                                  )
                                : undefined
                        }
                    />
                ) : null}
            </div>
        )
    }

    if (phase === "confirm") {
        return <SubmissionRouteConfirmation definition={approval?.definition} />
    }

    return (
        <div className="space-y-3">
            <RuntimeSummary instance={approval?.instance} />
            {documentId ? (
                <FinancialDraftEditButton
                    kind="customer_receipt"
                    documentId={documentId}
                />
            ) : null}
            <ExecutionHistory
                id="customer-receivables-receipt-approval-history"
                items={historyItems}
                hasMore={historyQuery.hasNextPage}
                loadingMore={historyQuery.isFetchingNextPage}
                onLoadMore={
                    historyQuery.hasNextPage
                        ? () => {
                              void historyQuery.fetchNextPage()
                          }
                        : undefined
                }
            />
            <ApprovalActionBar
                id="customer-receivables-receipt-approval-action-bar"
                allowedActions={allowedActions}
                recoveryOptions={recoveryQuery.data?.actions ?? []}
                workItemId={workItemId}
                expectedTaskVersion={expectedTaskVersion}
                instance={approval?.instance}
                definition={approval?.definition}
                documentType={CUSTOMER_RECEIPT_DOCUMENT_TYPE}
                documentId={documentId}
                documentVersion={documentVersion}
                editDocumentHref={
                    documentId
                        ? financialDraftEditHref("customer_receipt", documentId)
                        : undefined
                }
                afterCancelStatusLabel="草稿"
                onDecisionApplied={onDecisionApplied}
            />
        </div>
    )
}
