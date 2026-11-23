"use client"

import * as React from "react"
import {
    ChevronLeftIcon,
    ChevronRightIcon,
    Columns2Icon,
    Maximize2Icon,
    PanelRightIcon,
    TablePropertiesIcon,
    XIcon,
} from "lucide-react"

import {
    BusinessEmptyState,
    BusinessFailureState,
    type BusinessEmptyStateKind,
} from "@/components/business/feedback-states"
import {
    ListWorkSurface,
    listWorkspaceEmptyStateClassName,
} from "@/components/business/list-workspace"
import { TaskActionSheet } from "@/components/business/task-action-sheet"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import {
    ResizableHandle,
    ResizablePanel,
    ResizablePanelGroup,
} from "@/components/ui/resizable"
import { Skeleton } from "@/components/ui/skeleton"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"
import { isWorkspaceFrameActive } from "@/lib/workspace-hotkeys"

export type WorkspaceQueueLayoutMode = "table" | "split"

export const WORKSPACE_QUEUE_LAYOUT_STORAGE_KEY = "erp.workspace.queueLayout"

const LAYOUT_CHANGE_EVENT = "erp-workspace-queue-layout-change"
const SPLIT_WIDE_MIN_WIDTH = 1120

const INTERACTIVE_CARD_TARGET_SELECTOR = [
    "button",
    "a",
    "input",
    "select",
    "textarea",
    '[contenteditable]:not([contenteditable="false"])',
    '[role="button"]',
    '[role="checkbox"]',
    '[role="combobox"]',
    '[role="link"]',
    '[role="menuitem"]',
].join(",")

const EDITABLE_TARGET_SELECTOR =
    'input, textarea, select, [contenteditable="true"], [role="textbox"]'

const ACTIVE_OVERLAY_SELECTOR = [
    '[role="dialog"]:not([data-closed]):not([data-slot="toast"])',
    '[role="alertdialog"]:not([data-closed])',
    '[role="menu"]:not([data-closed])',
    '[role="listbox"]:not([data-closed]):not([data-slot="table-container"])',
    '[data-slot="sheet"]:not([data-closed])',
    '[data-slot="sheet-content"]:not([data-closed])',
    '[data-slot="combobox-content"][data-open]',
    '[data-slot="select-content"][data-open]',
    '[data-slot="dropdown-menu-content"][data-open]',
    '[data-slot="popover-content"][data-open]',
].join(", ")

function readStoredQueueLayout(
    storageKey: string,
    defaultMode: WorkspaceQueueLayoutMode,
): WorkspaceQueueLayoutMode {
    if (typeof window === "undefined") return defaultMode
    try {
        const stored = window.localStorage.getItem(storageKey)
        if (stored === "split" || stored === "table") return stored
    } catch {
        // 隐私模式下忽略本地存储读取异常
    }
    return defaultMode
}

function writeStoredQueueLayout(
    storageKey: string,
    mode: WorkspaceQueueLayoutMode,
): void {
    if (typeof window === "undefined") return
    try {
        window.localStorage.setItem(storageKey, mode)
    } catch {
        // 隐私模式下忽略本地存储写入异常
    }
    window.dispatchEvent(
        new CustomEvent(LAYOUT_CHANGE_EVENT, { detail: { storageKey, mode } }),
    )
}

/**
 * 持久化队列布局模式（`"table" | "split"`），默认写入 `localStorage["erp.workspace.queueLayout"]`。
 * 使用 `useSyncExternalStore` 保证 SSR 首屏无水合闪烁，并在同页多消费者间即时同步。
 */
export function useWorkspaceQueueLayout(
    defaultMode: WorkspaceQueueLayoutMode = "table",
    storageKey: string = WORKSPACE_QUEUE_LAYOUT_STORAGE_KEY,
): readonly [
    WorkspaceQueueLayoutMode,
    (next: WorkspaceQueueLayoutMode) => void,
] {
    const subscribe = React.useCallback(
        (listener: () => void) => {
            if (typeof window === "undefined") return () => {}
            const onCustom = (event: Event) => {
                const detail = (event as CustomEvent<{ storageKey?: string }>)
                    .detail
                if (!detail?.storageKey || detail.storageKey === storageKey) {
                    listener()
                }
            }
            const onStorage = (event: StorageEvent) => {
                if (event.key === null || event.key === storageKey) {
                    listener()
                }
            }
            window.addEventListener(LAYOUT_CHANGE_EVENT, onCustom)
            window.addEventListener("storage", onStorage)
            return () => {
                window.removeEventListener(LAYOUT_CHANGE_EVENT, onCustom)
                window.removeEventListener("storage", onStorage)
            }
        },
        [storageKey],
    )

    const getSnapshot = React.useCallback(
        () => readStoredQueueLayout(storageKey, defaultMode),
        [defaultMode, storageKey],
    )
    const getServerSnapshot = React.useCallback(
        () => defaultMode,
        [defaultMode],
    )

    const mode = React.useSyncExternalStore(
        subscribe,
        getSnapshot,
        getServerSnapshot,
    )

    const setMode = React.useCallback(
        (next: WorkspaceQueueLayoutMode) => {
            writeStoredQueueLayout(storageKey, next)
        },
        [storageKey],
    )

    return [mode, setMode] as const
}

