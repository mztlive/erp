"use client"

import { useMemo } from "react"
import type { ColumnDef, RowSelectionState } from "@tanstack/react-table"
import {
    CalendarClockIcon,
    ClockAlertIcon,
    PanelRightOpenIcon,
} from "lucide-react"

import { BusinessStatusBadge } from "@/components/business"
import { ListDataTable } from "@/components/business/list-workspace"
import { WorkspaceSplitCardList } from "@/components/business/workspace-split-queue"
import { Button } from "@/components/ui/button"
import type { ApprovalInstanceListView } from "@/features/approval-workflow/api"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { displayName } from "@/lib/display-name"
import {
    getApprovalInstanceStatusMeta,
    getWorkItemStatusMeta,
} from "@/lib/task-status-dictionary"
import { cn } from "@/lib/utils"
import { useWorkspaceListMaterials } from "../hooks/use-workspace-list-materials"
import { useWorkspaceScroll } from "../hooks/use-workspace-scroll"
import { isBlockedWorkItem } from "../lib/work-item"
import type { WorkspaceWorkItem } from "../types"
import { WorkspaceDocumentBadge } from "./workspace-document-badge"
import { WorkspaceMoney } from "./workspace-money"
import {
    queueTaskRow,
    workspaceTaskColumns,
    type QueueTaskRow,
} from "./workspace-task-columns"

const PAGE_SIZE = 50

const APPROVAL_VIEW_EMPTY_COPY: Record<
    ApprovalInstanceListView,
    {
        caption: string
        emptyTitle: string
        emptyDescription: string
        noScopeTitle?: string
        noScopeDescription?: string
    }
> = {
    started: {
        caption: "已发起的审批",
        emptyTitle: "暂无已发起的审批",
        emptyDescription: "已提交的审批会显示在这里。",
    },
    mine: {
        caption: "待我审批",
        emptyTitle: "当前没有待审批事项",
        emptyDescription: "流转到你名下的审批单会显示在这里。",
    },
    blocked: {
        caption: "受阻审批治理",
        emptyTitle: "当前没有受阻审批",
        emptyDescription: "需要治理的受阻审批实例会显示在这里。",
        noScopeTitle: "暂无受阻审批治理范围",
        noScopeDescription:
            "当前账号尚未配置审批管理范围，无法查看或治理组织内受阻审批。",
    },
    managed: {
        caption: "管理范围审批",
        emptyTitle: "管理范围内暂无审批记录",
        emptyDescription: "所辖组织的审批实例会显示在这里。",
        noScopeTitle: "暂无管理范围审批查看权限",
        noScopeDescription:
            "当前账号尚未配置审批管理范围，如需查看请联系管理员开通。",
    },
}

/** 仅开放状态且具备 REASSIGN 或 CLOSE 权限的任务允许勾选参与批量责任治理。 */
export function canBatchOperateTask(item: WorkspaceWorkItem): boolean {
    return (
        item.status === "OPEN" &&
        (item.allowedActions.includes("REASSIGN") ||
            item.allowedActions.includes("CLOSE"))
    )
}

function resolveQueueCopy({
    tracking,
    approvalView,
    history,
}: {
    tracking: boolean
    approvalView?: ApprovalInstanceListView
    history: boolean
}) {
    const approvalCopy =
        tracking && approvalView
            ? APPROVAL_VIEW_EMPTY_COPY[approvalView]
            : undefined
    return {
        caption:
            approvalCopy?.caption ??
            (tracking ? "已发起的审批" : history ? "处理记录" : "待办事项"),
        emptyCopy: {
            filterTitle: "没有符合条件的事项",
            noScopeTitle: approvalCopy?.noScopeTitle,
            noScopeDescription: approvalCopy?.noScopeDescription,
            emptyTitle:
                approvalCopy?.emptyTitle ??
                (tracking
                    ? "暂无已发起的审批"
                    : history
                      ? "暂无处理记录"
                      : "当前没有待处理事项"),
            emptyDescription:
                approvalCopy?.emptyDescription ??
                (tracking
                    ? "已提交的审批会显示在这里。"
                    : history
                      ? "处理过的事项会显示在这里。"
                      : "新事项到达后会显示在这里。"),
        },
    }
}

