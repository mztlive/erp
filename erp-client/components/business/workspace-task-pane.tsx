"use client"

import {
    createContext,
    useContext,
    useMemo,
    useState,
    type ReactNode,
} from "react"
import { createPortal } from "react-dom"

import {
    UnifiedTaskWorkbench,
    type TaskWorkbenchArchetype,
    type UnifiedTaskWorkbenchAutoNext,
    type UnifiedTaskWorkbenchContextRail,
} from "@/components/business/unified-task-workbench"

type WorkspaceTaskFooterHost = {
    element: HTMLElement | null
    summaryElement?: HTMLElement | null
}

const WorkspaceTaskFooterHostContext =
    createContext<WorkspaceTaskFooterHost | null>(null)

export type WorkspaceTaskBackLink = {
    id: string
    label: string
    visibility?: "standalone-only" | "always"
} & ({ href: string; onClick?: never } | { href?: never; onClick: () => void })

export type WorkspaceTaskNavigation = {
    back?: WorkspaceTaskBackLink
    meta?: ReactNode
    onRefresh?: () => void
    refreshing?: boolean
}

export type WorkspaceTaskPageLayout = {
    archetype?: TaskWorkbenchArchetype
    flat?: boolean
    fullWidth?: boolean
    fillBody?: boolean
    singleColumn?: boolean
    summaryInHeader?: boolean
    denseHeader?: boolean
    navigation?: WorkspaceTaskNavigation
    summary?: ReactNode
    aside?: ReactNode
    contextRail?: UnifiedTaskWorkbenchContextRail
    /** 底栏左侧标准「处理后自动下一项」开关；可被内层 `WorkspaceTaskPane` 的 `autoNext` 覆盖。 */
    autoNext?: UnifiedTaskWorkbenchAutoNext
    footerLeading?: ReactNode
    footer?: ReactNode
}

const WorkspaceTaskPageContext = createContext<WorkspaceTaskPageLayout | null>(
    null,
)
const WorkspaceTaskNavigationContext =
    createContext<WorkspaceTaskNavigation | null>(null)
const WorkspaceTaskSlotsContext = createContext<{
    summary: HTMLElement | null
    aside: HTMLElement | null
} | null>(null)
const WorkspaceInspectorModeContext = createContext(false)

/** 声明当前任务运行在分栏检查器或轻作业抽屉中，供内层标题栏省略重复单据标题与徽章。 */
export function WorkspaceInspectorModeProvider({
    inspectorMode,
    children,
}: {
    inspectorMode: boolean
    children: ReactNode
}) {
    return (
        <WorkspaceInspectorModeContext.Provider value={inspectorMode}>
            {children}
        </WorkspaceInspectorModeContext.Provider>
    )
}

export function useWorkspaceInspectorMode() {
    return useContext(WorkspaceInspectorModeContext)
}

/** 完整办理页布局仅由工作台入口启用，不改变其他嵌入式消费者。 */
export function WorkspaceTaskPageProvider({
    children,
    ...layout
}: WorkspaceTaskPageLayout & { children: ReactNode }) {
    return (
        <WorkspaceTaskPageContext.Provider value={layout}>
            {children}
        </WorkspaceTaskPageContext.Provider>
    )
}

export function useWorkspaceTaskPage() {
    return useContext(WorkspaceTaskPageContext) !== null
}

/** 获取当前完整办理页的顶部返回、队列切换与刷新能力；嵌套面板自动隔离。 */
export function useWorkspaceTaskNavigation() {
    return useContext(WorkspaceTaskNavigationContext)
}

/** 业务模块可把已授权的摘要、状态送入完整办理页；独立详情保留原位置。 */
export function WorkspaceTaskPageSlot({
    slot,
    children,
}: {
    slot: "summary" | "aside"
    children: ReactNode
}) {
    const hosts = useContext(WorkspaceTaskSlotsContext)
    if (!hosts) return children
    const host = hosts[slot]
    return host ? createPortal(children, host) : null
}

