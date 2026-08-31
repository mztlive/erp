"use client"

import * as React from "react"
import { useRouter } from "next/navigation"

import {
    BusinessFailureState,
    FormalActionConfirmDialog,
    FormalActionResult,
    MetricFilterItem,
    MetricStrip,
    PageScaffold,
    workspaceEmbeddedScaffoldClassName,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { MallSyncMappingView } from "@/features/mall-sync/components/mall-sync-mapping-view"
import { MallSyncReadViews } from "@/features/mall-sync/components/mall-sync-read-views"
import { SourceSystemsCard } from "@/features/mall-sync/components/source-systems-card"
import { useMallSyncUrlState } from "@/features/mall-sync/pages/hooks/use-mall-sync-url-state"
import type {
    MallSyncAppliedChip,
    PatchUrl,
} from "@/features/mall-sync/pages/hooks/use-mall-sync-url-state"
import { useMallSyncPage } from "@/features/mall-sync/pages/hooks/use-mall-sync-page"
import { MallSyncPageHeader } from "@/features/mall-sync/pages/components/mall-sync-page-header"
import { MallSyncOwnershipBanner } from "@/features/mall-sync/pages/components/mall-sync-ownership-banner"
import { MallSyncPageLoading } from "@/features/mall-sync/pages/components/mall-sync-page-status"
import { MallSyncViewToolbar } from "@/features/mall-sync/pages/components/mall-sync-view-toolbar"
import {
    MallSyncIncrementalDialog,
    MallSyncPullDialog,
} from "@/features/mall-sync/pages/components/mall-sync-sync-dialogs"
import { MallSyncSourceFixDialog } from "@/features/mall-sync/pages/components/mall-sync-mapping-dialogs"
import { MallSyncConfirmMappingForm } from "@/features/mall-sync/pages/components/mall-sync-confirm-mapping-form"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { MAPPING_TYPE_LABEL } from "@/features/mall-sync/types"

export function MallSyncPage({
    forcedMappingTaskId,
    forcedWorkItemId,
    forcedQueueContextId,
    embedded = false,
    onTaskCompleted,
}: {
    forcedMappingTaskId?: string
    forcedWorkItemId?: string
    forcedQueueContextId?: string
    embedded?: boolean
    onTaskCompleted?: (workItemId: string) => void
} = {}) {
    const router = useRouter()
    const url = useMallSyncUrlState()
    const patchUrl = React.useCallback<PatchUrl>(
        (patch) => {
            if (!embedded) url.patchUrl(patch)
        },
        [embedded, url],
    )
    const view = embedded ? "mapping" : url.view
    const mappingTaskId = forcedMappingTaskId ?? url.mappingTaskId
    const workItemId = forcedWorkItemId ?? url.workItemId
    const queueContextId = forcedQueueContextId ?? url.queueContextId
    const page = useMallSyncPage({
        view,
        q: embedded ? "" : url.q,
        mappingType: embedded ? undefined : url.mappingType,
        jobId: embedded ? undefined : url.jobId,
        snapshotId: embedded ? undefined : url.snapshotId,
        mappingTaskId,
        workItemId,
        differenceId: embedded ? undefined : url.differenceId,
        queueContextId,
        searchParams: url.searchParams,
        patchUrl,
        advanceAfterConfirm: !embedded,
        onTaskCompleted,
    })

    /** 已生效条件全部显性化为可单独移除的 chip（含深链对象定位条件）。 */
    const appliedChips = React.useMemo<readonly MallSyncAppliedChip[]>(() => {
        const chips: MallSyncAppliedChip[] = []
        const appliedMappingType = url.mappingType
        const trimmedQ = url.q.trim()
        if (trimmedQ) {
            chips.push({ key: "q", label: `搜索：${trimmedQ}` })
        }
        if (appliedMappingType) {
            chips.push({
                key: "mappingType",
                label: `映射类型：${MAPPING_TYPE_LABEL[appliedMappingType]}`,
            })
        }
        if (url.jobId) {
            chips.push({
                key: "jobId",
                label: `任务：${page.data?.selectedJob?.jobNo ?? url.jobId}`,
            })
        }
        if (url.snapshotId) {
            chips.push({
                key: "snapshotId",
                label: `来源快照：${page.data?.selectedSnapshot?.externalOrderNo ?? url.snapshotId}`,
            })
        }
        if (url.mappingTaskId) {
            const task = page.data?.selectedMappingTask
            chips.push({
                key: "mappingTaskId",
                label: task
                    ? `映射任务：${task.externalOrderNo}（${task.mappingTypeLabel}）`
                    : `映射任务：${url.mappingTaskId}`,
            })
        } else if (url.workItemId) {
            const task = page.data?.selectedMappingTask
            chips.push({
                key: "workItemId",
                label: task
                    ? `映射待办：${task.externalOrderNo}`
                    : `映射待办：${url.workItemId}`,
            })
        }
        if (url.differenceId) {
            chips.push({
                key: "differenceId",
                label: `核对差异：${page.data?.selectedDifference?.externalOrderNo ?? url.differenceId}`,
            })
        }
        return chips
    }, [
        page.data?.selectedDifference,
        page.data?.selectedJob,
        page.data?.selectedMappingTask,
        page.data?.selectedSnapshot,
        url.differenceId,
        url.jobId,
        url.mappingTaskId,
        url.mappingType,
        url.q,
        url.snapshotId,
        url.workItemId,
    ])

    const { context } = page

    if (page.pageQuery.isPending && !page.data) {
        return <MallSyncPageLoading />
    }

    return (
        <PageScaffold
            density={embedded ? "compact" : "default"}
            className={
                embedded ? workspaceEmbeddedScaffoldClassName : undefined
            }
        >
            {!embedded ? (
                <MallSyncPageHeader
                    context={context}
                    canManualSync={page.canManualSync}
                    manualSyncDisabledReason={page.manualSyncDisabledReason}
                    onOpenIncremental={() => {
                        page.setActionError(null)
                        page.setIncrementalOpen(true)
                    }}
                    onOpenPull={() => {
                        page.setActionError(null)
                        page.setPullOpen(true)
                    }}
                    onRefresh={() => void page.pageQuery.refetch()}
                />
            ) : null}

            {!embedded && !page.pageQuery.isError ? (
                <>
                    <MallSyncOwnershipBanner
                        ownership={page.ownership}
                        sealed={page.sealed}
                        view={view}
                        onEnterHistory={() => url.patchUrl({ view: "history" })}
                    />

                    <Alert>
                        <AlertTitle>人工同步审计边界</AlertTitle>
                        <AlertDescription>
                            授权管理员可直接提交带理由的立即增量与按单补拉；服务端重读执行阶段、来源身份与水位，封存后拒绝。
                            {context?.scheduledIncrementalNote}
                        </AlertDescription>
                    </Alert>

                    {context?.sourceUnavailable ? (
                        <Alert variant="destructive">
                            <AlertTitle>来源商城不可用</AlertTitle>
                            <AlertDescription>
                                {context.sourceUnavailableMessage}
                            </AlertDescription>
                        </Alert>
                    ) : null}

                    {/* 来源系统列表 */}
                    <SourceSystemsCard />

                    <MetricStrip
                        columns={
                            Math.min(
                                5,
                                Math.max(2, context?.metrics.length ?? 4),
                            ) as 2 | 3 | 4 | 5
                        }
                        aria-label="商城同步指标"
                    >
                        {(context?.metrics ?? []).map((m) => (
                            <MetricFilterItem
                                key={m.key}
                                id={`mall-sync-metric-${toAutomationIdSegment(m.key)}`}
                                label={m.label}
                                value={
                                    m.count != null ? m.count : (m.value ?? "—")
                                }
                                detail={m.detail}
                                active={view === m.targetView}
                                onClick={() => {
                                    url.patchUrl({
                                        view: m.targetView,
                                        ...url.clearObjectParamsForView(
                                            m.targetView,
                                        ),
                                    })
                                }}
                            />
                        ))}
                    </MetricStrip>
                </>
            ) : null}

            {!embedded ? (
                <MallSyncViewToolbar
                    view={view}
                    onViewChange={(next) =>
                        url.patchUrl({
                            view: next,
                            // 清理跨视图残留的对象定位参数；保留当前视图归属的对象参数
                            ...url.clearObjectParamsForView(next),
                        })
                    }
                    searchInputRef={url.searchInputRef}
                    searchDraft={url.searchDraft}
                    setSearchDraft={url.setSearchDraft}
                    mappingTypeDraft={url.mappingTypeDraft}
                    setMappingTypeDraft={url.setMappingTypeDraft}
                    panelOpen={url.panelOpen}
                    setPanelOpen={url.setPanelOpen}
                    hasStructuredFilters={url.hasStructuredFilters}
                    hasActiveFilters={url.hasActiveFilters}
                    appliedChips={appliedChips}
                    removeFilter={url.removeFilter}
                    applyFilters={url.applyFilters}
                    resetMoreFilters={url.resetMoreFilters}
                    clearAllFilters={url.clearAllFilters}
                />
            ) : null}

            {page.pageQuery.isError ? (
                <BusinessFailureState
                    error={page.pageQuery.error}
                    onRetry={() => void page.pageQuery.refetch()}
                />
            ) : (
                <>
                    {page.result ? (
                        <FormalActionResult
                            status={
                                page.result.status === "succeeded"
                                    ? "succeeded"
                                    : page.result.status === "unknown"
                                      ? "unknown"
                                      : page.result.status === "blocked"
                                        ? "blocked"
                                        : "rejected"
                            }
                            title={page.result.title}
                            description={page.result.description}
                            reference={page.result.reference}
                            facts={page.result.facts}
                            actions={
                                page.result.status === "unknown" &&
                                page.mappingTask?.reapplyOperation?.status ===
                                    "UNKNOWN" ? (
                                    <Button
                                        id="mall-sync-result-resolve-unknown"
                                        type="button"
                                        size="sm"
                                        onClick={() =>
                                            void page.handleResolveUnknownReapply()
                                        }
                                    >
                                        查询重新归集处理结果
                                    </Button>
                                ) : undefined
                            }
                        />
                    ) : null}

                    {page.actionError ? (
                        <Alert variant="destructive">
                            <AlertTitle>动作失败</AlertTitle>
                            <AlertDescription>
                                {page.actionError}
                            </AlertDescription>
                        </Alert>
                    ) : null}

                    {/* ── 子视图内容 ── */}
                    {!embedded ? (
                        <MallSyncReadViews
                            view={view}
                            context={context}
                            ownership={page.ownership}
                            data={page.data}
                            pageJobs={page.pageJobs}
                            jobColumns={page.jobColumns}
                            snapshotColumns={page.snapshotColumns}
                            diffColumns={page.diffColumns}
                            pagination={page.pagination}
                            onPaginationChange={page.setPagination}
                            retryPending={page.retryPending}
                            onRetryJob={() => page.setRetryConfirmOpen(true)}
                            patchUrl={url.patchUrl}
                            firstPhase={page.firstPhase}
                            sealed={page.sealed}
                            onPullDifference={(externalOrderNo) => {
                                page.setPullOpen(true)
                                page.pullForm.setFieldValue(
                                    "externalOrderNo",
                                    externalOrderNo,
                                )
                            }}
                        />
                    ) : null}

                    {view === "mapping" ? (
                        <MallSyncMappingView
                            data={page.data}
                            mappingTask={page.mappingTask}
                            mappingColumns={page.mappingColumns}
                            selectedCandidateId={page.selectedCandidateId}
                            onSelectCandidate={page.setSelectedCandidateId}
                            confirmFormContent={
                                <MallSyncConfirmMappingForm
                                    mappingTask={page.mappingTask}
                                    form={page.confirmForm}
                                    selectedCandidateId={
                                        page.selectedCandidateId
                                    }
                                    canConfirmMapping={page.canConfirmMapping}
                                    responsibilityStatus={
                                        page.responsibilityStatus
                                    }
                                    onOpenSourceFix={() =>
                                        page.setSourceFixOpen(true)
                                    }
                                />
                            }
                            mappingIndex={page.mappingIndex}
                            responsibilityStatus={page.responsibilityStatus}
                            canConfirmMapping={page.canConfirmMapping}
                            actionPending={page.actionPending}
                            reapplyPending={page.reapplyPending}
                            onReapply={page.handleReapply}
                            onResolveUnknownReapply={
                                page.handleResolveUnknownReapply
                            }
                            onBackToQueue={() =>
                                router.push(
                                    workItemId
                                        ? `/workspace?currentWorkItemId=${encodeURIComponent(workItemId)}`
                                        : "/workspace",
                                )
                            }
                            onConfirm={() => page.confirmForm.handleSubmit()}
                            embedded={embedded}
                        />
                    ) : null}
                </>
            )}

            {/* 立即增量 */}
            {!embedded ? (
                <MallSyncIncrementalDialog
                    open={page.incrementalOpen}
                    onOpenChange={page.setIncrementalOpen}
                    firstPhase={page.firstPhase}
                    manualSyncDisabledReason={page.manualSyncDisabledReason}
                    stage={page.stage}
                    currentWatermark={context?.freshness.currentWatermark}
                    form={page.incrementalForm}
                />
            ) : null}

            {/* 按单补拉 */}
            {!embedded ? (
                <MallSyncPullDialog
                    open={page.pullOpen}
                    onOpenChange={page.setPullOpen}
                    firstPhase={page.firstPhase}
                    manualSyncDisabledReason={page.manualSyncDisabledReason}
                    form={page.pullForm}
                />
            ) : null}

            <MallSyncSourceFixDialog
                open={page.sourceFixOpen}
                onOpenChange={page.setSourceFixOpen}
                form={page.sourceFixForm}
            />

            {!embedded ? (
                <FormalActionConfirmDialog
                    open={page.retryConfirmOpen}
                    onOpenChange={page.setRetryConfirmOpen}
                    actionLabel="重试失败任务"
                    title="确认重试失败任务"
                    description="沿原任务范围与同步规则重试未成功部分；不回退已捕获的同步进度。"
                    fromStatus={{
                        label: page.data?.selectedJob?.statusLabel ?? "失败",
                        tone: "warning",
                    }}
                    toStatus={{ label: "重试中", tone: "info" }}
                    effects={["仅重试未成功的分页", "不修改来源数据"]}
                    irreversibleEffects={["重试记录进入任务审计"]}
                    pending={page.retryPending}
                    onConfirm={() => page.handleRetryJob()}
                />
            ) : null}

            <FormalActionConfirmDialog
                open={page.confirmOpen}
                onOpenChange={page.setConfirmOpen}
                actionLabel="确认映射"
                fromStatus={{ label: "待处理", tone: "warning" }}
                toStatus={{ label: "映射已解决", tone: "success" }}
                description="确认身份关系后，映射任务将标为已解决并完成待办；不立即形成销售版本。"
                effects={[
                    "追加可审计映射目标",
                    "完成当前任务",
                    "不向商城回写",
                    "重新归集为独立下一步",
                ]}
                irreversibleEffects={["映射结论进入不可变处理审计"]}
                pending={page.confirmPending}
                onConfirm={() => page.handleConfirm()}
            />
        </PageScaffold>
    )
}
