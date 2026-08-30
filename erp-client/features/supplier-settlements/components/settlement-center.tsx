"use client"

import {
    PageScaffold,
    SequentialProcessBar,
    surfaceInsetClassName,
    surfacePanelClassName,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { CrossEntryBanner } from "@/features/supplier-settlements/components/cross-entry-banner"
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
import { formatDateTime } from "@/lib/datetime"
import { cn } from "@/lib/utils"

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
    const actions = useSettlementCenterActions({
        statementId,
        workItemId,
        urlState,
        patchUrl,
        onTaskCompleted,
    })
    useSettlementResultFocus(actions.result, actions.resultRef)
    useSettlementSectionHotkey(patchUrl)

    const detailQuery = actions.detailQuery
    const section = urlState.section

    if (detailQuery.isPending) {
        return <SettlementCenterLoading />
    }

    if (detailQuery.isError) {
        return (
            <SettlementCenterError
                error={detailQuery.error}
                onBack={onBack}
                onRetry={() => void detailQuery.refetch()}
            />
        )
    }

    const detail = actions.data
    if (!detail) {
        return <SettlementCenterEmpty onBack={onBack} />
    }

    const st = detail.statement

    return (
        <PageScaffold
            density={embedded ? "compact" : "default"}
            className={embedded ? "max-w-none p-0" : undefined}
        >
            <SettlementCenterDocumentHeader
                statement={st}
                immutableFactsAsOf={detail.freshness.immutableFactsAsOf}
                allowed={actions.allowed}
                refreshPending={actions.refreshMutation.isPending}
                submitBlocker={actions.submitBlocker}
                onBack={onBack}
                onRefresh={actions.onRefresh}
                onSubmitReview={() => actions.setSubmitOpen(true)}
            />

            {returnTo ? <CrossEntryBanner returnTo={returnTo} /> : null}

            {detail.workItem ? (
                <div className="space-y-2">
                    <SequentialProcessBar
                        current={1}
                        total={1}
                        responsibilityStatus={actions.responsibilityStatus}
                        processLabel="确认结算"
                        processDisabled={!actions.allowed.has("CONFIRM")}
                        showProcessNext={false}
                        pending={actions.decisionMutation.isPending}
                        onBack={onBack}
                        onProcess={() => actions.setConfirmOpen(true)}
                        onProcessNext={() => undefined}
                    />
                    {actions.responsibilityStatus === "assigned_to_me" &&
                    actions.allowed.has("REJECT") ? (
                        <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            disabled={actions.decisionMutation.isPending}
                            onClick={() => actions.setRejectOpen(true)}
                        >
                            驳回复核
                        </Button>
                    ) : null}
                </div>
            ) : detail.workItemBlocker ? (
                <Alert variant="warning">
                    <AlertTitle>正式复核任务不可处理</AlertTitle>
                    <AlertDescription>
                        {detail.workItemBlocker.message}
                    </AlertDescription>
                </Alert>
            ) : null}

            <SettlementCenterBlockersAlert blockers={detail.actionBlockers} />

            <SettlementCenterResultPanel
                result={actions.result}
                resultRef={actions.resultRef}
            />

            <SettlementCenterTotals detail={detail} />

            <div
                className={cn(
                    surfaceInsetClassName,
                    "px-3 py-2 text-xs text-muted-foreground",
                )}
            >
                <span className="font-medium text-foreground">来源数据 </span>
                更新时间 {formatDateTime(st.sourceAsOf, "default")}
                {st.externalBillNo ? (
                    <>
                        {" "}
                        · 账单 {st.externalBillNo}（第{" "}
                        {String(st.externalBillVersion ?? "").replace(
                            /^v/i,
                            "",
                        )}{" "}
                        版）
                    </>
                ) : null}
                <span className="ml-2">以下数据仅供参考，不进入结算结果</span>
            </div>

            <div
                className={cn(surfacePanelClassName, "min-w-0 overflow-hidden")}
            >
                <Tabs
                    value={section}
                    onValueChange={(v) =>
                        patchUrl({ section: v as SettlementSection })
                    }
                >
                    <TabsList
                        variant="line"
                        className="sticky top-0 z-10 h-auto w-full flex-wrap justify-start gap-1 overflow-x-auto rounded-none border-b border-grid bg-card/95 px-3 py-1.5 backdrop-blur supports-backdrop-filter:bg-card/80"
                    >
                        {SECTIONS.map((s) => (
                            <TabsTrigger key={s} value={s}>
                                {SECTION_LABEL[s]}
                                {s === "differences" &&
                                detail.differenceSummary.blocking > 0
                                    ? ` (${detail.differenceSummary.blocking})`
                                    : null}
                            </TabsTrigger>
                        ))}
                    </TabsList>
                </Tabs>
                <div className="space-y-4 p-3 md:p-4">
                    <p className="text-xs text-muted-foreground">
                        快捷键 d 可直达差异处理
                    </p>

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
                </div>
            </div>

            <SettlementResolveDialog
                open={actions.resolveOpen}
                onOpenChange={actions.setResolveOpen}
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
                reviewerUserId={actions.reviewerUserId}
                onReviewerUserIdChange={actions.setReviewerUserId}
                pending={actions.submitMutation.isPending}
                onConfirm={async () => {
                    await actions.onSubmitReview()
                }}
            />

            <SettlementConfirmSettlementDialog
                open={actions.confirmOpen}
                onOpenChange={actions.setConfirmOpen}
                statement={st}
                totals={detail.totals}
                pending={actions.decisionMutation.isPending}
                onConfirm={async () => {
                    await actions.onConfirm()
                }}
            />

            <SettlementRejectDialog
                open={actions.rejectOpen}
                onOpenChange={actions.setRejectOpen}
                reasonCode={actions.rejectReason}
                onReasonCodeChange={actions.setRejectReason}
                pending={actions.decisionMutation.isPending}
                onSubmit={actions.onReject}
            />
        </PageScaffold>
    )
}

export { SettlementCenter }
