"use client"

import * as React from "react"
import {
    BusinessEmptyState,
    BusinessFailureState,
    DetailWorkspaceBody,
    DetailWorkspaceMeta,
    DetailWorkspacePage,
    DetailWorkspaceSection,
    DetailWorkspaceSkeleton,
    PageScaffold,
} from "@/components/business"
import { listWorkspaceStyles as styles } from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { useRouter } from "@/lib/navigation"
import { isWorkspaceFrame, workspaceHost } from "@/lib/workspace-tabs"

import { useAccountProfileQuery } from "@/features/auth/queries"
import { IntegrationEvidencePanel } from "../components/integration-evidence-panel"
import {
    useIntegrationItemQuery,
    useIntegrationQueueQuery,
} from "../hooks/queries"
import type { IntegrationFormalResult } from "../types"
import { IntegrationActionResult } from "./components/integration-action-result"
import { IntegrationDetailNav } from "./components/integration-detail-nav"
import { IntegrationErrorMetricStrip } from "./components/integration-error-metric-strip"
import { IntegrationErrorWorkspace } from "./components/integration-error-workspace"
import {
    IntegrationNotFound,
    IntegrationPageFailure,
    IntegrationPageSkeleton,
} from "./components/integration-page-states"
import { IntegrationPageHeader } from "./components/integration-page-header"
import { IntegrationQueueToolbar } from "./components/integration-queue-toolbar"
import { useIntegrationActions } from "./hooks/use-integration-actions"
import { useIntegrationItemNavigation } from "./hooks/use-integration-item-navigation"
import { useIntegrationPageSync } from "./hooks/use-integration-page-sync"
import { useIntegrationPageUrl } from "./hooks/use-integration-page-url"
import { useIntegrationSearch } from "./hooks/use-integration-search"
import { isPanelErrorClass } from "./lib/helpers"
import {
    resolveDetailTarget,
    resolveDisplayItem,
    selectQueueSelection,
} from "./lib/selection"