/** 紧凑 `[通栏 / 分栏]` 视图切换胶囊，可直接挂入页头或表格工具栏右侧。 */
export function WorkspaceQueueLayoutToggle({
    value,
    onValueChange,
    idPrefix = "workspace-queue-layout",
    className,
}: {
    value: WorkspaceQueueLayoutMode
    onValueChange: (next: WorkspaceQueueLayoutMode) => void
    idPrefix?: string
    className?: string
}) {
    return (
        <div
            role="group"
            aria-label="列表布局切换"
            data-slot="workspace-queue-layout-toggle"
            className={cn(
                "inline-flex h-control items-center rounded-md border border-border/80 bg-muted/45 p-0.5",
                className,
            )}
        >
            <Button
                id={`${idPrefix}-table`}
                type="button"
                variant="ghost"
                size="xs"
                aria-pressed={value === "table"}
                title="通栏表格视图（高密度单行）"
                className={cn(
                    "h-control-sm gap-1 rounded-sm px-2 text-xs font-normal text-muted-foreground shadow-none hover:bg-transparent hover:text-foreground",
                    value === "table" &&
                        "bg-card font-medium text-foreground shadow-2xs hover:bg-card",
                )}
                onClick={() => onValueChange("table")}
            >
                <TablePropertiesIcon className="size-3.5" aria-hidden="true" />
                <span>通栏</span>
            </Button>
            <Button
                id={`${idPrefix}-split`}
                type="button"
                variant="ghost"
                size="xs"
                aria-pressed={value === "split"}
                title="分栏快审视图（左侧摘要队列 + 右侧原位检查器）"
                className={cn(
                    "h-control-sm gap-1 rounded-sm px-2 text-xs font-normal text-muted-foreground shadow-none hover:bg-transparent hover:text-foreground",
                    value === "split" &&
                        "bg-card font-medium text-foreground shadow-2xs hover:bg-card",
                )}
                onClick={() => onValueChange("split")}
            >
                <Columns2Icon className="size-3.5" aria-hidden="true" />
                <span>分栏</span>
            </Button>
        </div>
    )
}

export type WorkspaceSplitQueueHeaderMeta = Readonly<{
    title: React.ReactNode
    subtitle?: React.ReactNode
    badges?: React.ReactNode
    openFullPageLabel?: string
}>

export type WorkspaceSplitQueueInspectorContext<TItem> = Readonly<{
    item: TItem
    id: string
    index: number
    totalInPage: number
    hasPrev: boolean
    hasNext: boolean
    onPrev: () => void
    onNext: () => void
    onClose: () => void
    onOpenFullPage: () => void
    /** `"pane"` 表示宽屏右分栏检查器；`"sheet"` 表示窄屏或通栏快览抽屉。 */
    presentation: "pane" | "sheet"
}>

export type WorkspaceSplitQueueProps<TItem> = Readonly<{
    idPrefix: string
    ariaLabel: string
    layoutMode: WorkspaceQueueLayoutMode
    items: readonly TItem[]
    getItemId: (item: TItem) => string
    selectedId: string | null
    /** 当已选条目因状态变更（如未读筛选下标为已读）移出当前列表时保留其快照，避免检查器被意外关闭。 */
    fallbackActiveItem?: TItem | null
    onSelectedIdChange: (id: string | null) => void
    /** 打开独立完整工作区页签（双击 / Cmd+Click / 全屏打开）。 */
    onOpenFullPage: (item: TItem) => void
    views?: React.ReactNode
    toolbar?: React.ReactNode
    tableActions?: React.ReactNode
    /** 底部悬浮批量操作条（通栏/窄屏挂载于列表表面底中；宽屏分栏挂载于左栏底部，不遮挡右栏操作区）。 */
    selectionBar?: React.ReactNode
    selectionBarOpen?: boolean
    /** 通栏表格视图（包含完整 `ListDataTable` 与列设置）。 */
    tableView: React.ReactNode
    /**
     * 分栏左侧队列视图（宽屏左 36% 与窄屏分栏态共用）。
     * 按 `AGENTS.md §8` 卡片视图不挂载列设置（若内部使用表格须传 `showColumnVisibility={false}`）。
     */
    splitListView: React.ReactNode
    getInspectorHeader: (item: TItem) => WorkspaceSplitQueueHeaderMeta
    renderInspector: (
        ctx: WorkspaceSplitQueueInspectorContext<TItem>,
    ) => React.ReactNode
    /** 抽屉正文自定义样式（例如承载内置吸底操作栏的作业面时传 `"flex flex-col overflow-hidden p-0"`）。 */
    sheetBodyClassName?: string
    emptyInspectorTitle?: string
    emptyInspectorDescription?: string
    className?: string
    toolbarClassName?: string
}>