export type WorkspaceTaskListProps = Readonly<{
    items: readonly WorkspaceWorkItem[]
    onSelect: (item: WorkspaceWorkItem) => void
    /** 原位快览回调：在通栏表格模式下点击行或「快览」按钮唤起检查器。 */
    onInspect?: (item: WorkspaceWorkItem) => void
    /** 当前正在检查器中查看的工作项 ID，用于高亮对应行。 */
    activeId?: string | null
    tracking?: boolean
    approvalView?: ApprovalInstanceListView
    history?: boolean
    selectedTypes: readonly string[]
    scrollKey: string
    total: number
    page: number
    onPageChange: (page: number) => void
    loading: boolean
    noScope: boolean
    hasActiveFilters: boolean
    onClearFilters: () => void
    /** 启用多选复选框（仅待我处理 / 范围内待办且存在可转交或可关闭项时开启）。 */
    enableBatchSelection?: boolean
    selectedIds?: readonly string[]
    onSelectedIdsChange?: (ids: string[]) => void
}>

/**
 * 紧凑单行通栏表格（`density="compact"`，36px 行高）：
 * - 全部 4 个审批视图与任务队列统一走页码分页；
 * - 单页队列（`pageCount <= 1`）自动隐藏底部分页条；
 * - 行内提供原位「快览」与完整页「处理/查看」双入口，按 Enter 或点主动作直达独立工作区页签；
 * - 支持多选复选框（配合底部悬浮 `ListSelectionBar` 执行批量转交/关闭）。
 */
export function WorkspaceTaskList({
    items,
    onSelect,
    onInspect,
    activeId,
    tracking = false,
    approvalView,
    history = false,
    selectedTypes,
    scrollKey,
    total,
    page,
    onPageChange,
    loading,
    noScope,
    hasActiveFilters,
    onClearFilters,
    enableBatchSelection = false,
    selectedIds = [],
    onSelectedIdsChange,
}: WorkspaceTaskListProps) {
    const scrollRef = useWorkspaceScroll<HTMLDivElement>(
        `queue:${scrollKey}`,
        "[data-slot=table-container]",
    )
    const materials = useWorkspaceListMaterials(items)
    const rows = useMemo(
        () =>
            materials.map(({ item, materialNotice, retryMaterials }) =>
                queueTaskRow(item, materialNotice, retryMaterials),
            ),
        [materials],
    )

    const columns = useMemo<ColumnDef<QueueTaskRow, unknown>[]>(() => {
        const base = workspaceTaskColumns({
            selectedTypes,
            tracking,
            history,
            onSelect,
        })
        if (!onInspect) return base
        return base.map((column) => {
            if (column.id !== "action") return column
            const originalCell = column.cell
            return {
                ...column,
                cell: (context) => {
                    const { item, presentation } = context.row.original
                    const segment = toAutomationIdSegment(item.workItemId)
                    return (
                        <div className="inline-flex items-center justify-end gap-0.5">
                            <Button
                                id={`workspace-task-inspect-${segment}`}
                                type="button"
                                variant="ghost"
                                size="icon-xs"
                                className="size-control-sm text-muted-foreground/80 opacity-80 hover:bg-sidebar-accent/70 hover:text-primary hover:opacity-100"
                                aria-label={`快览${presentation.number}`}
                                title="原位快览"
                                onClick={(event) => {
                                    event.stopPropagation()
                                    onInspect(item)
                                }}
                            >
                                <PanelRightOpenIcon
                                    className="size-3.5"
                                    aria-hidden="true"
                                />
                            </Button>
                            {typeof originalCell === "function"
                                ? originalCell(context)
                                : originalCell}
                        </div>
                    )
                },
            }
        })
    }, [history, onInspect, onSelect, selectedTypes, tracking])

    const rowSelection = useMemo<RowSelectionState>(
        () => Object.fromEntries(selectedIds.map((id) => [id, true])),
        [selectedIds],
    )

    const { caption, emptyCopy } = resolveQueueCopy({
        tracking,
        approvalView,
        history,
    })
    const pageCount = Math.ceil(total / PAGE_SIZE)

    return (
        <div ref={scrollRef} className="flex min-h-0 min-w-0 flex-1 flex-col">
            <ListDataTable
                id="workspace-task-table"
                layout="flush"
                density="compact"
                data={rows}
                columns={columns}
                getRowId={(row) => row.item.workItemId}
                rowLabel={(row) =>
                    `${row.presentation.typeLabel} ${row.presentation.number} ${row.presentation.title}`
                }
                caption={caption}
                rowCount={total}
                defaultColumnVisibility={{ source: false }}
                defaultColumnPinning={{ left: ["number"], right: ["action"] }}
                scrollable
                enableRowSelection={
                    enableBatchSelection
                        ? (row) => canBatchOperateTask(row.original.item)
                        : false
                }
                rowSelection={enableBatchSelection ? rowSelection : undefined}
                onRowSelectionChange={
                    enableBatchSelection && onSelectedIdsChange
                        ? (selection) =>
                              onSelectedIdsChange(
                                  Object.keys(selection).filter(
                                      (id) => selection[id],
                                  ),
                              )
                        : undefined
                }
                highlightedRowId={activeId ?? undefined}
                showToolbarSummary={false}
                showPagination={pageCount > 1}
                pagination={{ pageIndex: page - 1, pageSize: PAGE_SIZE }}
                pageSizeOptions={[PAGE_SIZE]}
                onPaginationChange={(next) => onPageChange(next.pageIndex + 1)}
                onRowPreview={
                    onInspect ? (row) => onInspect(row.item) : undefined
                }
                onRowOpen={(row) => onSelect(row.item)}
                loading={loading}
                entityName="事项"
                noScope={noScope}
                hasActiveFilters={hasActiveFilters}
                onClearFilters={noScope ? undefined : onClearFilters}
                emptyCopy={emptyCopy}
            />
        </div>
    )
}

