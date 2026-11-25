"use client"

import {
    CheckIcon,
    CircleCheckIcon,
    CircleDashedIcon,
    ShieldAlertIcon,
    ShieldCheckIcon,
    UsersRoundIcon,
    type LucideIcon,
} from "lucide-react"

import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import {
    BusinessEmptyState,
    BusinessFailureState,
    DetailWorkspaceBody,
    DetailWorkspacePage,
    DetailWorkspaceSection,
    DetailWorkspaceTabs,
} from "@/components/business"
import type { ResponsibilityStatus } from "@/components/business/workflow-actions"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import { StatusBadge, type StatusTone } from "@/components/ui/status-badge"
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
import { toAutomationIdSegment } from "@/lib/automation-id"
import { isCommandOutcomeUnknown } from "@/lib/formal-command"
import { responsibilityText } from "@/lib/ui-text"
import { SettlementReviewWorkspace } from "./settlement-review-workspace"

const RESPONSIBILITY_STATUS_META: Record<
    ResponsibilityStatus,
    { label: string; tone: StatusTone; icon: LucideIcon }
> = {
    assigned_to_me: {
        label: responsibilityText.assignedToMe,
        tone: "success",
        icon: ShieldCheckIcon,
    },
    assigned_to_other: {
        label: responsibilityText.assignedToOther,
        tone: "neutral",
        icon: UsersRoundIcon,
    },
    blocked: {
        label: responsibilityText.blocked,
        tone: "warning",
        icon: ShieldAlertIcon,
    },
    completed: {
        label: responsibilityText.completed,
        tone: "success",
        icon: CircleCheckIcon,
    },
    closed: {
        label: responsibilityText.closed,
        tone: "neutral",
        icon: CircleDashedIcon,
    },
}

function SettlementSectionHotkey({
    patchUrl,
}: {
    patchUrl: (patch: Partial<SettlementsUrlState>) => void
}) {
    useSettlementSectionHotkey(patchUrl)
    return null
}