function useWorkspaceWideContainer(
    ref: React.RefObject<HTMLDivElement | null>,
): boolean {
    const [wide, setWide] = React.useState(false)

    React.useLayoutEffect(() => {
        const element = ref.current
        if (!element) return
        const container =
            element.closest<HTMLElement>("#main-content") ?? element

        const update = (width: number) => {
            setWide(width >= SPLIT_WIDE_MIN_WIDTH)
        }
        update(container.getBoundingClientRect().width)

        if (typeof ResizeObserver === "undefined") {
            const onResize = () =>
                update(container.getBoundingClientRect().width)
            window.addEventListener("resize", onResize)
            return () => window.removeEventListener("resize", onResize)
        }

        const observer = new ResizeObserver((entries) => {
            const entry = entries[0]
            if (!entry) return
            update(entry.contentRect.width)
        })
        observer.observe(container)
        return () => observer.disconnect()
    }, [ref])

    return wide
}

function isEditableElement(target: EventTarget | null): boolean {
    if (!(target instanceof Element)) return false
    if (target instanceof HTMLElement && target.isContentEditable) return true
    return target.closest(EDITABLE_TARGET_SELECTOR) !== null
}

/**
 * 统一 Master-Detail 分栏与通栏队列壳（`data-slot="workspace-split-queue"`）。
 *
 * - 宽容器（`>= 1120px`，对齐 `@[1120px]/workspace:`）+ 分栏模式：启用 `ResizablePanelGroup`
 *   （左队列默认 36%，范围 28%–50%；右原位检查器 64% + 吸顶紧凑顶栏 `[全屏打开]` / `[× 关闭]`，零 iframe 冷启动）；
 * - 窄容器（`< 1120px`）或通栏模式点选快览：优雅回退为 `TaskActionSheet`（固定 `size="detail"` 768px），
 *   并在抽屉关闭后将焦点归还触发行；
 * - 始终仅挂载一份检查器 DOM，避免重复自动化 ID 或重复详情查询。
 */
