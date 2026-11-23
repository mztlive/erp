"use client"

import { useCallback, useMemo, useState } from "react"
import Link from "next/link"
import { ArrowRightLeftIcon, CircleOffIcon, MailIcon } from "lucide-react"

import { BusinessEmptyState, BusinessFailureState } from "@/components/business"
import {
    ListSelectionBar,
    listWorkspaceEmptyStateClassName,
} from "@/components/business/list-workspace"
import {
    WorkItemResponsibilityDialog,
    type WorkItemResponsibilitySummary,
    type WorkItemResponsibilityTarget,
} from "@/components/business/work-item-responsibility-dialog"
import {
    WorkspaceSplitQueue,
    type WorkspaceQueueLayoutMode,
} from "@/components/business/workspace-split-queue"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { StatusBadge } from "@/components/ui/status-badge"
import { hasPermission } from "@/lib/permissions"
import { workspaceHost } from "@/lib/workspace-tabs"
import { workspaceQueueSubjects } from "../hooks/queries"
import type { useWorkspaceHome } from "../hooks/use-workspace-home"
import { useWorkspaceListMaterials } from "../hooks/use-workspace-list-materials"
import { queueTypeLabel } from "../lib/queue-presentation"
import { workspaceTaskPresentation } from "../lib/task-presentation"
import { buildWorkspaceSearchParams } from "../lib/url-state"
import type { TodayWorkspaceView, WorkspaceWorkItem } from "../types"
import { WorkspaceDocumentBadge } from "./workspace-document-badge"
import { WorkspaceSearchPanel } from "./workspace-search-panel"
import { WorkspaceSortMenu } from "./workspace-sort-menu"
import { WorkspaceTaskInspector } from "./workspace-task-inspector"
import { queueTaskRow } from "./workspace-task-columns"
import {
    canBatchOperateTask,
    WorkspaceTaskList,
    WorkspaceTaskSplitList,
} from "./workspace-task-list"
import { WorkspaceTypeNav } from "./workspace-type-nav"

export function WorkspaceQueuePanel({
    home,
    onSelectTask,
    layoutMode = "table",
    activeInspectorId,
    onActiveInspectorIdChange,
    selectedIds,
    onSelectedIdsChange,
}: {
    home: ReturnType<typeof useWorkspaceHome>
    onSelectTask: (item: WorkspaceWorkItem) => void
    layoutMode?: WorkspaceQueueLayoutMode
    activeInspectorId?: string | null
    onActiveInspectorIdChange?: (next: string | null) => void
    selectedIds?: string[]
    onSelectedIdsChange?: (next: string[]) => void
}) {
    const { view, accountProfileQuery, dashboardQuery, refresh } = home
    if (accountProfileQuery.isError || (dashboardQuery.isError && !view))
        return (
            <BusinessFailureState
                idPrefix="workspace-queue-load-failure"
                error={accountProfileQuery.error ?? dashboardQuery.error}
                readContext={{
                    subject: accountProfileQuery.isError
                        ? "账号资料"
                        : workspaceQueueSubjects[home.urlState.view],
                }}
                onRetry={refresh}
            />
        )
    if (
        accountProfileQuery.data &&
        !hasPermission(
            accountProfileQuery.data.permissions,
            home.urlState.view === "started"
                ? "approval_instance:read"
                : "work_item:list",
        )
    )
        return (
            <section aria-label="我的事项">
                <BusinessEmptyState
                    kind="no-tasks"
                    className={listWorkspaceEmptyStateClassName}
                    title="当前角色没有待办入口"
                    description="可返回工作台，或查看抄送给你的审批。"
                    action={
                        accountProfileQuery.data.account_kind === "admin" ? (
                            <Button
                                id="workspace-copy-link"
                                variant="outline"
                                render={<Link href="/workspace/copies" />}
                            >
                                <MailIcon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                />
                                抄送给我
                            </Button>
                        ) : undefined
                    }
                />
            </section>
        )
    if (!view)
        return (
            <div aria-busy="true" className="flex flex-col gap-3">
                <span role="status" className="sr-only">
                    正在读取事项…
                </span>
                <Skeleton className="h-10 w-full" />
                <Skeleton className="h-96 w-full rounded-none" />
            </div>
        )
    return (
        <WorkspaceQueueContent
            home={home}
            view={view}
            onSelectTask={onSelectTask}
            layoutMode={layoutMode}
            controlledInspectorId={activeInspectorId}
            onControlledInspectorIdChange={onActiveInspectorIdChange}
            controlledSelectedIds={selectedIds}
            onControlledSelectedIdsChange={onSelectedIdsChange}
        />
    )
}

