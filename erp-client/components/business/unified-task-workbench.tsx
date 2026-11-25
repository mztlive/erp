"use client"

import {
    createContext,
    useCallback,
    useContext,
    useEffect,
    useLayoutEffect,
    useMemo,
    useRef,
    useState,
    type ReactNode,
    type Ref,
} from "react"
import type { GroupImperativeHandle, Layout } from "react-resizable-panels"
import { PanelRightCloseIcon, PanelRightOpenIcon } from "lucide-react"

import { IconActionButton } from "@/components/business/icon-action-button"
import { workspaceTaskSurfacePadClassName } from "@/components/business/page"
import {
    ResizableHandle,
    ResizablePanel,
    ResizablePanelGroup,
} from "@/components/ui/resizable"
import { cn } from "@/lib/utils"
import { subscribeWorkspaceHotkeyAction } from "@/lib/workspace-hotkeys"

export type TaskWorkbenchArchetype =
    | "decision" // Archetype 1: 72/28 双栏，右轨挂载 Route + Trail + MaterialViewer
    | "action" // Archetype 2: 100% 通栏专注作业面，无冗余 20rem 侧栏
    | "investigation" // Archetype 3: 顶部诊断横幅 + 排查正文 + 可折叠载荷轨
    | "informational" // Archetype 4: 精简速览面（常运行在 SplitQueue / TaskActionSheet 内）

export type UnifiedTaskWorkbenchContextRail = Readonly<{
    collapsible?: boolean
    resizable?: boolean
    /** 右栏默认宽度百分比（默认 28，可拖拽扩展至 maxWidthPercent 做左右对照）。 */
    defaultWidthPercent?: number
    minWidthPercent?: number
    maxWidthPercent?: number
    /** 跨任务持久化用户拖拽后的右栏宽度（写入 localStorage）。 */
    storageKey?: string
    /** 进入附件或来源单据对照时自动扩宽的最小目标宽度百分比；切回常规页签且未手动拖拽时恢复。 */
    preferredMinWidthPercent?: number
    label?: string
    collapsed?: boolean
    onCollapsedChange?: (collapsed: boolean) => void
    /** 允许全宽执行型作业面挂载按需展开的右侧参考对照轨（默认折叠，不占常规工作宽度）。 */
    allowOnFullWidth?: boolean
    content: ReactNode
}>

export type UnifiedTaskWorkbenchProps = Readonly<{
    archetype: TaskWorkbenchArchetype
    header: ReactNode
    headerSupplement?: ReactNode
    alertBanner?: ReactNode
    kpiStrip?: ReactNode
    /** Archetype 1/3 专用右侧上下文轨；支持折叠与拖拽调宽至 50% 对照附件。 */
    contextRail?: UnifiedTaskWorkbenchContextRail
    footerLeading?: ReactNode
    footerSummary?: ReactNode
    footerActions?: ReactNode
    /** 供 WorkspaceTaskPane / WorkspaceTaskFooter 挂载的内部 portal 槽位。 */
    summaryHostRef?: Ref<HTMLDivElement | null>
    asideHostRef?: Ref<HTMLDivElement | null>
    footerSummaryHostRef?: Ref<HTMLDivElement | null>
    footerHostRef?: Ref<HTMLDivElement | null>
    /** 完整详情页启用通栏负边距贴合；嵌入式检查器/抽屉设为 false。 */
    framed?: boolean
    fullWidth?: boolean
    fillBody?: boolean
    idPrefix?: string
    className?: string
    "aria-label"?: string
    children: ReactNode
}>

export type UnifiedTaskWorkbenchRailControl = Readonly<{
    hasRail: boolean
    collapsible: boolean
    collapsed: boolean
    toggle: () => void
    expand: () => void
    collapse: () => void
    toggleButtonId: string
}>

const UnifiedTaskWorkbenchRailContext =
    createContext<UnifiedTaskWorkbenchRailControl | null>(null)

/** 获取当前统一工作台右侧上下文轨的折叠状态与控制方法。 */
export function useUnifiedTaskWorkbenchRail(): UnifiedTaskWorkbenchRailControl | null {
    return useContext(UnifiedTaskWorkbenchRailContext)
}