export function WorkspaceSplitQueue<TItem>({
    idPrefix,
    ariaLabel,
    layoutMode,
    items,
    getItemId,
    selectedId,
    fallbackActiveItem,
    onSelectedIdChange,
    onOpenFullPage,
    views,
    toolbar,
    tableActions,
    selectionBar,
    selectionBarOpen = false,
    tableView,
    splitListView,
    getInspectorHeader,
    renderInspector,
    sheetBodyClassName,
    emptyInspectorTitle = "选择左侧条目即可原位查看与办理",
    emptyInspectorDescription = "单击条目在右侧即时展开，双击或 ⌘/Ctrl+单击打开独立完整页签。",
    className,
    toolbarClassName,
}: WorkspaceSplitQueueProps<TItem>) {
    const rootRef = React.useRef<HTMLDivElement | null>(null)
    const isWide = useWorkspaceWideContainer(rootRef)
    const isWideSplit = layoutMode === "split" && isWide

    const selectedIndex = React.useMemo(
        () =>
            selectedId
                ? items.findIndex((entry) => getItemId(entry) === selectedId)
                : -1,
        [getItemId, items, selectedId],
    )
    const activeItem =
        selectedIndex >= 0
            ? items[selectedIndex]
            : selectedId &&
                fallbackActiveItem &&
                getItemId(fallbackActiveItem) === selectedId
              ? fallbackActiveItem
              : null
    const activeId = activeItem ? getItemId(activeItem) : null

    const lastFocusedIdRef = React.useRef<string | null>(null)
    React.useEffect(() => {
        if (activeId) {
            lastFocusedIdRef.current = activeId
        }
    }, [activeId])

    const hasPrev = selectedIndex > 0
    const hasNext =
        selectedIndex >= 0 ? selectedIndex + 1 < items.length : items.length > 0

    const handlePrev = React.useCallback(() => {
        if (selectedIndex > 0) {
            onSelectedIdChange(getItemId(items[selectedIndex - 1]))
        }
    }, [getItemId, items, onSelectedIdChange, selectedIndex])

    const handleNext = React.useCallback(() => {
        if (selectedIndex >= 0 && selectedIndex + 1 < items.length) {
            onSelectedIdChange(getItemId(items[selectedIndex + 1]))
        } else if (selectedIndex < 0 && items.length > 0) {
            onSelectedIdChange(getItemId(items[0]))
        }
    }, [getItemId, items, onSelectedIdChange, selectedIndex])

    const handleClose = React.useCallback(() => {
        onSelectedIdChange(null)
    }, [onSelectedIdChange])

    // 宽屏分栏下按 Esc 收起右栏检查器（仅在当前激活页签生效，存在浮层、IME 组合或输入焦点时让渡）。
    React.useEffect(() => {
        if (!isWideSplit || !activeItem) return
        const onKeyDown = (event: KeyboardEvent) => {
            if (!isWorkspaceFrameActive()) return
            if (
                event.key !== "Escape" ||
                event.defaultPrevented ||
                event.isComposing ||
                event.keyCode === 229 ||
                event.metaKey ||
                event.ctrlKey ||
                event.altKey
            ) {
                return
            }
            if (
                isEditableElement(event.target) ||
                isEditableElement(document.activeElement)
            ) {
                return
            }
            if (document.querySelector(ACTIVE_OVERLAY_SELECTOR)) return
            event.preventDefault()
            handleClose()
        }
        window.addEventListener("keydown", onKeyDown)
        return () => window.removeEventListener("keydown", onKeyDown)
    }, [activeItem, handleClose, isWideSplit])

    const restoreTriggerFocus = React.useCallback(() => {
        const targetId = lastFocusedIdRef.current
        if (!targetId) return
        const escaped = CSS.escape(targetId)
        const target =
            rootRef.current?.querySelector<HTMLElement>(
                `[data-split-item-id="${escaped}"], [data-row-id="${escaped}"]`,
            ) ?? null
        target?.focus()
    }, [])

    const inspectorHeader = activeItem ? getInspectorHeader(activeItem) : null
    const inspectorCtx: WorkspaceSplitQueueInspectorContext<TItem> | null =
        activeItem && activeId
            ? {
                  item: activeItem,
                  id: activeId,
                  index: selectedIndex,
                  totalInPage: items.length,
                  hasPrev,
                  hasNext,
                  onPrev: handlePrev,
                  onNext: handleNext,
                  onClose: handleClose,
                  onOpenFullPage: () => onOpenFullPage(activeItem),
                  presentation: isWideSplit ? "pane" : "sheet",
              }
            : null

    const showSurfaceSelectionDock = !isWideSplit && selectionBar != null
    const showLeftSelectionDock = isWideSplit && selectionBar != null
    const isLeftDockOpen = showLeftSelectionDock && selectionBarOpen

    const mainContent = isWideSplit ? (
        <ResizablePanelGroup
            id={`${idPrefix}-split-group`}
            orientation="horizontal"
            className="min-h-0 flex-1 overflow-hidden bg-card"
        >
            <ResizablePanel
                id={`${idPrefix}-split-list`}
                defaultSize="36%"
                minSize="28%"
                maxSize="50%"
                className="flex min-h-0 min-w-0 flex-col overflow-hidden"
            >
                <div
                    data-slot="workspace-split-list-pane"
                    data-selection-dock-open={
                        isLeftDockOpen ? "true" : undefined
                    }
                    className="relative flex h-full min-h-0 min-w-0 flex-1 flex-col overflow-hidden bg-card"
                >
                    {splitListView}
                    {showLeftSelectionDock ? (
                        <div
                            data-slot="list-selection-dock"
                            data-open={isLeftDockOpen ? "true" : "false"}
                            aria-hidden={!isLeftDockOpen || undefined}
                            className={cn(
                                "pointer-events-none absolute inset-x-0 bottom-3 z-40 flex justify-center px-3",
                                "transition-[opacity,transform,visibility] motion-reduce:transition-none",
                                isLeftDockOpen
                                    ? "visible translate-y-0 scale-100 opacity-100 duration-200 ease-out"
                                    : "invisible translate-y-2.5 scale-[0.98] opacity-0 duration-150 ease-in",
                            )}
                        >
                            <div
                                data-slot="list-selection-dock-surface"
                                className={cn(
                                    "max-w-full rounded-2xl border border-border/90 bg-popover/95 px-2.5 py-1.5 text-popover-foreground shadow-xl ring-1 ring-foreground/5 backdrop-blur-md dark:ring-foreground/10",
                                    "[&_[data-slot=button]]:shadow-none",
                                    isLeftDockOpen
                                        ? "pointer-events-auto"
                                        : "pointer-events-none",
                                )}
                            >
                                {selectionBar}
                            </div>
                        </div>
                    ) : null}
                </div>
            </ResizablePanel>

            <ResizableHandle
                id={`${idPrefix}-split-handle`}
                withHandle
                aria-label="调整列表与检查器宽度"
                className="bg-border/80 transition-colors hover:bg-primary/40"
            />

            <ResizablePanel
                id={`${idPrefix}-split-inspector`}
                defaultSize="64%"
                minSize="50%"
                className="flex min-h-0 min-w-0 flex-col overflow-hidden"
            >
                <div
                    data-slot="workspace-split-inspector-pane"
                    className="@container/inspector flex h-full min-h-0 min-w-0 flex-1 flex-col overflow-hidden bg-card"
                >
                    {inspectorCtx && inspectorHeader ? (
                        <>
                            <div
                                data-slot="workspace-split-inspector-bar"
                                className="flex h-11 shrink-0 items-center justify-between gap-3 border-b border-border/60 bg-card px-4"
                            >
                                <div className="flex min-w-0 flex-1 items-center gap-2 overflow-hidden">
                                    <div className="min-w-0 truncate text-body-compact font-semibold text-foreground">
                                        {inspectorHeader.title}
                                    </div>
                                    {inspectorHeader.subtitle ? (
                                        <span className="hidden shrink-0 text-xs text-muted-foreground @[520px]/inspector:inline">
                                            {inspectorHeader.subtitle}
                                        </span>
                                    ) : null}
                                    {inspectorHeader.badges ? (
                                        <div className="flex shrink-0 items-center gap-1.5">
                                            {inspectorHeader.badges}
                                        </div>
                                    ) : null}
                                </div>

                                <div className="flex shrink-0 items-center gap-1">
                                    {items.length > 1 ||
                                    (selectedIndex < 0 && items.length > 0) ? (
                                        <div className="mr-1 flex items-center gap-0.5">
                                            {selectedIndex >= 0 ? (
                                                <span className="num mr-1 text-xs text-muted-foreground">
                                                    {selectedIndex + 1}/
                                                    {items.length}
                                                </span>
                                            ) : null}
                                            <Button
                                                id={`${idPrefix}-inspector-prev`}
                                                type="button"
                                                variant="ghost"
                                                size="icon-xs"
                                                disabled={!hasPrev}
                                                aria-label="上一项"
                                                title="上一项"
                                                onClick={handlePrev}
                                            >
                                                <ChevronLeftIcon
                                                    className="size-3.5"
                                                    aria-hidden="true"
                                                />
                                            </Button>
                                            <Button
                                                id={`${idPrefix}-inspector-next`}
                                                type="button"
                                                variant="ghost"
                                                size="icon-xs"
                                                disabled={!hasNext}
                                                aria-label="下一项"
                                                title="下一项"
                                                onClick={handleNext}
                                            >
                                                <ChevronRightIcon
                                                    className="size-3.5"
                                                    aria-hidden="true"
                                                />
                                            </Button>
                                        </div>
                                    ) : null}
                                    <Button
                                        id={`${idPrefix}-inspector-open-full`}
                                        type="button"
                                        variant="ghost"
                                        size="xs"
                                        className="h-control-sm gap-1 px-2 text-xs text-muted-foreground hover:text-foreground"
                                        onClick={inspectorCtx.onOpenFullPage}
                                    >
                                        <Maximize2Icon
                                            className="size-3.5"
                                            aria-hidden="true"
                                        />
                                        <span>
                                            {inspectorHeader.openFullPageLabel ??
                                                "全屏打开"}
                                        </span>
                                    </Button>
                                    <Button
                                        id={`${idPrefix}-inspector-close`}
                                        type="button"
                                        variant="ghost"
                                        size="icon-xs"
                                        className="size-control-sm text-muted-foreground hover:text-foreground"
                                        aria-label="关闭检查器"
                                        title="关闭检查器（Esc）"
                                        onClick={handleClose}
                                    >
                                        <XIcon
                                            className="size-3.5"
                                            aria-hidden="true"
                                        />
                                    </Button>
                                </div>
                            </div>

                            <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
                                {renderInspector(inspectorCtx)}
                            </div>
                        </>
                    ) : (
                        <div className="flex h-full min-h-48 flex-col items-center justify-center gap-2.5 px-6 py-12 text-center">
                            <div className="flex size-10 items-center justify-center rounded-lg bg-muted/70 text-muted-foreground">
                                <PanelRightIcon
                                    className="size-5"
                                    aria-hidden="true"
                                />
                            </div>
                            <p className="text-body-compact font-medium text-foreground">
                                {emptyInspectorTitle}
                            </p>
                            <p className="max-w-sm text-xs text-muted-foreground">
                                {emptyInspectorDescription}
                            </p>
                            {items.length > 0 ? (
                                <Button
                                    id={`${idPrefix}-inspector-select-first`}
                                    type="button"
                                    variant="outline"
                                    size="sm"
                                    className="mt-1"
                                    onClick={() =>
                                        onSelectedIdChange(getItemId(items[0]))
                                    }
                                >
                                    查看第一条
                                </Button>
                            ) : null}
                        </div>
                    )}
                </div>
            </ResizablePanel>
        </ResizablePanelGroup>
    ) : layoutMode === "split" ? (
        splitListView
    ) : (
        tableView
    )

    const sheetOpen = !isWideSplit && Boolean(inspectorCtx && inspectorHeader)

    return (
        <div
            ref={rootRef}
            data-slot="workspace-split-queue"
            data-layout={isWideSplit ? "split" : layoutMode}
            className={cn("flex min-h-0 min-w-0 flex-1 flex-col", className)}
        >
            <ListWorkSurface
                ariaLabel={ariaLabel}
                views={views}
                toolbar={toolbar}
                toolbarClassName={toolbarClassName}
                tableActions={tableActions}
                selectionBar={
                    showSurfaceSelectionDock ? selectionBar : undefined
                }
                selectionBarOpen={
                    showSurfaceSelectionDock ? selectionBarOpen : undefined
                }
                selectionBarPlacement="dock"
                tableClassName="flex min-w-0 flex-col"
                table={mainContent}
            />

            {!isWideSplit && inspectorCtx && inspectorHeader ? (
                <TaskActionSheet
                    open={sheetOpen}
                    onOpenChange={(nextOpen) => {
                        if (!nextOpen) handleClose()
                    }}
                    onOpenChangeComplete={(nextOpen) => {
                        if (!nextOpen) restoreTriggerFocus()
                    }}
                    idPrefix={`${idPrefix}-sheet`}
                    title={inspectorHeader.title}
                    subtitle={inspectorHeader.subtitle}
                    badges={inspectorHeader.badges}
                    positionLabel={
                        selectedIndex >= 0 && items.length > 1
                            ? `第 ${selectedIndex + 1} / ${items.length} 项`
                            : undefined
                    }
                    hasPrev={hasPrev}
                    hasNext={hasNext}
                    onPrev={
                        selectedIndex >= 0 && items.length > 1
                            ? handlePrev
                            : undefined
                    }
                    onNext={
                        items.length > 1 ||
                        (selectedIndex < 0 && items.length > 0)
                            ? handleNext
                            : undefined
                    }
                    onOpenFullPage={inspectorCtx.onOpenFullPage}
                    openFullPageLabel={
                        inspectorHeader.openFullPageLabel ?? "全屏打开"
                    }
                    bodyClassName={sheetBodyClassName}
                >
                    {renderInspector(inspectorCtx)}
                </TaskActionSheet>
            ) : null}
        </div>
    )
}