function SplitCardDeadline({ item }: { item: WorkspaceWorkItem }) {
    if (!item.dueAt || Number.isNaN(new Date(item.dueAt).getTime())) {
        if (item.enteredAtLabel && item.enteredAtLabel !== "—") {
            return (
                <span className="num shrink-0 text-tiny text-muted-foreground">
                    {item.enteredAtLabel}
                </span>
            )
        }
        return null
    }
    const urgent = item.status === "OPEN" && item.dueBucket !== "later"
    if (urgent) {
        const overdue = item.dueBucket === "overdue"
        return (
            <BusinessStatusBadge
                appearance="text"
                icon={overdue ? ClockAlertIcon : CalendarClockIcon}
                label={overdue ? "超期" : "今日"}
                tone={overdue ? "destructive" : "warning"}
                className="shrink-0"
            />
        )
    }
    const label = new Intl.DateTimeFormat("zh-CN", {
        timeZone: "Asia/Shanghai",
        month: "2-digit",
        day: "2-digit",
    }).format(new Date(item.dueAt))
    return (
        <time
            dateTime={item.dueAt}
            className="num shrink-0 text-tiny text-muted-foreground"
        >
            {label}
        </time>
    )
}

function SplitCardStatus({
    item,
    tracking,
}: {
    item: WorkspaceWorkItem
    tracking: boolean
}) {
    const closed = item.status !== "OPEN"
    const blocked = isBlockedWorkItem(item)
    const node = displayName(item.approval?.currentNodeLabel)
    const approvalMeta = tracking
        ? getApprovalInstanceStatusMeta(item.approval?.status)
        : undefined
    const workItemMeta = getWorkItemStatusMeta(
        item.status,
        blocked
            ? item.processingState === "READY"
                ? "APPROVAL_BLOCKED"
                : item.processingState
            : undefined,
    )
    const tone = tracking ? approvalMeta!.tone : workItemMeta.tone
    const label = tracking
        ? approvalMeta!.label
        : closed || blocked
          ? workItemMeta.label
          : node || workItemMeta.label

    return (
        <BusinessStatusBadge
            label={label}
            tone={tone}
            appearance={
                tone === "destructive" || tone === "warning" ? "badge" : "quiet"
            }
            className="shrink-0"
        />
    )
}

