"use client"

import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import { WorkspaceTaskFooter } from "@/components/business/workspace-task-pane"
import {
    DetailWorkspaceBody,
    DetailWorkspacePage,
    DetailWorkspaceTabs,
    SequentialProcessBar,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { DifferencesWorkspace } from "@/features/supplier-settlements/components/differences-workspace"
import { SettlementCenterAudit } from "@/features/supplier-settlements/components/settlement-center-audit"
import { SettlementCenterBlockersAlert } from "@/features/supplier-settlements/components/settlement-center-blockers"
import {
    SettlementConfirmSettlementDialog,
    SettlementEvidenceDialog,
    SettlementRejectDialog,
    SettlementResolveDialog,
    SettlementSubmitReviewDialog,
} from "@/features/supplier-settlements/components/settlement-center-dialogs"
import { SettlementCenterDocumentHeader } from "@/features/supplier-settlements/components/settlement-center-document-header"
import {
    SettlementCenterEmpty,
    SettlementCenterError,
    SettlementCenterForbidden,
    SettlementCenterLoading,
} from "@/features/supplier-settlements/components/settlement-center-states"
import { SettlementCenterItems } from "@/features/supplier-settlements/components/settlement-center-items"
import { SettlementCenterOverview } from "@/features/supplier-settlements/components/settlement-center-overview"
import { SettlementCenterPayable } from "@/features/supplier-settlements/components/settlement-center-payable"
import { SettlementCenterResultPanel } from "@/features/supplier-settlements/components/settlement-center-result"
import { SettlementCenterReview } from "@/features/supplier-settlements/components/settlement-center-review"
import { SettlementCenterTotals } from "@/features/supplier-settlements/components/settlement-center-totals"
import { useSettlementCenterActions } from "@/features/supplier-settlements/hooks/use-settlement-center-actions"
import { useSettlementResultFocus } from "@/features/supplier-settlements/hooks/use-settlement-result-focus"
import { useSettlementSectionHotkey } from "@/features/supplier-settlements/hooks/use-settlement-section-hotkey"
import type { SettlementsUrlState } from "@/features/supplier-settlements/lib/url-state"
import {
    SECTION_LABEL,
    SECTIONS,
    type SettlementSection,
} from "@/features/supplier-settlements/types"
import { isNotFoundError } from "@/lib/api/errors"

function SettlementCenter({
    statementId,
    workItemId,
    urlState,
    patchUrl,
    returnTo,
    onBack,
    embedded = false,
    onTaskCompleted,
}: {
    statementId: string
    workItemId?: string
    urlState: SettlementsUrlState
    patchUrl: (patch: Partial<SettlementsUrlState>) => void
    returnTo?: string
    onBack: () => void
    embedded?: boolean
    onTaskCompleted?: (workItemId: string) => void
}) {
    const account = useAccountProfileQuery()
    const canDetail = hasPermission(
        account.data?.permissions,
        "supplier_settlement_statement:detail",
    )
    const canReview = hasPermission(
        account.data?.permissions,
        "supplier_settlement_statement:confirm",
    )
    const actions = useSettlementCenterActions({
        statementId,
        workItemId,
        urlState,
        patchUrl,
        onTaskCompleted,
        detailEnabled: !account.isPending && canDetail,
    })
    useSettlementResultFocus(actions.result, actions.resultRef)
    useSettlementSectionHotkey(patchUrl)

    const detailQuery = actions.detailQuery
    const section = urlState.section
    const backLabel = returnTo ? "返回来源页" : "返回对账单列表"
    const backVisibility = returnTo ? "always" : "standalone-only"

    if (account.isPending || (canDetail && detailQuery.isPending)) {
        return <SettlementCenterLoading />
    }

    if (account.isError) {
        return (
            <SettlementCenterError
                error={account.error}
                onBack={onBack}
                backLabel={backLabel}
                backVisibility={backVisibility}
                embedded={embedded}
                onRetry={() => void account.refetch()}
            />
        )
    }

    if (!canDetail) {
        return (
            <SettlementCenterForbidden
                onBack={onBack}
                backLabel={backLabel}
                backVisibility={backVisibility}
                embedded={embedded}
            />
        )
    }

    if (detailQuery.isError) {
        if (isNotFoundError(detailQuery.error)) {
            return (
                <SettlementCenterEmpty
                    onBack={onBack}
                    backLabel={backLabel}
                    backVisibility={backVisibility}
                    embedded={embedded}
                />
            )
        }
        return (
            <SettlementCenterError
                error={detailQuery.error}
                onBack={onBack}
                backLabel={backLabel}
                backVisibility={backVisibility}
                embedded={embedded}
                onRetry={() => void detailQuery.refetch()}
            />
        )
    }

    const detail = actions.data
    if (!detail) {
        return (
            <SettlementCenterEmpty
                onBack={onBack}
                backLabel={backLabel}
                backVisibility={backVisibility}
                embedded={embedded}
            />
        )
    }

    const st = detail.statement
    const hasAlertContent = Boolean(
        detail.workItem ||
        detail.workItemBlocker ||
        detail.actionBlockers.length > 0 ||
        actions.result,
    )

    return (
        <DetailWorkspacePage>
            <SettlementCenterDocumentHeader
                statement={st}
                immutableFactsAsOf={detail.freshness.immutableFactsAsOf}
                allowed={actions.allowed}
                refreshPending={actions.refreshMutation.isPending}
                submitBlocker={actions.submitBlocker}
                onBack={onBack}
                backLabel={backLabel}
                backVisibility={backVisibility}
                onRefresh={actions.onRefresh}
                onSubmitReview={() => actions.setSubmitOpen(true)}
                embedded={embedded}
                tabs={
                    <DetailWorkspaceTabs
                        value={section}
                        onValueChange={(value) =>
                            patchUrl({ section: value as SettlementSection })
                        }
                        items={SECTIONS.map((id) => ({
                            id,
                            label: SECTION_LABEL[id],
                            badge:
                                id === "differences" &&
                                detail.differenceSummary.blocking > 0 ? (
                                    <span className="num ml-1.5 rounded-sm bg-destructive-soft px-1.5 text-tiny leading-4 font-semibold text-destructive-soft-foreground">
                                        {detail.differenceSummary.blocking}
                                    </span>
                                ) : undefined,
                        }))}
                        ariaLabel="结算单分区"
                        tabIdPrefix="supplier-settlements-center-tab"
                    />
                }
            />
            <DetailWorkspaceBody
                summary={<SettlementCenterTotals detail={detail} />}
                alerts={
                    hasAlertContent ? (
                        <>
                            {detail.workItem ? (
                                <WorkspaceTaskFooter inline={!embedded}>
                                    <div className="flex flex-wrap items-center gap-2">
                                        {embedded ? (
                                            canReview && (
                                                <Button
                                                    id="supplier-settlements-center-confirm-task"
                                                    disabled={
                                                        actions.responsibilityStatus !==
                                                            "assigned_to_me" ||
                                                        !actions.allowed.has(
                                                            "CONFIRM",
                                                        ) ||
                                                        actions.decisionMutation
                                                            .isPending
                                                    }
                                                    onClick={() =>
                                                        actions.setConfirmOpen(
                                                            true,
                                                        )
                                                    }
                                                >
                                                    确认结算
                                                </Button>
                                            )
                                        ) : (
                                            <SequentialProcessBar
                                                id="supplier-settlements-center-process-bar"
                                                current={1}
                                                total={1}
                                                responsibilityStatus={
                                                    actions.responsibilityStatus
                                                }
                                                processLabel="确认结算"
                                                showProcess={canReview}
                                                processDisabled={
                                                    !actions.allowed.has(
                                                        "CONFIRM",
                                                    )
                                                }
                                                showProcessNext={false}
                                                pending={
                                                    actions.decisionMutation
                                                        .isPending
                                                }
                                                onBack={onBack}
                                                onProcess={() =>
                                                    actions.setConfirmOpen(true)
                                                }
                                                onProcessNext={() => undefined}
                                            />
                                        )}
                                        {canReview &&
                                        actions.responsibilityStatus ===
                                            "assigned_to_me" &&
                                        actions.allowed.has("REJECT") ? (
                                            <Button
                                                id="supplier-settlements-center-reject-trigger"
                                                type="button"
                                                size="sm"
                                                variant="outline"
                                                disabled={
                                                    actions.decisionMutation
                                                        .isPending
                                                }
                                                onClick={() =>
                                                    actions.setRejectOpen(true)
                                                }
                                            >
                                                驳回复核
                                            </Button>
                                        ) : null}
                                    </div>
                                </WorkspaceTaskFooter>
                            ) : detail.workItemBlocker ? (
                                <Alert variant="warning">
                                    <AlertTitle>
                                        当前无法办理结算复核
                                    </AlertTitle>
                                    <AlertDescription>
                                        {detail.workItemBlocker.message}
                                    </AlertDescription>
                                </Alert>
                            ) : null}

                            <SettlementCenterBlockersAlert
                                blockers={detail.actionBlockers}
                            />

                            <SettlementCenterResultPanel
                                result={actions.result}
                                resultRef={actions.resultRef}
                                onClose={actions.clearResult}
                            />
                        </>
                    ) : undefined
                }
            >
                {section === "overview" ? (
                    <SettlementCenterOverview
                        detail={detail}
                        patchUrl={patchUrl}
                    />
                ) : null}

                {section === "items" ? (
                    <SettlementCenterItems
                        items={detail.items}
                        statementId={statementId}
                    />
                ) : null}

                {section === "differences" ? (
                    <DifferencesWorkspace
                        differences={detail.differences}
                        activeDiff={actions.activeDiff}
                        onSelect={(id) => patchUrl({ diff: id })}
                        allowed={actions.allowed}
                        onResolve={() => actions.setResolveOpen(true)}
                        onEvidence={() => actions.setEvidenceOpen(true)}
                    />
                ) : null}

                {section === "review" ? (
                    <SettlementCenterReview detail={detail} />
                ) : null}

                {section === "payable" ? (
                    <SettlementCenterPayable payable={detail.payable} />
                ) : null}

                {section === "audit" ? (
                    <SettlementCenterAudit events={detail.auditEvents} />
                ) : null}
            </DetailWorkspaceBody>

            <SettlementResolveDialog
                open={actions.resolveOpen}
                onOpenChange={actions.setResolveOpen}
                activeDiff={actions.activeDiff}
                resolution={actions.resolution}
                onResolutionChange={actions.setResolution}
                reasonCode={actions.reasonCode}
                onReasonCodeChange={actions.setReasonCode}
                pending={actions.resolveMutation.isPending}
                onSubmit={actions.onResolve}
            />

            <SettlementEvidenceDialog
                open={actions.evidenceOpen}
                onOpenChange={actions.setEvidenceOpen}
                referenceId={actions.evidenceReferenceId}
                onReferenceIdChange={actions.setEvidenceReferenceId}
                comment={actions.evidenceComment}
                onCommentChange={actions.setEvidenceComment}
                pending={actions.evidenceMutation.isPending}
                onSubmit={actions.onEvidence}
            />

            <SettlementSubmitReviewDialog
                open={actions.submitOpen}
                onOpenChange={actions.setSubmitOpen}
                statement={st}
                totals={detail.totals}
                reviewerUserId={actions.reviewerUserId}
                onReviewerUserIdChange={actions.setReviewerUserId}
                pending={actions.submitMutation.isPending}
                onConfirm={async () => {
                    await actions.onSubmitReview()
                }}
            />

            <SettlementConfirmSettlementDialog
                open={canReview && actions.confirmOpen}
                onOpenChange={actions.setConfirmOpen}
                statement={st}
                totals={detail.totals}
                pending={actions.decisionMutation.isPending}
                onConfirm={async () => {
                    await actions.onConfirm()
                }}
            />

            <SettlementRejectDialog
                open={canReview && actions.rejectOpen}
                onOpenChange={actions.setRejectOpen}
                reasonCode={actions.rejectReason}
                onReasonCodeChange={actions.setRejectReason}
                comment={actions.rejectComment}
                onCommentChange={actions.setRejectComment}
                pending={actions.decisionMutation.isPending}
                onSubmit={actions.onReject}
            />
        </DetailWorkspacePage>
    )
}

export { SettlementCenter }