export type WorkspaceSplitCardListEmptyCopy = Readonly<{
    noScopeKind?: BusinessEmptyStateKind
    noScopeTitle?: string
    noScopeDescription?: React.ReactNode
    filterTitle?: string
    filterDescription?: React.ReactNode
    emptyTitle?: string
    emptyDescription?: React.ReactNode
}>

export type WorkspaceSplitCardListProps<TItem> = Readonly<{
    idPrefix: string
    ariaLabel: string
    items: readonly TItem[]
    getItemId: (item: TItem) => string
    getItemLabel: (item: TItem) => string
    selectedId: string | null
    onSelect: (item: TItem) => void
    onOpenFullPage: (item: TItem) => void
    renderCard: (
        item: TItem,
        state: { active: boolean; checked: boolean },
    ) => React.ReactNode
    /** 勾选能力（可选）。仅 `canSelectItem` 返回 true 的行允许勾选。 */
    enableSelection?: boolean
    canSelectItem?: (item: TItem) => boolean
    selectedIds?: readonly string[]
    onSelectedIdsChange?: (ids: string[]) => void
    loading?: boolean
    isError?: boolean
    error?: unknown
    onRetry?: () => void
    noScope?: boolean
    hasActiveFilters?: boolean
    onClearFilters?: () => void
    emptyCopy?: WorkspaceSplitCardListEmptyCopy
    /** 分页（仅 `pageCount > 1` 显示，与通栏表格保持单页零分页条策略）。 */
    total: number
    page: number
    pageSize: number
    onPageChange: (page: number) => void
    scrollContainerRef?: React.RefObject<HTMLDivElement | null>
    className?: string
}>