/** 标题栏右侧 `[◧]` 折叠/展开右侧上下文轨按钮；无右轨或不可折叠时自动隐藏。 */
export function UnifiedTaskWorkbenchRailToggle({
    id,
    className,
}: {
    id?: string
    className?: string
}) {
    const rail = useUnifiedTaskWorkbenchRail()
    if (!rail || !rail.hasRail || !rail.collapsible) return null

    const label = rail.collapsed
        ? "展开右侧上下文对照栏"
        : "收起右侧上下文栏以全宽查看"
    const button = (
        <IconActionButton
            id={id ?? rail.toggleButtonId}
            testId="workspace-workbench-rail-toggle"
            label={label}
            onClick={rail.toggle}
        >
            {rail.collapsed ? (
                <PanelRightOpenIcon aria-hidden="true" />
            ) : (
                <PanelRightCloseIcon aria-hidden="true" />
            )}
        </IconActionButton>
    )
    return className ? <span className={className}>{button}</span> : button
}

const useIsomorphicLayoutEffect =
    typeof window !== "undefined" ? useLayoutEffect : useEffect

function clampPercent(value: number, min: number, max: number): number {
    return Math.min(max, Math.max(min, Math.round(value)))
}

function readStoredRailPercent(
    storageKey: string | undefined,
    min: number,
    max: number,
    fallback: number,
): number {
    if (!storageKey || typeof window === "undefined") return fallback
    try {
        const raw = window.localStorage.getItem(storageKey)
        if (!raw) return fallback
        const parsed = Number.parseInt(raw, 10)
        if (!Number.isFinite(parsed)) return fallback
        return clampPercent(parsed, min, max)
    } catch {
        return fallback
    }
}

function writeStoredRailPercent(
    storageKey: string | undefined,
    value: number,
): void {
    if (!storageKey || typeof window === "undefined") return
    try {
        window.localStorage.setItem(storageKey, String(Math.round(value)))
    } catch {
        // 隐私模式下忽略本地存储写入失败
    }
}

/**
 * 统一任务与审批工作台骨架（4 原型容器查询引擎）：
 * - 顶部 56px 紧凑吸顶页头，底部 48px 紧凑吸底操作栏；
 * - Archetype 1 (`decision`) / Archetype 3 (`investigation`) 在 `@[960px]/workspace:` 启用可拖拽/可折叠双栏（右轨默认 28%，最大 50% 左右对照附件）；
 * - Archetype 2 (`action`) / Archetype 4 (`informational`) 默认 100% 通栏单画布，彻底撤除冗余 20rem 侧栏。
 */