/**
 * 分栏左栏 2 行紧凑摘要卡列表（每条 ~54px 高，符合 `AGENTS.md §8` 不显示列设置）。
 */
export function WorkspaceTaskSplitList({
    items,
    onSelect,
    onInspect,
    activeId,
    tracking = false,
    approvalView,
    history = false,
    scrollKey,
    total,
    page,
    onPageChange,
    loading,
    noScope,
    hasActiveFilters,
    onClearFilters,
    enableBatchSelection = false,
    selectedIds = [],
    onSelectedIdsChange,
}: Omit<WorkspaceTaskListProps, "selectedTypes">) {
    const scrollRef = useWorkspaceScroll<HTMLDivElement>(
        `split-queue:${scrollKey}`,
        "[data-slot=table-container]",
    )
    const materials = useWorkspaceListMaterials(items)
    const rows = useMemo(
        () =>
            materials.map(({ item, materialNotice, retryMaterials }) =>
                queueTaskRow(item, materialNotice, retryMaterials),
            ),
        [materials],
    )
    const { caption, emptyCopy } = resolveQueueCopy({
        tracking,
        approvalView,
        history,
    })

    return (
        <div ref={scrollRef} className="flex min-h-0 min-w-0 flex-1 flex-col">
            <WorkspaceSplitCardList
                idPrefix="workspace-task-split"
                ariaLabel={caption}
                items={rows}
                getItemId={(row) => row.item.workItemId}
                getItemLabel={(row) =>
                    `${row.presentation.typeLabel} ${row.presentation.number} ${row.presentation.title}`
                }
                selectedId={activeId ?? null}
                onSelect={(row) => (onInspect ?? onSelect)(row.item)}
                onOpenFullPage={(row) => onSelect(row.item)}
                enableSelection={enableBatchSelection}
                canSelectItem={(row) => canBatchOperateTask(row.item)}
                selectedIds={selectedIds}
                onSelectedIdsChange={onSelectedIdsChange}
                loading={loading}
                noScope={noScope}
                hasActiveFilters={hasActiveFilters}
                onClearFilters={noScope ? undefined : onClearFilters}
                emptyCopy={emptyCopy}
                total={total}
                page={page}
                pageSize={PAGE_SIZE}
                onPageChange={onPageChange}
                renderCard={(row) => {
                    const metric = row.content.metric
                    const hasAmount =
                        !row.materialNotice &&
                        metric.value &&
                        metric.value !== "—"
                    return (
                        <div className="flex min-w-0 flex-col gap-1">
                            <div className="flex min-w-0 items-center justify-between gap-2">
                                <div className="flex min-w-0 items-center gap-1.5">
                                    <span className="num truncate text-body-compact font-medium text-foreground">
                                        {row.presentation.number !== "—"
                                            ? row.presentation.number
                                            : row.presentation.title}
                                    </span>
                                    <WorkspaceDocumentBadge
                                        item={row.item}
                                        decorative
                                        className="px-1.5 py-0 text-tiny"
                                    />
                                </div>
                                <div className="flex shrink-0 items-center gap-2">
                                    {hasAmount ? (
                                        <span className="num text-xs font-medium text-foreground">
                                            <WorkspaceMoney
                                                value={metric.value}
                                                display="exact"
                                            />
                                        </span>
                                    ) : null}
                                    <SplitCardDeadline item={row.item} />
                                </div>
                            </div>

                            <div className="flex min-w-0 items-center justify-between gap-2">
                                <span
                                    className={cn(
                                        "min-w-0 truncate text-xs text-muted-foreground",
                                        row.presentation.attention &&
                                            "font-medium text-warning-soft-foreground",
                                    )}
                                    title={
                                        [
                                            row.presentation.title,
                                            row.summaryPreview,
                                        ]
                                            .filter(Boolean)
                                            .join(" · ") || undefined
                                    }
                                >
                                    {row.presentation.title}
                                    {row.summaryPreview
                                        ? ` · ${row.summaryPreview}`
                                        : ""}
                                </span>
                                <SplitCardStatus
                                    item={row.item}
                                    tracking={tracking}
                                />
                            </div>
                        </div>
                    )
                }}
            />
        </div>
    )
}
