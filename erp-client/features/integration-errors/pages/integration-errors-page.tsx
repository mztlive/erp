"use client"

import * as React from "react"
import { PageScaffold } from "@/components/business"

import { useAccountProfileQuery } from "@/features/auth/queries"
import { useIntegrationItemQuery, useIntegrationQueueQuery } from "../hooks/queries"
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
}: {
    forcedTaskId?: string
    forcedDifferenceId?: string
} = {}) {
    const {
        urlState,
        currentTaskId,
        currentDifferenceId,
        focusMode,
        query,
        replaceUrl,
        autoNext,
        hasQueueFilters,
    } = useIntegrationPageUrl({ forcedTaskId, forcedDifferenceId })

    const queueQuery = useIntegrationQueueQuery(query)
    const profileQuery = useAccountProfileQuery()

    const view = queueQuery.data
    const queueItems = React.useMemo(() => view?.items ?? [], [view?.items])
    const metrics = view?.metrics

    const queueSelection = React.useMemo(
        () =>
            selectQueueSelection(queueItems, currentTaskId, currentDifferenceId),
        [queueItems, currentTaskId, currentDifferenceId],
    )

    const detailTarget = React.useMemo(
        () =>
            resolveDetailTarget(forcedTaskId, forcedDifferenceId, queueSelection),
        [forcedTaskId, forcedDifferenceId, queueSelection],
    )

    const detailItemQuery = useIntegrationItemQuery({
        itemType: detailTarget?.itemType ?? "ERROR_TASK",
        id: detailTarget?.id ?? "",
        enabled: detailTarget !== null,
    })

    const item = React.useMemo(
        () => resolveDisplayItem(detailTarget, detailItemQuery.data, queueSelection),
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
        void queueQuery.refetch()
        if (focusMode) void detailItemQuery.refetch()
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
    })

    const { searchDraft, setSearchDraft, searchInputRef } = useIntegrationSearch({
        q: urlState.q,
        onCommitSearch: React.useCallback(
            (q: string | null) =>
                replaceUrl({ q, taskId: null, differenceId: null }),
            [replaceUrl],
        ),
    })

    const clearQueueFilters = React.useCallback(() => {
        setSearchDraft("")
        replaceUrl({
            mode: "all",
            environment: "production",
            errorClass: null,
            owner: "me",
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
        return <IntegrationPageSkeleton focus />
    }

    if (queueQuery.isError && !focusMode) {
        return (
            <IntegrationPageFailure
                title="接口错误与对账中心"
                description="加载失败"
                error={queueQuery.error}
                onRetry={() => void queueQuery.refetch()}
            />
        )
    }

    if (focusMode && detailItemQuery.isError) {
        return (
            <IntegrationPageFailure
                title="接口错误与对账中心"
                description="任务加载失败"
                error={detailItemQuery.error}
                onRetry={() => void detailItemQuery.refetch()}
            />
        )
    }

    if (focusError) {
        return (
            <IntegrationNotFound
                view={urlState.view}
                queueContextId={urlState.queueContextId}
                onRetry={() => void detailItemQuery.refetch()}
            />
        )
    }

    return (
        <PageScaffold>
            <IntegrationPageHeader
                focusMode={focusMode}
                itemNumber={item?.identity.number}
                updatedAt={view?.context.updatedAt}
            />

            {metrics ? (
                <IntegrationErrorMetricStrip
                    metrics={metrics}
                    activeView={urlState.view}
                    focusMode={focusMode}
                    onSelectView={(view) =>
                        replaceUrl({ view, taskId: null, differenceId: null })
                    }
                />
            ) : null}

            {!focusMode ? (
                <IntegrationQueueToolbar
                    urlState={urlState}
                    searchDraft={searchDraft}
                    onSearchDraftChange={setSearchDraft}
                    searchInputRef={searchInputRef}
                    autoNext={autoNext}
                    hasQueueFilters={hasQueueFilters}
                    patchUrl={replaceUrl}
                    onClearFilters={clearQueueFilters}
                />
            ) : (
                <IntegrationDetailNav
                    view={urlState.view}
                    queueContextId={urlState.queueContextId}
                    onRefresh={actions.refresh}
                />
            )}

            {!focusMode ? (
                <p className="text-xs text-muted-foreground">
                    筛选：{view?.context.filterSummary}
                </p>
            ) : null}

            <IntegrationActionResult
                lastResult={lastResult}
                actionError={actionError}
                autoNext={autoNext}
                resultRef={actions.resultRef}
                onNext={() => {
                    const next = neighbor(1)
                    if (next) goToItem(next)
                }}
            />

            <IntegrationErrorWorkspace
                items={items}
                item={item}
                focusMode={focusMode}
                urlState={urlState}
                positionIndex={positionIndex}
                positionTotal={positionTotal}
                panelErrorClass={panelErrorClass}
                actions={actions}
                goToItem={goToItem}
                neighbor={neighbor}
                replaceUrl={replaceUrl}
                onClearFilters={clearQueueFilters}
            />
        </PageScaffold>
    )
}
