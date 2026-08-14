"use client"

import * as React from "react"
import { useRouter } from "next/navigation"

import {
    FormalActionConfirmDialog,
    FormalActionResult,
    MetricFilterItem,
    MetricStrip,
    PageScaffold,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { MallSyncMappingView } from "@/features/mall-sync/components/mall-sync-mapping-view"
import { MallSyncReadViews } from "@/features/mall-sync/components/mall-sync-read-views"
import { SourceSystemsCard } from "@/features/mall-sync/components/source-systems-card"
import { useMallSyncUrlState } from "@/features/mall-sync/pages/hooks/use-mall-sync-url-state"
import { useMallSyncPage } from "@/features/mall-sync/pages/hooks/use-mall-sync-page"
import { MallSyncPageHeader } from "@/features/mall-sync/pages/components/mall-sync-page-header"
import { MallSyncOwnershipBanner } from "@/features/mall-sync/pages/components/mall-sync-ownership-banner"
import {
    MallSyncPageError,
    MallSyncPageLoading,
} from "@/features/mall-sync/pages/components/mall-sync-page-status"
import { MallSyncViewToolbar } from "@/features/mall-sync/pages/components/mall-sync-view-toolbar"
import {
    MallSyncIncrementalDialog,
    MallSyncPullDialog,
} from "@/features/mall-sync/pages/components/mall-sync-sync-dialogs"
import {
    MallSyncReleaseDialog,
    MallSyncSourceFixDialog,
} from "@/features/mall-sync/pages/components/mall-sync-mapping-dialogs"
import { MallSyncConfirmMappingForm } from "@/features/mall-sync/pages/components/mall-sync-confirm-mapping-form"

export function MallSyncPage() {
    const router = useRouter()
    const url = useMallSyncUrlState()
    const page = useMallSyncPage({
        view: url.view,
        q: url.q,
        jobId: url.jobId,
        snapshotId: url.snapshotId,
        mappingTaskId: url.mappingTaskId,
        workItemId: url.workItemId,
        differenceId: url.differenceId,
        queueContextId: url.queueContextId,
        searchParams: url.searchParams,
        patchUrl: url.patchUrl,
    })

    if (page.pageQuery.isPending && !page.data) {
        return <MallSyncPageLoading />
    }

    if (page.pageQuery.isError) {
        return (
            <MallSyncPageError
                message={(page.pageQuery.error as Error)?.message ?? "请重试"}
                onRetry={() => void page.pageQuery.refetch()}
            />
        )
    }

    const { context } = page

    return (
        <PageScaffold>
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

            <MallSyncOwnershipBanner
                ownership={page.ownership}
                sealed={page.sealed}
                view={url.view}
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
                    Math.min(5, Math.max(2, context?.metrics.length ?? 4)) as
                        | 2
                        | 3
                        | 4
                        | 5
                }
                aria-label="商城同步指标"
            >
                {(context?.metrics ?? []).map((m) => (
                    <MetricFilterItem
                        key={m.key}
                        label={m.label}
                        value={m.count != null ? m.count : (m.value ?? "—")}
                        detail={m.detail}
                        active={url.view === m.targetView}
                        onClick={() => {
                            url.patchUrl({
                                view: m.targetView,
                                ...url.clearObjectParamsForView(m.targetView),
                            })
                        }}
                    />
                ))}
            </MetricStrip>

            <MallSyncViewToolbar
                view={url.view}
                searchInput={url.searchInput}
                searchInputRef={url.searchInputRef}
                onSearchChange={url.setSearchInput}
                hasActiveFilters={url.hasActiveFilters}
                onClearFilters={url.clearAllFilters}
                onViewChange={(next) =>
                    url.patchUrl({
                        view: next,
                        // 清理跨视图残留的对象定位参数；保留当前视图归属的对象参数
                        ...url.clearObjectParamsForView(next),
                    })
                }
            />

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
                    <AlertDescription>{page.actionError}</AlertDescription>
                </Alert>
            ) : null}

            {/* ── 子视图内容 ── */}
            <MallSyncReadViews
                view={url.view}
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

            {url.view === "mapping" ? (
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
                            selectedCandidateId={page.selectedCandidateId}
                            canConfirmMapping={page.canConfirmMapping}
                            responsibilityStatus={page.responsibilityStatus}
                            onOpenSourceFix={() => page.setSourceFixOpen(true)}
                            onOpenRelease={() => page.setReleaseOpen(true)}
                        />
                    }
                    mappingIndex={page.mappingIndex}
                    responsibilityStatus={page.responsibilityStatus}
                    canConfirmMapping={page.canConfirmMapping}
                    responsibilityPending={page.responsibilityPending}
                    reapplyPending={page.reapplyPending}
                    onReapply={page.handleReapply}
                    onResolveUnknownReapply={page.handleResolveUnknownReapply}
                    onBackToQueue={() =>
                        router.push(
                            `/workspace/tasks?queueContextId=${encodeURIComponent(url.queueContextId)}`,
                        )
                    }
                    onConfirm={() => page.confirmForm.handleSubmit()}
                    onStartProcessing={page.handleStartProcessing}
                />
            ) : null}

            {/* 立即增量 */}
            <MallSyncIncrementalDialog
                open={page.incrementalOpen}
                onOpenChange={page.setIncrementalOpen}
                firstPhase={page.firstPhase}
                manualSyncDisabledReason={page.manualSyncDisabledReason}
                stage={page.stage}
                currentWatermark={context?.freshness.currentWatermark}
                form={page.incrementalForm}
            />

            {/* 按单补拉 */}
            <MallSyncPullDialog
                open={page.pullOpen}
                onOpenChange={page.setPullOpen}
                firstPhase={page.firstPhase}
                manualSyncDisabledReason={page.manualSyncDisabledReason}
                form={page.pullForm}
            />

            <MallSyncSourceFixDialog
                open={page.sourceFixOpen}
                onOpenChange={page.setSourceFixOpen}
                form={page.sourceFixForm}
            />

            <MallSyncReleaseDialog
                open={page.releaseOpen}
                onOpenChange={page.setReleaseOpen}
                form={page.releaseForm}
            />

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