/**
 * 分栏左栏紧凑双行摘要卡列表（每条 ~54px 高，符合 `AGENTS.md §8` 卡片态不显示列设置）。
 *
 * - 单击卡片：就地选中并在右侧检查器（宽屏）或 `TaskActionSheet`（窄屏）展示；
 * - 双击或 `Cmd/Ctrl + 单击`：打开独立完整工作区页签；
 * - 支持键盘 `↑/↓` 切换焦点、`Enter` 打开、`Space` 勾选。
 */
export function WorkspaceSplitCardList<TItem>({
    idPrefix,
    ariaLabel,
    items,
    getItemId,
    getItemLabel,
    selectedId,
    onSelect,
    onOpenFullPage,
    renderCard,
    enableSelection = false,
    canSelectItem,
    selectedIds = [],
    onSelectedIdsChange,
    loading = false,
    isError = false,
    error,
    onRetry,
    noScope = false,
    hasActiveFilters = false,
    onClearFilters,
    emptyCopy,
    total,
    page,
    pageSize,
    onPageChange,
    scrollContainerRef,
    className,
}: WorkspaceSplitCardListProps<TItem>) {
    const itemRefs = React.useRef(new Map<string, HTMLDivElement>())
    const selectedSet = React.useMemo(() => new Set(selectedIds), [selectedIds])

    const selectableIds = React.useMemo(() => {
        if (!enableSelection) return []
        return items
            .filter((item) => (canSelectItem ? canSelectItem(item) : true))
            .map(getItemId)
    }, [canSelectItem, enableSelection, getItemId, items])

    const allSelectableChecked =
        selectableIds.length > 0 &&
        selectableIds.every((id) => selectedSet.has(id))
    const someSelectableChecked =
        !allSelectableChecked && selectableIds.some((id) => selectedSet.has(id))

    const toggleAll = (checked: boolean) => {
        if (!onSelectedIdsChange) return
        onSelectedIdsChange(checked ? selectableIds : [])
    }

    const toggleOne = (id: string, checked: boolean) => {
        if (!onSelectedIdsChange) return
        if (checked) {
            if (!selectedSet.has(id)) {
                onSelectedIdsChange([...selectedIds, id])
            }
        } else {
            onSelectedIdsChange(selectedIds.filter((entry) => entry !== id))
        }
    }

    const pageCount = Math.ceil(total / pageSize)

    return (
        <div
            data-slot="workspace-split-card-list"
            className={cn(
                "flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden bg-card",
                className,
            )}
        >
            <div className="flex h-table-header shrink-0 items-center justify-between gap-2 border-b border-border/80 bg-card px-4 text-xs text-muted-foreground">
                <div className="flex min-w-0 items-center gap-2.5">
                    {enableSelection && selectableIds.length > 0 ? (
                        <Checkbox
                            id={`${idPrefix}-select-all`}
                            checked={allSelectableChecked}
                            indeterminate={someSelectableChecked}
                            onCheckedChange={(value) =>
                                toggleAll(value === true)
                            }
                            aria-label="选择当前页全部可操作记录"
                        />
                    ) : null}
                    <span className="truncate font-medium">
                        {ariaLabel}
                        {total > 0 ? (
                            <span className="num ml-1.5 font-normal text-muted-foreground/80">
                                {total.toLocaleString("zh-CN")}
                            </span>
                        ) : null}
                    </span>
                </div>
                <span className="hidden shrink-0 text-tiny text-muted-foreground/75 sm:inline">
                    单击快览 · 双击全屏
                </span>
            </div>

            <div
                ref={scrollContainerRef}
                data-slot="table-container"
                role="listbox"
                aria-label={ariaLabel}
                className={cn(
                    "min-h-0 flex-1 overflow-y-auto overscroll-contain",
                    selectedIds.length > 0 && "pb-18",
                )}
            >
                {isError ? (
                    <div className="p-4">
                        <BusinessFailureState
                            idPrefix={`${idPrefix}-failure`}
                            error={error}
                            onRetry={onRetry}
                        />
                    </div>
                ) : loading && items.length === 0 ? (
                    <div className="divide-y divide-grid/65">
                        {Array.from({ length: 8 }, (_, idx) => (
                            <div
                                key={`split-skeleton-${idx}`}
                                className="flex min-h-13.5 flex-col justify-center gap-1.5 px-4 py-2"
                            >
                                <div className="flex items-center justify-between gap-2">
                                    <Skeleton className="h-3.5 w-32" />
                                    <Skeleton className="h-3.5 w-16" />
                                </div>
                                <div className="flex items-center justify-between gap-2">
                                    <Skeleton className="h-3 w-44" />
                                    <Skeleton className="h-3 w-12" />
                                </div>
                            </div>
                        ))}
                    </div>
                ) : items.length === 0 ? (
                    <BusinessEmptyState
                        kind={
                            noScope
                                ? (emptyCopy?.noScopeKind ?? "no-scope")
                                : hasActiveFilters
                                  ? "filter"
                                  : "no-data"
                        }
                        className={listWorkspaceEmptyStateClassName}
                        title={
                            noScope
                                ? (emptyCopy?.noScopeTitle ??
                                  "暂无可查看的数据")
                                : hasActiveFilters
                                  ? (emptyCopy?.filterTitle ?? "当前筛选无结果")
                                  : (emptyCopy?.emptyTitle ?? "尚无业务数据")
                        }
                        description={
                            noScope
                                ? emptyCopy?.noScopeDescription
                                : hasActiveFilters
                                  ? (emptyCopy?.filterDescription ??
                                    "没有记录符合当前筛选条件，可清除筛选后重试。")
                                  : emptyCopy?.emptyDescription
                        }
                        action={
                            !noScope && hasActiveFilters && onClearFilters ? (
                                <Button
                                    id={`${idPrefix}-empty-clear-filters`}
                                    type="button"
                                    variant="secondary"
                                    size="sm"
                                    className="rounded-lg shadow-none"
                                    onClick={onClearFilters}
                                >
                                    清除筛选
                                </Button>
                            ) : undefined
                        }
                    />
                ) : (
                    <div className="divide-y divide-grid/65">
                        {items.map((item, index) => {
                            const id = getItemId(item)
                            const active = selectedId === id
                            const checked = selectedSet.has(id)
                            const canSelect =
                                enableSelection &&
                                (canSelectItem ? canSelectItem(item) : true)
                            const segment = toAutomationIdSegment(id)
                            return (
                                <div
                                    key={id}
                                    id={`${idPrefix}-card-${segment}`}
                                    ref={(node) => {
                                        if (node) itemRefs.current.set(id, node)
                                        else itemRefs.current.delete(id)
                                    }}
                                    role="option"
                                    aria-selected={active}
                                    aria-label={getItemLabel(item)}
                                    tabIndex={0}
                                    data-split-item-id={id}
                                    data-highlighted={
                                        active ? "true" : undefined
                                    }
                                    data-state={
                                        checked ? "selected" : undefined
                                    }
                                    className={cn(
                                        "group/split-card relative flex min-h-13.5 cursor-pointer items-center gap-2.5 px-4 py-2 transition-colors outline-none",
                                        "hover:bg-row-hover focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-primary",
                                        active &&
                                            "bg-row-selected before:absolute before:inset-y-0 before:left-0 before:w-0.5 before:bg-primary",
                                    )}
                                    onClick={(event) => {
                                        if (
                                            event.target instanceof Element &&
                                            event.target.closest(
                                                INTERACTIVE_CARD_TARGET_SELECTOR,
                                            )
                                        ) {
                                            return
                                        }
                                        if (event.metaKey || event.ctrlKey) {
                                            event.preventDefault()
                                            onOpenFullPage(item)
                                            return
                                        }
                                        if (event.detail > 1) return
                                        onSelect(item)
                                    }}
                                    onDoubleClick={(event) => {
                                        if (
                                            event.target instanceof Element &&
                                            event.target.closest(
                                                INTERACTIVE_CARD_TARGET_SELECTOR,
                                            )
                                        ) {
                                            return
                                        }
                                        event.preventDefault()
                                        onOpenFullPage(item)
                                    }}
                                    onKeyDown={(event) => {
                                        if (
                                            event.target instanceof Element &&
                                            event.target.closest(
                                                INTERACTIVE_CARD_TARGET_SELECTOR,
                                            )
                                        ) {
                                            return
                                        }
                                        if (event.key === "ArrowDown") {
                                            event.preventDefault()
                                            const next = items[index + 1]
                                            if (next) {
                                                itemRefs.current
                                                    .get(getItemId(next))
                                                    ?.focus()
                                            }
                                        } else if (event.key === "ArrowUp") {
                                            event.preventDefault()
                                            const prev = items[index - 1]
                                            if (prev) {
                                                itemRefs.current
                                                    .get(getItemId(prev))
                                                    ?.focus()
                                            }
                                        } else if (event.key === "Enter") {
                                            event.preventDefault()
                                            if (
                                                event.metaKey ||
                                                event.ctrlKey
                                            ) {
                                                onOpenFullPage(item)
                                            } else {
                                                onSelect(item)
                                            }
                                        } else if (
                                            event.key === " " &&
                                            canSelect
                                        ) {
                                            event.preventDefault()
                                            toggleOne(id, !checked)
                                        }
                                    }}
                                >
                                    {enableSelection ? (
                                        <div className="flex shrink-0 items-center">
                                            <Checkbox
                                                id={`${idPrefix}-card-${segment}-select`}
                                                checked={checked}
                                                disabled={!canSelect}
                                                onClick={(event) =>
                                                    event.stopPropagation()
                                                }
                                                onCheckedChange={(value) =>
                                                    toggleOne(
                                                        id,
                                                        value === true,
                                                    )
                                                }
                                                aria-label={`选择 ${getItemLabel(item)}`}
                                            />
                                        </div>
                                    ) : null}
                                    <div className="min-w-0 flex-1">
                                        {renderCard(item, { active, checked })}
                                    </div>
                                </div>
                            )
                        })}
                    </div>
                )}
            </div>

            {pageCount > 1 ? (
                <div
                    data-slot="data-table-pagination"
                    className="mt-auto flex shrink-0 items-center justify-between gap-2 border-t border-border/60 bg-card px-4 py-table-pagination-block text-xs text-muted-foreground"
                >
                    <span className="num">
                        第 {page} / {pageCount} 页
                    </span>
                    <div className="flex items-center gap-1">
                        <Button
                            id={`${idPrefix}-page-prev`}
                            type="button"
                            variant="outline"
                            size="xs"
                            className="h-control-sm px-2 text-xs shadow-none"
                            disabled={page <= 1 || loading}
                            onClick={() => onPageChange(page - 1)}
                        >
                            上一页
                        </Button>
                        <Button
                            id={`${idPrefix}-page-next`}
                            type="button"
                            variant="outline"
                            size="xs"
                            className="h-control-sm px-2 text-xs shadow-none"
                            disabled={page >= pageCount || loading}
                            onClick={() => onPageChange(page + 1)}
                        >
                            下一页
                        </Button>
                    </div>
                </div>
            ) : null}
        </div>
    )
}