function SettlementCenter({
    statementId,
    workItemId,
    urlState,
    patchUrl,
    returnTo,
    onBack,
    embedded = false,
    idPrefix = `supplier-settlements-center-${toAutomationIdSegment(statementId)}`,
    fullTaskHref,
    onTaskCompleted,
}: {
    statementId: string
    workItemId?: string
    urlState: SettlementsUrlState
    patchUrl: (patch: Partial<SettlementsUrlState>) => void
    returnTo?: string
    onBack: () => void
    embedded?: boolean
    idPrefix?: string
    fullTaskHref?: string
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

    const detailQuery = actions.detailQuery
    const section = urlState.section
    const backLabel = returnTo ? "返回来源页" : "返回对账单列表"
    const backVisibility = returnTo ? "always" : "standalone-only"

    if (account.isPending || (canDetail && detailQuery.isPending)) {
        if (embedded) {
            return (
                <DetailWorkspaceSection>
                    <div
                        className="space-y-4"
                        aria-busy="true"
                        aria-label="正在加载供应商结算单"
                    >
                        <div className="h-16 animate-pulse rounded-lg bg-surface-sunken" />
                        <div className="h-44 animate-pulse rounded-lg bg-surface-sunken" />
                    </div>
                </DetailWorkspaceSection>
            )
        }
        return <SettlementCenterLoading />
    }

    if (account.isError) {
        if (embedded) {
            return (
                <DetailWorkspaceSection width="form">
                    <BusinessFailureState
                        idPrefix={`${idPrefix}-error`}
                        title="结算单加载失败"
                        error={account.error}
                        action={
                            <Button
                                id={`${idPrefix}-error-retry`}
                                type="button"
                                variant="outline"
                                size="sm"
                                onClick={() => void account.refetch()}
                            >
                                重试
                            </Button>
                        }
                    />
                </DetailWorkspaceSection>
            )
        }
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
        if (embedded) {
            return (
                <DetailWorkspaceSection width="form">
                    <BusinessFailureState
                        kind="permission"
                        title="权限不足"
                        description="当前账号没有查看供应商结算单详情的权限，请联系管理员开通。"
                    />
                </DetailWorkspaceSection>
            )
        }
        return (
            <SettlementCenterForbidden
                onBack={onBack}
                backLabel={backLabel}
                backVisibility={backVisibility}
                embedded={embedded}
            />
        )
    }

    if (
        detailQuery.isError &&
        !(
            embedded &&
            detailQuery.data &&
            isCommandOutcomeUnknown(detailQuery.error)
        )
    ) {
        if (isNotFoundError(detailQuery.error)) {
            if (embedded) {
                return (
                    <DetailWorkspaceSection width="form">
                        <BusinessEmptyState
                            kind="no-data"
                            title="结算单不存在"
                            description="该结算单不存在或已被作废。"
                        />
                    </DetailWorkspaceSection>
                )
            }
            return (
                <SettlementCenterEmpty
                    onBack={onBack}
                    backLabel={backLabel}
                    backVisibility={backVisibility}
                    embedded={embedded}
                />
            )
        }
        if (embedded) {
            return (
                <DetailWorkspaceSection width="form">
                    <BusinessFailureState
                        idPrefix={`${idPrefix}-error`}
                        title="结算单加载失败"
                        error={detailQuery.error}
                        action={
                            <Button
                                id={`${idPrefix}-error-retry`}
                                type="button"
                                variant="outline"
                                size="sm"
                                onClick={() => void detailQuery.refetch()}
                            >
                                重试
                            </Button>
                        }
                    />
                </DetailWorkspaceSection>
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
        if (embedded) {
            return (
                <DetailWorkspaceSection width="form">
                    <BusinessEmptyState
                        kind="no-data"
                        title="结算单不存在"
                        description="该结算单不存在或已被作废。"
                    />
                </DetailWorkspaceSection>
            )
        }
        return (
            <SettlementCenterEmpty
                onBack={onBack}
                backLabel={backLabel}
                backVisibility={backVisibility}
                embedded={embedded}
            />
        )
    }

    if (embedded) {
        return (
            <SettlementReviewWorkspace
                detail={detail}
                actions={actions}
                idPrefix={idPrefix}
                fullTaskHref={fullTaskHref}
                canReview={canReview}
                canManageDifferences={hasPermission(
                    account.data?.permissions,
                    "supplier_settlement_difference:update",
                )}
                section={section}
                patchUrl={patchUrl}
                onTaskCompleted={onTaskCompleted}
            />
        )
    }

    const st = detail.statement
    const responsibility =
        RESPONSIBILITY_STATUS_META[actions.responsibilityStatus]
    const canConfirmAction =
        actions.responsibilityStatus === "assigned_to_me" &&
        actions.allowed.has("CONFIRM") &&
        !actions.decisionMutation.isPending
    const canRejectAction =
        canReview &&
        actions.responsibilityStatus === "assigned_to_me" &&
        actions.allowed.has("REJECT")

    const reviewButtons = (
        <>
            {canRejectAction ? (
                <Button
                    id="supplier-settlements-center-reject-trigger"
                    type="button"
                    size="sm"
                    variant="outline"
                    className="border-destructive-border text-destructive-soft-foreground hover:bg-destructive-soft"
                    disabled={actions.decisionMutation.isPending}
                    onClick={() => actions.setRejectOpen(true)}
                >
                    驳回复核
                </Button>
            ) : null}
            {canReview ? (
                <LoadingButton
                    id="supplier-settlements-center-confirm-task"
                    type="button"
                    size="sm"
                    loading={actions.decisionMutation.isPending}
                    disabled={!canConfirmAction}
                    onClick={() => actions.setConfirmOpen(true)}
                >
                    {!actions.decisionMutation.isPending ? (
                        <CheckIcon
                            data-icon="inline-start"
                            aria-hidden="true"
                        />
                    ) : null}
                    {actions.decisionMutation.isPending
                        ? "正在处理"
                        : "确认结算"}
                </LoadingButton>
            ) : null}
        </>
    )

    const sectionTabs = (
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
                        <span className="num ml-1.5 rounded-sm bg-destructive-soft px-1.5 text-tiny font-semibold text-destructive-soft-foreground">
                            {detail.differenceSummary.blocking}
                        </span>
                    ) : undefined,
            }))}
            ariaLabel="结算单分区"
            tabIdPrefix="supplier-settlements-center-tab"
        />
    )

    const activeSectionContent = (
        <>
            {section === "overview" ? (
                <SettlementCenterOverview detail={detail} patchUrl={patchUrl} />
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
        </>
    )

    const dialogs = (
        <>
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
                statement={st}
                reasonCode={actions.rejectReason}
                onReasonCodeChange={actions.setRejectReason}
                comment={actions.rejectComment}
                onCommentChange={actions.setRejectComment}
                pending={actions.decisionMutation.isPending}
                onSubmit={actions.onReject}
            />
        </>
    )

    const hasAlertContent = Boolean(
        detail.workItem ||
        detail.workItemBlocker ||
        detail.actionBlockers.length > 0 ||
        actions.result,
    )

    return (
        <DetailWorkspacePage>
            <SettlementSectionHotkey patchUrl={patchUrl} />
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
                embedded={false}
                tabs={sectionTabs}
            />
            <DetailWorkspaceBody
                summary={<SettlementCenterTotals detail={detail} />}
                alerts={
                    hasAlertContent ? (
                        <>
                            {detail.workItem ? (
                                <section
                                    aria-label="结算复核操作"
                                    className="flex flex-col gap-3 border-b border-border/60 pb-4 lg:flex-row lg:items-center lg:justify-between"
                                >
                                    <div
                                        className="flex min-w-0 flex-wrap items-center gap-2.5"
                                        aria-live="polite"
                                    >
                                        <StatusBadge
                                            tone={responsibility.tone}
                                            icon={responsibility.icon}
                                            label={responsibility.label}
                                        />
                                        {actions.responsibilityStatus ===
                                        "assigned_to_other" ? (
                                            <span className="text-body-compact text-destructive">
                                                {responsibilityText.changed}
                                            </span>
                                        ) : null}
                                    </div>
                                    <div className="flex flex-wrap items-center gap-2">
                                        {reviewButtons}
                                    </div>
                                </section>
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
                {activeSectionContent}
            </DetailWorkspaceBody>

            {dialogs}
        </DetailWorkspacePage>
    )
}

export { SettlementCenter }