export type WorkspaceTaskPaneProps = {
    header: ReactNode
    headerSupplement?: ReactNode
    alertBanner?: ReactNode
    kpiStrip?: ReactNode
    /** 显式指定工作台原型；未传时继承 WorkspaceTaskPageProvider 配置。 */
    archetype?: TaskWorkbenchArchetype
    /** 显式提供右侧上下文轨（审批轨迹 / 附件对照 / 诊断载荷）。 */
    contextRail?: UnifiedTaskWorkbenchContextRail
    /** 底栏左侧标准「处理后自动下一项」开关；未传时继承 WorkspaceTaskPageProvider 配置。 */
    autoNext?: UnifiedTaskWorkbenchAutoNext
    /** 附加在底栏左侧的自定义内容（位于 `autoNext` 开关之后）。 */
    footerLeading?: ReactNode
    footer?: ReactNode
    children: ReactNode
    idPrefix?: string
    className?: string
    "aria-label"?: string
}

/**
 * 工作台作业面适配壳：统一委托给 `UnifiedTaskWorkbench`。
 * 操作按钮放 `footer`，或由子树通过 `WorkspaceTaskFooter` 传送至底部吸底栏。
 */
export function WorkspaceTaskPane({
    header,
    headerSupplement,
    alertBanner,
    kpiStrip,
    archetype,
    contextRail,
    autoNext,
    footerLeading,
    footer,
    children,
    idPrefix,
    className,
    "aria-label": ariaLabel = "当前任务",
}: WorkspaceTaskPaneProps) {
    const pageLayout = useContext(WorkspaceTaskPageContext)
    const parentPane = useContext(WorkspaceTaskFooterHostContext)
    const layout = parentPane ? null : pageLayout
    const [summaryHost, setSummaryHost] = useState<HTMLElement | null>(null)
    const [asideHost, setAsideHost] = useState<HTMLElement | null>(null)
    const slots = useMemo(
        () => ({
            summary: summaryHost,
            aside: asideHost,
        }),
        [summaryHost, asideHost],
    )
    const [footerHost, setFooterHost] = useState<HTMLElement | null>(null)
    const [footerSummaryHost, setFooterSummaryHost] =
        useState<HTMLElement | null>(null)
    const hostValue = useMemo<WorkspaceTaskFooterHost>(
        () => ({ element: footerHost, summaryElement: footerSummaryHost }),
        [footerHost, footerSummaryHost],
    )

    if (!layout) {
        return (
            <WorkspaceTaskFooterHostContext.Provider value={hostValue}>
                <WorkspaceTaskPageContext.Provider value={null}>
                    <WorkspaceTaskNavigationContext.Provider value={null}>
                        <WorkspaceTaskSlotsContext.Provider value={null}>
                            <UnifiedTaskWorkbench
                                archetype={archetype ?? "informational"}
                                framed={false}
                                header={header}
                                headerSupplement={headerSupplement}
                                alertBanner={alertBanner}
                                kpiStrip={kpiStrip}
                                contextRail={contextRail}
                                autoNext={autoNext}
                                footerLeading={footerLeading}
                                footerSummaryHostRef={setFooterSummaryHost}
                                footerHostRef={setFooterHost}
                                footerActions={footer}
                                idPrefix={idPrefix}
                                className={className}
                                aria-label={ariaLabel}
                            >
                                {children}
                            </UnifiedTaskWorkbench>
                        </WorkspaceTaskSlotsContext.Provider>
                    </WorkspaceTaskNavigationContext.Provider>
                </WorkspaceTaskPageContext.Provider>
            </WorkspaceTaskFooterHostContext.Provider>
        )
    }

    const resolvedArchetype: TaskWorkbenchArchetype =
        archetype ??
        layout.archetype ??
        (layout.singleColumn || layout.fullWidth ? "action" : "decision")

    const effectiveContextRail: UnifiedTaskWorkbenchContextRail | undefined =
        contextRail ??
        layout.contextRail ??
        (!layout.fullWidth &&
        !layout.singleColumn &&
        (resolvedArchetype === "decision" ||
            resolvedArchetype === "investigation") &&
        layout.aside
            ? {
                  collapsible: true,
                  resizable: false,
                  label: "办理进度与状态",
                  content: layout.aside,
              }
            : undefined)

    const resolvedSummary =
        !layout.fullWidth &&
        (!layout.singleColumn || layout.summaryInHeader) &&
        layout.summary
            ? layout.summary
            : null

    const resolvedKpiStrip =
        kpiStrip || resolvedSummary ? (
            <>
                {kpiStrip}
                {resolvedSummary}
            </>
        ) : undefined

    const showSummaryHost =
        !layout.fullWidth && (!layout.singleColumn || layout.summaryInHeader)

    const effectiveAutoNext = autoNext ?? layout.autoNext
    const effectiveFooterLeading = layout.fillBody ? (
        layout.footer || layout.footerLeading || footerLeading ? (
            <>
                {layout.footer}
                {layout.footerLeading}
                {footerLeading}
            </>
        ) : undefined
    ) : layout.footerLeading && footerLeading ? (
        <>
            {layout.footerLeading}
            {footerLeading}
        </>
    ) : (
        (layout.footerLeading ?? footerLeading)
    )

    return (
        <WorkspaceTaskFooterHostContext.Provider value={hostValue}>
            <WorkspaceTaskNavigationContext.Provider
                value={layout.navigation ?? null}
            >
                <WorkspaceTaskSlotsContext.Provider value={slots}>
                    <UnifiedTaskWorkbench
                        archetype={resolvedArchetype}
                        framed
                        fullWidth={layout.fullWidth}
                        fillBody={layout.fillBody}
                        header={header}
                        headerSupplement={headerSupplement}
                        alertBanner={alertBanner}
                        kpiStrip={resolvedKpiStrip}
                        contextRail={effectiveContextRail}
                        autoNext={effectiveAutoNext}
                        summaryHostRef={
                            showSummaryHost ? setSummaryHost : undefined
                        }
                        asideHostRef={
                            effectiveContextRail ? setAsideHost : undefined
                        }
                        footerLeading={effectiveFooterLeading}
                        footerSummary={
                            !layout.fillBody ? layout.footer : undefined
                        }
                        footerSummaryHostRef={setFooterSummaryHost}
                        footerHostRef={setFooterHost}
                        footerActions={footer}
                        idPrefix={idPrefix}
                        className={className}
                        aria-label={ariaLabel}
                    >
                        {children}
                    </UnifiedTaskWorkbench>
                </WorkspaceTaskSlotsContext.Provider>
            </WorkspaceTaskNavigationContext.Provider>
        </WorkspaceTaskFooterHostContext.Provider>
    )
}

/**
 * 把作业面操作按钮送到右侧底栏。不在作业面内时按 `fallback` 或原位置渲染。
 */
export function WorkspaceTaskFooter({
    children,
    fallback,
    inline = false,
    summary,
}: {
    children: ReactNode
    fallback?: ReactNode
    inline?: boolean
    summary?: ReactNode
}) {
    const host = useContext(WorkspaceTaskFooterHostContext)
    if (!host || inline) return fallback ?? children
    if (!host.element) return null
    if (summary) {
        if (!host.summaryElement)
            return createPortal(fallback ?? children, host.element)
        return (
            <>
                {createPortal(summary, host.summaryElement)}
                {createPortal(children, host.element)}
            </>
        )
    }
    return createPortal(children, host.element)
}

/** 返回当前组件是否位于工作台右侧作业面内。 */
export function useWorkspaceTaskPane(): boolean {
    return useContext(WorkspaceTaskFooterHostContext) !== null
}