/**
 * 统一待办/审批/历史队列内容壳：
 * - 支持通栏紧凑表与 Master-Detail 分栏快审双向切换；
 * - 在 `/workspace/tasks`（含 `managed` 范围）挂载多选复选框与底部悬浮 `ListSelectionBar`，
 *   直连 `WorkItemResponsibilityDialog` 执行批量转交与批量关闭。
 */
function WorkspaceQueueContent({
    home,
    view,
    onSelectTask,
    layoutMode,
    controlledInspectorId,
    onControlledInspectorIdChange,
    controlledSelectedIds,
    onControlledSelectedIdsChange,
}: {
    home: ReturnType<typeof useWorkspaceHome>
    view: TodayWorkspaceView
    onSelectTask: (item: WorkspaceWorkItem) => void
    layoutMode: WorkspaceQueueLayoutMode
    controlledInspectorId?: string | null
    onControlledInspectorIdChange?: (next: string | null) => void
    controlledSelectedIds?: string[]
    onControlledSelectedIdsChange?: (next: string[]) => void
}) {
    const {
        dashboardQuery,
        urlState,
        approvalView,
        queueCounts,
        refreshing,
        hasActiveFilter,
        clearFilters,
        onSortChange,
        applyFilters,
        onPageChange,
        refresh,
    } = home
    const options = view.filterOptions ?? {}
    const filters = urlState.filters ?? {}
    const selectedTypes = filters.item_types ?? []
    const started = urlState.view === "started"
    const history = urlState.view === "history"
    const materials = useWorkspaceListMaterials(view.items)
    const rows = useMemo(
        () =>
            materials.map(({ item, materialNotice, retryMaterials }) =>
                queueTaskRow(item, materialNotice, retryMaterials),
            ),
        [materials],
    )
    const selectedLabel =
        selectedTypes.length === 1
            ? options.item_types?.find(
                  (option) => option.value === selectedTypes[0],
              )
            : undefined
    const title = selectedLabel
        ? queueTypeLabel(selectedLabel)
        : selectedTypes.length
          ? "已选事项"
          : "全部事项"

    const [uncontrolledInspectorId, setUncontrolledInspectorId] = useState<
        string | null
    >(null)
    const [uncontrolledSelectedIds, setUncontrolledSelectedIds] = useState<
        string[]
    >([])
    const activeInspectorId =
        controlledInspectorId !== undefined
            ? controlledInspectorId
            : uncontrolledInspectorId
    const setActiveInspectorId = useCallback(
        (next: string | null) => {
            if (controlledInspectorId === undefined) {
                setUncontrolledInspectorId(next)
            }
            onControlledInspectorIdChange?.(next)
        },
        [controlledInspectorId, onControlledInspectorIdChange],
    )
    const rawSelectedIds =
        controlledSelectedIds !== undefined
            ? controlledSelectedIds
            : uncontrolledSelectedIds
    const setRawSelectedIds = useCallback(
        (next: string[] | ((prev: string[]) => string[])) => {
            const resolved =
                typeof next === "function" ? next(rawSelectedIds) : next
            if (controlledSelectedIds === undefined) {
                setUncontrolledSelectedIds(resolved)
            }
            onControlledSelectedIdsChange?.(resolved)
        },
        [controlledSelectedIds, onControlledSelectedIdsChange, rawSelectedIds],
    )
    const [batchDialogMode, setBatchDialogMode] = useState<
        "reassign" | "close" | null
    >(null)

    const scrollKey = buildWorkspaceSearchParams(urlState)

    const allowBatch = !started && !history
    const operableItems = useMemo(
        () => (allowBatch ? view.items.filter(canBatchOperateTask) : []),
        [allowBatch, view.items],
    )
    const anyReassignable = useMemo(
        () =>
            allowBatch &&
            view.items.some(
                (item) =>
                    item.status === "OPEN" &&
                    item.allowedActions.includes("REASSIGN"),
            ),
        [allowBatch, view.items],
    )
    const anyClosable = useMemo(
        () =>
            allowBatch &&
            view.items.some(
                (item) =>
                    item.status === "OPEN" &&
                    item.allowedActions.includes("CLOSE"),
            ),
        [allowBatch, view.items],
    )
    const enableBatchSelection = anyReassignable || anyClosable

    const visibleSelectedIds = useMemo(() => {
        if (!enableBatchSelection || rawSelectedIds.length === 0) return []
        const validSet = new Set(operableItems.map((item) => item.workItemId))
        return rawSelectedIds.filter((id) => validSet.has(id))
    }, [enableBatchSelection, operableItems, rawSelectedIds])

    const visibleSelectedItems = useMemo(() => {
        if (visibleSelectedIds.length === 0) return []
        const idSet = new Set(visibleSelectedIds)
        return view.items.filter((item) => idSet.has(item.workItemId))
    }, [view.items, visibleSelectedIds])

    const allSelectedCanReassign =
        visibleSelectedItems.length > 0 &&
        visibleSelectedItems.every(
            (item) =>
                item.status === "OPEN" &&
                item.allowedActions.includes("REASSIGN"),
        )
    const allSelectedCanClose =
        visibleSelectedItems.length > 0 &&
        visibleSelectedItems.every(
            (item) =>
                item.status === "OPEN" && item.allowedActions.includes("CLOSE"),
        )

    const responsibilityTargets = useMemo<WorkItemResponsibilityTarget[]>(
        () =>
            visibleSelectedItems.map((item) => ({
                workItemId: item.workItemId,
                label:
                    [item.counterpartyName, item.stableNumber]
                        .filter(Boolean)
                        .join(" · ") || item.objectTitle,
                expectedTaskVersion: item.taskVersion,
                currentOwnerLabel: item.ownerUserLabel,
            })),
        [visibleSelectedItems],
    )

    const handleInspectItem = useCallback(
        (item: WorkspaceWorkItem) => {
            setActiveInspectorId(item.workItemId)
        },
        [setActiveInspectorId],
    )

    const advanceAfterCompletion = useCallback(
        (
            completedId: string,
            preferredId: string | undefined,
            autoNext: boolean,
        ) => {
            if (!autoNext) {
                setActiveInspectorId(null)
                return
            }
            const currentIndex = view.items.findIndex(
                (entry) => entry.workItemId === completedId,
            )
            const next =
                (preferredId && preferredId !== completedId
                    ? view.items.find(
                          (entry) => entry.workItemId === preferredId,
                      )
                    : undefined) ??
                view.items
                    .slice(currentIndex + 1)
                    .find((entry) => entry.workItemId !== completedId) ??
                view.items.find((entry) => entry.workItemId !== completedId)
            setActiveInspectorId(next ? next.workItemId : null)
        },
        [setActiveInspectorId, view.items],
    )

    const handleBatchCompleted = useCallback(
        (summary: WorkItemResponsibilitySummary) => {
            if (summary.succeededIds.length === 0) return
            const succeeded = new Set(summary.succeededIds)
            setRawSelectedIds((prev) => prev.filter((id) => !succeeded.has(id)))
            if (activeInspectorId && succeeded.has(activeInspectorId)) {
                setActiveInspectorId(null)
            }
            workspaceHost()?.changed(window)
            refresh()
        },
        [activeInspectorId, refresh, setActiveInspectorId, setRawSelectedIds],
    )

    const selectionSummaryExtra = useMemo(() => {
        if (visibleSelectedItems.length === 0) return undefined
        const hints: string[] = []
        if (anyReassignable && !allSelectedCanReassign) {
            hints.push("含不可转交项")
        }
        if (anyClosable && !allSelectedCanClose) {
            hints.push("含不可关闭项")
        }
        return hints.length > 0 ? hints.join(" · ") : undefined
    }, [
        allSelectedCanClose,
        allSelectedCanReassign,
        anyClosable,
        anyReassignable,
        visibleSelectedItems.length,
    ])

    const selectionBar = enableBatchSelection ? (
        <ListSelectionBar
            idPrefix="workspace-task-selection"
            resultCount={view.total}
            pageRowCount={operableItems.length}
            selectedCount={visibleSelectedIds.length}
            allPageSelected={
                operableItems.length > 0 &&
                visibleSelectedIds.length === operableItems.length
            }
            clearLabel="清空选择"
            summaryExtra={selectionSummaryExtra}
            onSelectPage={() =>
                setRawSelectedIds(operableItems.map((item) => item.workItemId))
            }
            onClear={() => setRawSelectedIds([])}
            actions={(display) => (
                <>
                    {anyReassignable ? (
                        <Button
                            id="workspace-task-batch-reassign"
                            type="button"
                            size="sm"
                            disabled={
                                !display.selectedCount ||
                                !allSelectedCanReassign
                            }
                            title={
                                !allSelectedCanReassign &&
                                visibleSelectedIds.length > 0
                                    ? "所选任务中有部分不支持转交，请仅勾选可转交任务"
                                    : "将所选任务转交给新的责任人"
                            }
                            onClick={() => setBatchDialogMode("reassign")}
                        >
                            <ArrowRightLeftIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            {display.selectedCount > 1
                                ? "批量转交"
                                : "转交任务"}
                        </Button>
                    ) : null}
                    {anyClosable ? (
                        <Button
                            id="workspace-task-batch-close"
                            type="button"
                            size="sm"
                            variant={anyReassignable ? "outline" : "default"}
                            disabled={
                                !display.selectedCount || !allSelectedCanClose
                            }
                            title={
                                !allSelectedCanClose &&
                                visibleSelectedIds.length > 0
                                    ? "所选任务中有部分不支持关闭，请仅勾选可关闭的异常任务"
                                    : "受控关闭所选异常任务"
                            }
                            onClick={() => setBatchDialogMode("close")}
                        >
                            <CircleOffIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            {display.selectedCount > 1
                                ? "批量关闭"
                                : "关闭任务"}
                        </Button>
                    ) : null}
                </>
            )}
        />
    ) : undefined

    const refreshFailureBanner = dashboardQuery.isError ? (
        <div className="shrink-0 px-(--scaffold-px) py-workspace-toolbar-block">
            <BusinessFailureState
                idPrefix="workspace-queue-refresh-failure"
                error={dashboardQuery.error}
                title={`${workspaceQueueSubjects[urlState.view]}刷新失败`}
                readContext={{
                    subject: workspaceQueueSubjects[urlState.view],
                    impact: "当前显示上次加载的列表。",
                }}
                onRetry={refresh}
            />
        </div>
    ) : null

    const getInspectorHeader = useCallback(
        (item: WorkspaceWorkItem) => {
            const row = rows.find(
                (entry) => entry.item.workItemId === item.workItemId,
            )
            const meta =
                row && !row.materialNotice
                    ? row.presentation
                    : workspaceTaskPresentation(item)
            return {
                title: meta.title,
                subtitle:
                    meta.number !== "—" && meta.number !== meta.title ? (
                        <span className="num select-all">{meta.number}</span>
                    ) : undefined,
                badges: (
                    <>
                        <WorkspaceDocumentBadge item={item} />
                        <StatusBadge
                            label={item.statusLabel}
                            tone={item.statusTone}
                        />
                    </>
                ),
                openFullPageLabel: "全屏打开",
            }
        },
        [rows],
    )

    return (
        <>
            <WorkspaceSplitQueue
                idPrefix="workspace-queue"
                ariaLabel="我的事项"
                layoutMode={layoutMode}
                items={view.items}
                getItemId={(item) => item.workItemId}
                selectedId={activeInspectorId}
                onSelectedIdChange={setActiveInspectorId}
                onOpenFullPage={onSelectTask}
                toolbarClassName="py-1.5 [&>[data-slot=table-toolbar-inline-controls]]:self-center"
                sheetBodyClassName="flex flex-col overflow-hidden p-0"
                views={
                    <WorkspaceTypeNav
                        state={urlState}
                        options={options}
                        counts={queueCounts.counts}
                        onChange={(item_types) =>
                            applyFilters(urlState.query ?? "", {
                                ...filters,
                                item_types: item_types.length
                                    ? item_types
                                    : undefined,
                            })
                        }
                    />
                }
                toolbar={
                    <WorkspaceSearchPanel
                        state={urlState}
                        approvalView={started ? approvalView : undefined}
                        options={options}
                        counts={queueCounts.counts}
                        countsFailureTitle={queueCounts.failure?.title}
                        onRetryCounts={queueCounts.retry}
                        total={view.total}
                        loading={refreshing}
                        failed={dashboardQuery.isError}
                        hasActiveFilters={hasActiveFilter}
                        onApply={applyFilters}
                        onReset={clearFilters}
                        onScopeChange={home.onMetricClick}
                    />
                }
                tableActions={
                    !started && (
                        <WorkspaceSortMenu
                            value={urlState.sort}
                            onValueChange={onSortChange}
                        />
                    )
                }
                selectionBar={selectionBar}
                selectionBarOpen={visibleSelectedIds.length > 0}
                tableView={
                    <>
                        {refreshFailureBanner}
                        <section
                            data-slot="workspace-queue"
                            aria-label="任务队列"
                            className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden"
                        >
                            <h2
                                id="workspace-queue-heading"
                                tabIndex={-1}
                                className="sr-only"
                            >
                                {title}
                            </h2>
                            <WorkspaceTaskList
                                rows={rows}
                                onSelect={onSelectTask}
                                onInspect={handleInspectItem}
                                activeId={activeInspectorId}
                                tracking={started}
                                approvalView={
                                    started ? approvalView : undefined
                                }
                                history={history}
                                selectedTypes={selectedTypes}
                                scrollKey={scrollKey}
                                total={view.total}
                                page={urlState.page ?? 1}
                                onPageChange={onPageChange}
                                loading={refreshing}
                                noScope={view.access === "no_data_scope"}
                                hasActiveFilters={hasActiveFilter}
                                onClearFilters={clearFilters}
                                enableBatchSelection={enableBatchSelection}
                                selectedIds={visibleSelectedIds}
                                onSelectedIdsChange={setRawSelectedIds}
                            />
                        </section>
                    </>
                }
                splitListView={
                    <>
                        {refreshFailureBanner}
                        <section
                            data-slot="workspace-queue"
                            aria-label="任务队列"
                            className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden"
                        >
                            <h2
                                id="workspace-queue-heading"
                                tabIndex={-1}
                                className="sr-only"
                            >
                                {title}
                            </h2>
                            <WorkspaceTaskSplitList
                                rows={rows}
                                onSelect={onSelectTask}
                                onInspect={handleInspectItem}
                                activeId={activeInspectorId}
                                tracking={started}
                                approvalView={
                                    started ? approvalView : undefined
                                }
                                history={history}
                                scrollKey={scrollKey}
                                total={view.total}
                                page={urlState.page ?? 1}
                                onPageChange={onPageChange}
                                loading={refreshing}
                                noScope={view.access === "no_data_scope"}
                                hasActiveFilters={hasActiveFilter}
                                onClearFilters={clearFilters}
                                enableBatchSelection={enableBatchSelection}
                                selectedIds={visibleSelectedIds}
                                onSelectedIdsChange={setRawSelectedIds}
                            />
                        </section>
                    </>
                }
                getInspectorHeader={getInspectorHeader}
                renderInspector={(ctx) => (
                    <WorkspaceTaskInspector
                        key={ctx.id}
                        item={ctx.item}
                        onOpenFullPage={ctx.onOpenFullPage}
                        onCompleted={advanceAfterCompletion}
                        presentation={ctx.presentation}
                    />
                )}
            />

            {batchDialogMode && responsibilityTargets.length > 0 ? (
                <WorkItemResponsibilityDialog
                    mode={batchDialogMode}
                    targets={responsibilityTargets}
                    open
                    onOpenChange={(nextOpen) => {
                        if (!nextOpen) setBatchDialogMode(null)
                    }}
                    idPrefix={
                        batchDialogMode === "reassign"
                            ? "workspace-task-batch-reassign"
                            : "workspace-task-batch-close"
                    }
                    onCompleted={handleBatchCompleted}
                />
            ) : null}
        </>
    )
}