export function IntegrationErrorsPage({
    forcedTaskId,
    forcedDifferenceId,
    embedded = false,
    onTaskCompleted,
}: {
    forcedTaskId?: string
    forcedDifferenceId?: string
    embedded?: boolean
    onTaskCompleted?: (workItemId: string) => void
} = {}) {
    const router = useRouter()
    const {
        urlState,
        currentTaskId,
        currentDifferenceId,
        focusMode,
        query,
        replaceUrl,
        autoNext,
        returnTo,
        currentQueueHref,
    } = useIntegrationPageUrl({ forcedTaskId, forcedDifferenceId })

    const profileQuery = useAccountProfileQuery()
    const queueQuery = useIntegrationQueueQuery(
        {
            ...query,
            currentUserId: profileQuery.data?.userid,
        },
        !focusMode,
    )

    const view = queueQuery.data
    const queueItems = React.useMemo(() => view?.items ?? [], [view?.items])
    const metrics = view?.metrics

    const queueSelection = React.useMemo(
        () =>
            selectQueueSelection(
                queueItems,
                currentTaskId,
                currentDifferenceId,
            ),
        [queueItems, currentTaskId, currentDifferenceId],
    )

    const detailTarget = React.useMemo(
        () =>
            resolveDetailTarget(
                forcedTaskId,
                forcedDifferenceId,
                queueSelection,
            ),
        [forcedTaskId, forcedDifferenceId, queueSelection],
    )

    const detailItemQuery = useIntegrationItemQuery({
        itemType: detailTarget?.itemType ?? "ERROR_TASK",
        id: detailTarget?.id ?? "",
        enabled: detailTarget !== null,
    })

    const item = React.useMemo(
        () =>
            resolveDisplayItem(
                detailTarget,
                detailItemQuery.data,
                queueSelection,
            ),
        [detailTarget, detailItemQuery.data, queueSelection],
    )

    const items = React.useMemo(
        () => (focusMode && item ? [item] : queueItems),
        [focusMode, item, queueItems],
    )

    const [lastResult, setLastResult] =
        React.useState<IntegrationFormalResult | null>(null)
    const [actionError, setActionError] = React.useState<string | null>(null)

    const clearTransient = React.useCallback(() => {
        setLastResult(null)
        setActionError(null)
    }, [])

    const { positionIndex, positionTotal, goToItem, neighbor } =
        useIntegrationItemNavigation({
            items,
            queueItems,
            item,
            focusMode,
            replaceUrl,
            onBeforeNavigate: clearTransient,
        })

    // refetch 保持与页面原实现一致：普通函数，每次渲染重建，调用时才读取当前查询引用
    const refetch = () => {
        if (!focusMode) void queueQuery.refetch()
        if (detailTarget) void detailItemQuery.refetch()
    }

    const actions = useIntegrationActions({
        item,
        focusMode,
        autoNext,
        lastResult,
        setLastResult,
        setActionError,
        userId: profileQuery.data?.userid,
        refetch,
        goToItem,
        neighbor,
        onTaskCompleted: embedded ? onTaskCompleted : undefined,
    })

    const { searchDraft, setSearchDraft, searchInputRef } =
        useIntegrationSearch({
            q: urlState.q,
        })

    const clearQueueFilters = React.useCallback(() => {
        setSearchDraft("")
        replaceUrl({
            mode: "all",
            environment: "all",
            errorClass: null,
            handlerUserIds: null,
            operatorUserIds: null,
            q: null,
            taskId: null,
            differenceId: null,
        })
    }, [replaceUrl, setSearchDraft])

    useIntegrationPageSync({
        focusMode,
        view,
        queuePending: queueQuery.isPending,
        urlState,
        item,
        itemCount: items.length,
        autoNext,
    })

    const defaultItemType = forcedDifferenceId
        ? "RECONCILIATION_DIFFERENCE"
        : "ERROR_TASK"
    const defaultDetailTitle = forcedDifferenceId ? "对账差异" : "外部同步异常"
    const backToQueueHref =
        returnTo ??
        `/governance/integration-errors?view=${urlState.view}&queueContextId=${encodeURIComponent(urlState.queueContextId)}`

    const handleBackToQueue = React.useCallback(() => {
        const host = workspaceHost()
        if (host) {
            host.closeCurrent(
                isWorkspaceFrame() ? window : null,
                backToQueueHref,
            )
            return
        }
        router.push(backToQueueHref)
    }, [backToQueueHref, router])

    const detailUpdatedAt =
        item?.freshness.updatedAt ||
        (detailItemQuery.dataUpdatedAt > 0
            ? new Date(detailItemQuery.dataUpdatedAt).toISOString()
            : undefined)

    const panelErrorClass =
        item && isPanelErrorClass(item.classification.errorClass)
            ? item.classification.errorClass
            : null

    const focusLoading =
        focusMode && detailItemQuery.isPending && !detailItemQuery.data
    const focusError =
        focusMode &&
        (detailItemQuery.isError ||
            (!detailItemQuery.isPending && detailItemQuery.data === null))

    if (queueQuery.isPending && !focusMode) {
        return <IntegrationPageSkeleton />
    }

    if (focusLoading) {
        if (embedded) {
            return (
                <DetailWorkspaceSection>
                    <div
                        className="space-y-4"
                        aria-busy="true"
                        aria-label={`正在加载${defaultDetailTitle}`}
                    >
                        <div className="h-14 animate-pulse rounded-md bg-muted/70" />
                        <div className="h-44 animate-pulse rounded-md bg-muted/70" />
                    </div>
                </DetailWorkspaceSection>
            )
        }
        return <DetailWorkspaceSkeleton title={defaultDetailTitle} sidebar />
    }

    if (queueQuery.isError && !focusMode) {
        return (
            <IntegrationPageFailure
                title="外部同步异常与对账"
                description="加载失败"
                error={queueQuery.error}
                onRetry={() => void queueQuery.refetch()}
            />
        )
    }

    if (focusMode && detailItemQuery.isError) {
        if (embedded) {
            return (
                <DetailWorkspaceSection width="form">
                    <BusinessFailureState
                        idPrefix="integration-page-failure"
                        title={
                            forcedDifferenceId
                                ? "对账差异加载失败"
                                : "异常任务加载失败"
                        }
                        error={detailItemQuery.error}
                        onRetry={() => void detailItemQuery.refetch()}
                    />
                </DetailWorkspaceSection>
            )
        }
        return (
            <IntegrationPageFailure
                title={defaultDetailTitle}
                description={
                    forcedDifferenceId ? "对账差异加载失败" : "异常任务加载失败"
                }
                detail
                backHref={backToQueueHref}
                backAlways={Boolean(returnTo)}
                error={detailItemQuery.error}
                onRetry={() => void detailItemQuery.refetch()}
            />
        )
    }

    if (focusError) {
        if (embedded) {
            return (
                <DetailWorkspaceSection width="form">
                    <BusinessEmptyState
                        kind="no-data"
                        title={
                            forcedDifferenceId
                                ? "未找到该对账差异"
                                : "未找到该异常任务"
                        }
                        description="记录可能已结束或链接失效；请刷新待办重试。"
                        action={
                            <Button
                                id="integration-detail-not-found-retry"
                                type="button"
                                variant="outline"
                                size="sm"
                                onClick={() => void detailItemQuery.refetch()}
                            >
                                重试
                            </Button>
                        }
                    />
                </DetailWorkspaceSection>
            )
        }
        return (
            <IntegrationNotFound
                title={defaultDetailTitle}
                detail
                view={urlState.view}
                queueContextId={urlState.queueContextId}
                backHref={backToQueueHref}
                backAlways={Boolean(returnTo)}
                onRetry={() => void detailItemQuery.refetch()}
            />
        )
    }

    const hasNext = !focusMode && Boolean(neighbor(1))

    const actionResult = (
        <IntegrationActionResult
            lastResult={lastResult}
            actionError={actionError}
            autoNext={autoNext}
            hasNext={hasNext}
            resultRef={actions.resultRef}
            detailWorkspace={focusMode}
            onNext={() => {
                const next = neighbor(1)
                if (next) goToItem(next)
            }}
            onBackToQueue={
                focusMode && !embedded ? handleBackToQueue : undefined
            }
        />
    )
    const workspace = (
        <IntegrationErrorWorkspace
            items={items}
            item={item}
            focusMode={focusMode}
            detailWorkspace={focusMode}
            showEvidenceSection={embedded}
            urlState={urlState}
            currentQueueHref={currentQueueHref}
            positionIndex={positionIndex}
            positionTotal={positionTotal}
            panelErrorClass={panelErrorClass}
            actions={actions}
            goToItem={goToItem}
            neighbor={neighbor}
            replaceUrl={replaceUrl}
            onClearFilters={clearQueueFilters}
            emptyReason={focusMode ? undefined : view?.emptyReason}
        />
    )

    if (embedded) {
        return (
            <div className="min-w-0 [&>[data-slot=detail-workspace-section]+[data-slot=detail-workspace-section]]:border-t [&>[data-slot=detail-workspace-section]+[data-slot=detail-workspace-section]]:border-border/60">
                {item ? (
                    <div className="border-b border-border/60 py-3">
                        <DetailWorkspaceMeta
                            items={[
                                <span key="age">
                                    已滞留{" "}
                                    <span className="num">{item.ageLabel}</span>
                                </span>,
                                item.fundsImpactLabel,
                                item.environment
                                    ? `${item.environmentLabel}环境`
                                    : false,
                            ]}
                        />
                    </div>
                ) : null}
                {lastResult || actionError ? (
                    <div className="space-y-2.5 pt-4">{actionResult}</div>
                ) : null}
                {workspace}
            </div>
        )
    }

    if (focusMode) {
        return (
            <DetailWorkspacePage>
                <IntegrationDetailNav
                    item={item}
                    defaultItemType={defaultItemType}
                    updatedAt={detailUpdatedAt}
                    view={urlState.view}
                    queueContextId={urlState.queueContextId}
                    returnTo={returnTo}
                    headingRef={actions.headingRef}
                    onRefresh={actions.refresh}
                />
                <DetailWorkspaceBody
                    alerts={lastResult || actionError ? actionResult : null}
                    sidebar={
                        item ? (
                            <IntegrationEvidencePanel item={item} sidebar />
                        ) : undefined
                    }
                    sidebarLabel="执行尝试、关联证据与处理审计"
                >
                    {workspace}
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }

    return (
        <PageScaffold density="default" className={styles.page}>
            {!focusMode ? (
                <IntegrationPageHeader
                    focusMode={focusMode}
                    itemNumber={item?.identity.number}
                    updatedAt={view?.context.updatedAt}
                />
            ) : null}

            {metrics && !focusMode ? (
                <IntegrationErrorMetricStrip metrics={metrics} />
            ) : null}

            {!focusMode ? (
                <IntegrationQueueToolbar
                    urlState={urlState}
                    searchDraft={searchDraft}
                    onSearchDraftChange={setSearchDraft}
                    searchInputRef={searchInputRef}
                    autoNext={autoNext}
                    patchUrl={replaceUrl}
                    onClearFilters={clearQueueFilters}
                    resultCount={queueItems.length}
                    loading={queueQuery.isFetching && !queueQuery.isPending}
                    failed={queueQuery.isError}
                />
            ) : null}

            {!focusMode ? (
                <p className="text-xs text-muted-foreground">
                    {view?.scopeSummary ? `${view.scopeSummary} · ` : null}
                    筛选：{view?.context.filterSummary}
                </p>
            ) : null}

            {actionResult}
            {workspace}
        </PageScaffold>
    )
}