export function UnifiedTaskWorkbench({
    archetype,
    header,
    headerSupplement,
    alertBanner,
    kpiStrip,
    contextRail,
    footerLeading,
    footerSummary,
    footerActions,
    summaryHostRef,
    asideHostRef,
    footerSummaryHostRef,
    footerHostRef,
    framed = true,
    fullWidth = false,
    fillBody = false,
    idPrefix = "unified-task-workbench",
    className,
    "aria-label": ariaLabel = "当前任务",
    children,
}: UnifiedTaskWorkbenchProps) {
    const rootRef = useRef<HTMLElement>(null)
    const groupRef = useRef<GroupImperativeHandle | null>(null)
    const preAutoExpandPctRef = useRef<number | null>(null)
    const autoExpandedRef = useRef(false)
    const [uncontrolledCollapsed, setUncontrolledCollapsed] = useState(false)
    const [isWideContainer, setIsWideContainer] = useState<boolean>(false)

    const railAllowed =
        Boolean(contextRail) &&
        ((!fullWidth &&
            (archetype === "decision" || archetype === "investigation")) ||
            Boolean(contextRail?.allowOnFullWidth))

    const collapsible = contextRail?.collapsible ?? true
    const resizable = contextRail?.resizable ?? false
    const railCollapsed =
        contextRail?.collapsed !== undefined
            ? contextRail.collapsed
            : uncontrolledCollapsed

    const setRailCollapsed = useCallback(
        (next: boolean) => {
            if (contextRail?.collapsed === undefined) {
                setUncontrolledCollapsed(next)
            }
            contextRail?.onCollapsedChange?.(next)
        },
        [contextRail],
    )

    const toggleRail = useCallback(() => {
        setRailCollapsed(!railCollapsed)
    }, [railCollapsed, setRailCollapsed])

    const expandRail = useCallback(() => {
        setRailCollapsed(false)
    }, [setRailCollapsed])

    const collapseRail = useCallback(() => {
        setRailCollapsed(true)
    }, [setRailCollapsed])

    useEffect(() => {
        if (
            !railAllowed ||
            !collapsible ||
            contextRail?.collapsed !== undefined
        )
            return
        return subscribeWorkspaceHotkeyAction((action) => {
            if (action === "toggle-rail") {
                setUncontrolledCollapsed((current) => {
                    const next = !current
                    contextRail?.onCollapsedChange?.(next)
                    return next
                })
            }
        })
    }, [collapsible, contextRail, railAllowed])

    useIsomorphicLayoutEffect(() => {
        if (!railAllowed || !resizable) return
        const root = rootRef.current
        if (!root) return
        const target =
            root.closest<HTMLElement>("#main-content") ??
            root.parentElement ??
            root
        const sync = () => {
            const width = target.clientWidth
            if (width > 0) {
                setIsWideContainer(width >= 960)
            }
        }
        sync()
        if (typeof ResizeObserver === "undefined") return
        const observer = new ResizeObserver(sync)
        observer.observe(target)
        return () => observer.disconnect()
    }, [railAllowed, resizable])

    const railControl = useMemo<UnifiedTaskWorkbenchRailControl>(
        () => ({
            hasRail: railAllowed,
            collapsible,
            collapsed: railCollapsed,
            toggle: toggleRail,
            expand: expandRail,
            collapse: collapseRail,
            toggleButtonId: `${idPrefix}-rail-toggle`,
        }),
        [
            collapseRail,
            collapsible,
            expandRail,
            idPrefix,
            railAllowed,
            railCollapsed,
            toggleRail,
        ],
    )

    const minRailPct = clampPercent(contextRail?.minWidthPercent ?? 24, 20, 50)
    const defaultRailPct = clampPercent(
        contextRail?.defaultWidthPercent ?? 28,
        minRailPct,
        50,
    )
    const maxRailPct = clampPercent(
        contextRail?.maxWidthPercent ?? 50,
        defaultRailPct,
        55,
    )
    const railStorageKey = contextRail?.storageKey
    const preferredMinRailPct = contextRail?.preferredMinWidthPercent
    const mainPanelId = `${idPrefix}-main-panel`
    const railPanelId = `${idPrefix}-rail-panel`
    const showSplitResizable =
        railAllowed && !railCollapsed && resizable && isWideContainer
    const showSplitStatic = railAllowed && !railCollapsed && !showSplitResizable
    const isSplitLayout = showSplitResizable || showSplitStatic

    const initialRailPct = (() => {
        const stored = readStoredRailPercent(
            railStorageKey,
            minRailPct,
            maxRailPct,
            defaultRailPct,
        )
        if (preferredMinRailPct != null) {
            return Math.max(
                stored,
                clampPercent(preferredMinRailPct, minRailPct, maxRailPct),
            )
        }
        return stored
    })()

    useEffect(() => {
        if (!showSplitResizable) return
        const group = groupRef.current
        if (!group) return
        const layout = group.getLayout()
        const currentRail = layout[railPanelId]
        if (typeof currentRail !== "number" || !Number.isFinite(currentRail)) {
            return
        }
        if (preferredMinRailPct != null) {
            const target = clampPercent(
                preferredMinRailPct,
                minRailPct,
                maxRailPct,
            )
            if (currentRail < target - 1) {
                if (preAutoExpandPctRef.current == null) {
                    preAutoExpandPctRef.current = currentRail
                }
                autoExpandedRef.current = true
                group.setLayout({
                    [mainPanelId]: 100 - target,
                    [railPanelId]: target,
                })
            }
            return
        }
        if (autoExpandedRef.current && preAutoExpandPctRef.current != null) {
            const restore = clampPercent(
                preAutoExpandPctRef.current,
                minRailPct,
                maxRailPct,
            )
            autoExpandedRef.current = false
            preAutoExpandPctRef.current = null
            group.setLayout({
                [mainPanelId]: 100 - restore,
                [railPanelId]: restore,
            })
        }
    }, [
        mainPanelId,
        maxRailPct,
        minRailPct,
        preferredMinRailPct,
        railPanelId,
        showSplitResizable,
    ])

    const handleSplitLayoutChanged = useCallback(
        (layout: Layout, meta: { isUserInteraction: boolean }) => {
            if (!meta.isUserInteraction) return
            autoExpandedRef.current = false
            preAutoExpandPctRef.current = null
            const railSize = layout[railPanelId]
            if (
                typeof railSize === "number" &&
                Number.isFinite(railSize) &&
                preferredMinRailPct == null
            ) {
                writeStoredRailPercent(
                    railStorageKey,
                    clampPercent(railSize, minRailPct, maxRailPct),
                )
            }
        },
        [
            maxRailPct,
            minRailPct,
            preferredMinRailPct,
            railPanelId,
            railStorageKey,
        ],
    )

    const mainCanvas = (
        <div
            data-slot="workspace-task-body"
            className={cn(
                "min-w-0 [&>[data-slot=alert]]:my-3 [&_[data-slot=page-scaffold]]:h-auto",
                fullWidth &&
                    "flex min-h-0 flex-1 flex-col overflow-hidden [&_[data-slot=page-scaffold]]:h-full",
                fillBody &&
                    "@[960px]/workspace:flex @[960px]/workspace:min-h-0 @[960px]/workspace:flex-1 @[960px]/workspace:flex-col @[960px]/workspace:overflow-hidden",
            )}
        >
            {alertBanner ? (
                <div
                    data-slot="workbench-alert-banner"
                    className="space-y-2 pt-3 empty:hidden"
                >
                    {alertBanner}
                </div>
            ) : null}
            {kpiStrip ? (
                <div
                    data-slot="workbench-kpi-strip"
                    className="border-b border-border/60 py-2.5 empty:hidden"
                >
                    {kpiStrip}
                </div>
            ) : null}
            {summaryHostRef ? (
                <div
                    ref={summaryHostRef}
                    data-slot="workbench-summary-host"
                    className="border-b border-border/60 py-2.5 empty:hidden"
                />
            ) : null}
            {children}
        </div>
    )

    const railContent =
        railAllowed && contextRail ? (
            <aside
                data-slot="workbench-context-rail"
                aria-label={contextRail.label ?? "审批进度与上下文对照"}
                className="flex h-full min-h-0 min-w-0 flex-col bg-card"
            >
                <div className="flex min-h-0 flex-1 flex-col">
                    {contextRail.content}
                </div>
                {asideHostRef ? (
                    <div
                        ref={asideHostRef}
                        className="space-y-4 border-t border-border/60 pt-4 empty:hidden"
                    />
                ) : null}
            </aside>
        ) : null

    return (
        <UnifiedTaskWorkbenchRailContext.Provider value={railControl}>
            <section
                ref={rootRef}
                data-slot="unified-task-workbench"
                data-archetype={archetype}
                data-rail-collapsed={railCollapsed ? "true" : "false"}
                className={cn(
                    "flex h-full min-h-0 min-w-0 flex-col bg-card",
                    className,
                )}
                aria-label={ariaLabel}
            >
                {header || headerSupplement ? (
                    <div
                        data-slot="workbench-header-chrome"
                        className={cn(
                            "shrink-0 bg-card empty:hidden",
                            framed
                                ? "-mx-(--scaffold-px) px-(--scaffold-px)"
                                : header
                                  ? cn(
                                        workspaceTaskSurfacePadClassName,
                                        "border-b border-border/60 py-1.5",
                                    )
                                  : undefined,
                        )}
                    >
                        {header}
                        {headerSupplement}
                    </div>
                ) : null}

                <div
                    data-slot="workbench-viewport"
                    className={cn(
                        "flex min-h-0 flex-1 flex-col bg-card",
                        framed &&
                            (isSplitLayout
                                ? "-mx-(--scaffold-px)"
                                : "-mx-(--scaffold-px) px-(--scaffold-px)"),
                        showSplitResizable || fullWidth
                            ? "overflow-hidden"
                            : "overflow-y-auto overscroll-contain",
                        fillBody && "@[960px]/workspace:overflow-hidden",
                    )}
                >
                    {showSplitResizable ? (
                        <ResizablePanelGroup
                            id={`${idPrefix}-panel-group`}
                            groupRef={groupRef}
                            orientation="horizontal"
                            onLayoutChanged={handleSplitLayoutChanged}
                            className="min-h-0 flex-1"
                        >
                            <ResizablePanel
                                id={mainPanelId}
                                defaultSize={`${100 - initialRailPct}%`}
                                minSize={`${100 - maxRailPct}%`}
                                className="flex min-h-0 min-w-0 flex-col"
                            >
                                <div
                                    className={cn(
                                        "min-h-0 flex-1 pr-4 @[1120px]/workspace:pr-6",
                                        framed && "pl-(--scaffold-px)",
                                        fullWidth
                                            ? "flex flex-col overflow-hidden pr-0 @[1120px]/workspace:pr-0"
                                            : "overflow-y-auto overscroll-contain",
                                    )}
                                >
                                    {mainCanvas}
                                </div>
                            </ResizablePanel>
                            <ResizableHandle
                                id={`${idPrefix}-resize-handle`}
                                withHandle
                                aria-label="拖拽调整单据画布与右侧对照栏宽度"
                                className="bg-border/60 transition-colors hover:bg-primary/40"
                            />
                            <ResizablePanel
                                id={railPanelId}
                                defaultSize={`${initialRailPct}%`}
                                minSize={`${minRailPct}%`}
                                maxSize={`${maxRailPct}%`}
                                className="flex min-h-0 min-w-0 flex-col bg-card"
                            >
                                <div className="flex min-h-0 flex-1 flex-col overflow-hidden bg-card px-4 @[1120px]/workspace:px-5">
                                    {railContent}
                                </div>
                            </ResizablePanel>
                        </ResizablePanelGroup>
                    ) : showSplitStatic ? (
                        <div
                            className={cn(
                                "grid min-h-0 flex-1 items-start gap-x-0 gap-y-0",
                                !resizable &&
                                    "@[960px]/workspace:grid-cols-[minmax(0,1fr)_minmax(18rem,24%)] @[1120px]/workspace:grid-cols-[minmax(0,1fr)_minmax(19rem,24%)]",
                            )}
                        >
                            <div
                                className={cn(
                                    "min-w-0",
                                    framed && "px-(--scaffold-px)",
                                    !resizable && "@[960px]/workspace:pr-6",
                                    !framed && workspaceTaskSurfacePadClassName,
                                )}
                            >
                                {mainCanvas}
                            </div>
                            <div
                                className={cn(
                                    "min-w-0 border-t border-border/60 bg-card py-4",
                                    framed && "px-(--scaffold-px)",
                                    !resizable &&
                                        "@[960px]/workspace:self-stretch @[960px]/workspace:border-t-0 @[960px]/workspace:border-l @[960px]/workspace:px-5 @[960px]/workspace:py-0",
                                    !framed && workspaceTaskSurfacePadClassName,
                                )}
                            >
                                {railContent}
                            </div>
                        </div>
                    ) : (
                        <div
                            className={cn(
                                "min-h-0 min-w-0 flex-1",
                                fullWidth &&
                                    "flex flex-col items-stretch overflow-hidden",
                                fillBody &&
                                    "@[960px]/workspace:flex @[960px]/workspace:flex-col @[960px]/workspace:items-stretch @[960px]/workspace:overflow-hidden",
                                !framed && workspaceTaskSurfacePadClassName,
                            )}
                        >
                            {mainCanvas}
                        </div>
                    )}
                </div>

                <footer
                    data-slot="workspace-task-footer"
                    className={cn(
                        "hidden min-h-12 w-auto min-w-0 shrink-0 flex-wrap items-center gap-x-3 gap-y-2 border-t border-border/60 bg-card py-2 shadow-footer has-[>:not(:empty)]:flex",
                        framed
                            ? "-mx-(--scaffold-px) px-(--scaffold-px)"
                            : workspaceTaskSurfacePadClassName,
                    )}
                >
                    {footerLeading}
                    {footerSummary}
                    {footerSummaryHostRef ? (
                        <div
                            ref={footerSummaryHostRef}
                            className={
                                fillBody
                                    ? "min-w-0 flex-1 empty:hidden"
                                    : "min-w-0 flex-[1_1_16rem] empty:hidden"
                            }
                        />
                    ) : null}
                    <div
                        ref={footerHostRef}
                        className="ml-auto flex min-w-0 flex-1 flex-wrap items-center justify-end gap-2 empty:hidden"
                    >
                        {footerActions}
                    </div>
                </footer>
            </section>
        </UnifiedTaskWorkbenchRailContext.Provider>
    )
}
