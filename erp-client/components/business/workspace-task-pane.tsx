"use client"

import {
    createContext,
    useContext,
    useMemo,
    useState,
    type ReactNode,
} from "react"
import { createPortal } from "react-dom"

import { workspaceTaskSurfacePadClassName } from "@/components/business/page"
import { cn } from "@/lib/utils"

type WorkspaceTaskFooterHost = {
    element: HTMLElement | null
    summaryElement?: HTMLElement | null
}

const WorkspaceTaskFooterHostContext =
    createContext<WorkspaceTaskFooterHost | null>(null)

type WorkspaceTaskPageLayout = {
    compact?: boolean
    fullWidth?: boolean
    summary?: ReactNode
    aside: ReactNode
    footerLeading?: ReactNode
    footer: ReactNode
}
const WorkspaceTaskPageContext = createContext<WorkspaceTaskPageLayout | null>(
    null,
)
const WorkspaceTaskSlotsContext = createContext<{
    summary: HTMLElement | null
    aside: HTMLElement | null
    bodyEnd: HTMLElement | null
} | null>(null)

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

/** 业务模块可把已授权的摘要、状态送入完整办理页；独立详情保留原位置。 */
export function WorkspaceTaskPageSlot({
    slot,
    children,
}: {
    slot: "summary" | "aside" | "bodyEnd"
    children: ReactNode
}) {
    const hosts = useContext(WorkspaceTaskSlotsContext)
    if (!hosts) return children
    const host = hosts[slot]
    return host ? createPortal(children, host) : null
}

export type WorkspaceTaskPaneProps = {
    header: ReactNode
    footer?: ReactNode
    children: ReactNode
    className?: string
    "aria-label"?: string
}

/**
 * 工作台右侧作业面：标题栏和底栏固定，中间区域独立滚动。
 * 操作按钮放 `footer`，或由子树通过 `WorkspaceTaskFooter` 传送到此底栏。
 */
export function WorkspaceTaskPane({
    header,
    footer,
    children,
    className,
    "aria-label": ariaLabel = "当前任务",
}: WorkspaceTaskPaneProps) {
    const pageLayout = useContext(WorkspaceTaskPageContext)
    const parentPane = useContext(WorkspaceTaskFooterHostContext)
    const layout = parentPane ? null : pageLayout
    const [summaryHost, setSummaryHost] = useState<HTMLElement | null>(null)
    const [asideHost, setAsideHost] = useState<HTMLElement | null>(null)
    const [bodyEndHost, setBodyEndHost] = useState<HTMLElement | null>(null)
    const slots = useMemo(
        () => ({
            summary: summaryHost,
            aside: asideHost,
            bodyEnd: bodyEndHost,
        }),
        [summaryHost, asideHost, bodyEndHost],
    )
    const [footerHost, setFooterHost] = useState<HTMLElement | null>(null)
    const [footerSummaryHost, setFooterSummaryHost] =
        useState<HTMLElement | null>(null)
    const hostValue = useMemo<WorkspaceTaskFooterHost>(
        () => ({ element: footerHost, summaryElement: footerSummaryHost }),
        [footerHost, footerSummaryHost],
    )

    if (!layout)
        return (
            <WorkspaceTaskFooterHostContext.Provider value={hostValue}>
                <WorkspaceTaskPageContext.Provider value={null}>
                    <WorkspaceTaskSlotsContext.Provider value={null}>
                        <section
                            className={cn(
                                "flex h-full min-h-0 flex-col",
                                className,
                            )}
                            aria-label={ariaLabel}
                        >
                            <header
                                data-slot="workspace-task-header"
                                className={cn(
                                    workspaceTaskSurfacePadClassName,
                                    "flex shrink-0 items-start justify-between gap-3 border-b border-grid bg-card py-5",
                                )}
                            >
                                {header}
                            </header>
                            <div
                                data-slot="workspace-task-body"
                                className="min-h-0 flex-1 overflow-auto [&>[data-slot=alert]]:mx-5 [&>[data-slot=alert]]:my-5 [&_[data-slot=page-scaffold]]:h-auto"
                            >
                                {children}
                            </div>
                            <footer
                                ref={setFooterHost}
                                data-slot="workspace-task-footer"
                                className={cn(
                                    workspaceTaskSurfacePadClassName,
                                    "flex w-full min-w-0 shrink-0 flex-wrap items-center justify-end gap-2 border-t border-border/40 bg-card py-3 empty:hidden",
                                )}
                            >
                                {footer}
                            </footer>
                        </section>
                    </WorkspaceTaskSlotsContext.Provider>
                </WorkspaceTaskPageContext.Provider>
            </WorkspaceTaskFooterHostContext.Provider>
        )

    return (
        <WorkspaceTaskFooterHostContext.Provider value={hostValue}>
            <WorkspaceTaskSlotsContext.Provider value={layout ? slots : null}>
                <section
                    className={cn(
                        "flex h-full min-h-0 min-w-0 flex-col",
                        className,
                    )}
                    aria-label={ariaLabel}
                >
                    <div
                        className={cn(
                            "flex min-h-0 flex-1 flex-col",
                            layout &&
                                "gap-4 overflow-auto bg-muted/20 p-3 sm:p-5",
                            layout?.compact && "gap-5 sm:p-6",
                            layout.fullWidth &&
                                "gap-3 overflow-hidden p-3 sm:p-4",
                        )}
                    >
                        <div
                            className={cn(
                                layout &&
                                    "shrink-0 rounded-xl border border-border/60 bg-card shadow-xs",
                                layout?.compact &&
                                    "mx-auto w-full max-w-7xl border-0 bg-transparent shadow-none",
                            )}
                        >
                            <header
                                data-slot="workspace-task-header"
                                className={cn(
                                    workspaceTaskSurfacePadClassName,
                                    "flex shrink-0 items-start justify-between gap-3 bg-card py-5",
                                    !layout && "border-b border-grid",
                                    layout.fullWidth && "py-3",
                                    layout && "rounded-t-xl sm:px-6",
                                    layout?.compact &&
                                        "bg-transparent px-0 py-1 sm:px-0",
                                )}
                            >
                                {header}
                            </header>
                            {layout && !layout.fullWidth && (
                                <div
                                    className={cn(
                                        "px-5 pb-5 sm:px-6",
                                        layout.compact && "px-0 pb-0 sm:px-0",
                                    )}
                                >
                                    {layout.summary}
                                    <div ref={setSummaryHost} />
                                </div>
                            )}
                        </div>
                        <div
                            className={cn(
                                "min-h-0 min-w-0",
                                layout
                                    ? layout.compact
                                        ? "mx-auto grid w-full max-w-7xl shrink-0 items-start gap-5 lg:grid-cols-[minmax(0,1fr)_20rem]"
                                        : "grid shrink-0 items-start gap-4 xl:grid-cols-[minmax(0,1fr)_minmax(17rem,28%)]"
                                    : "flex flex-1 flex-col",
                                layout.fullWidth &&
                                    "flex flex-1 shrink flex-col items-stretch overflow-hidden",
                            )}
                        >
                            <div
                                data-slot="workspace-task-body"
                                className={cn(
                                    "min-w-0",
                                    layout
                                        ? "rounded-xl border border-border/60 bg-card shadow-xs"
                                        : "min-h-0 flex-1 overflow-auto",
                                    layout?.compact && "shadow-none",
                                    "[&>[data-slot=alert]]:mx-5 [&>[data-slot=alert]]:my-5 [&_[data-slot=page-scaffold]]:h-auto",
                                    layout.fullWidth &&
                                        "flex min-h-0 flex-1 flex-col overflow-hidden [&_[data-slot=page-scaffold]]:h-full",
                                )}
                            >
                                {children}
                                <div
                                    ref={setBodyEndHost}
                                    className="px-5 pb-6 empty:hidden sm:px-7"
                                />
                            </div>
                            {layout && !layout.fullWidth && (
                                <aside
                                    aria-label="办理进度与状态"
                                    className={cn(
                                        "space-y-6 rounded-xl border border-border/60 bg-card p-5 shadow-xs xl:sticky xl:top-0",
                                        layout.compact &&
                                            "shadow-none lg:sticky lg:top-0",
                                    )}
                                >
                                    {layout.aside}
                                    <div
                                        ref={setAsideHost}
                                        className="space-y-5 empty:hidden"
                                    />
                                </aside>
                            )}
                        </div>
                    </div>
                    <footer
                        data-slot="workspace-task-footer"
                        className={cn(
                            workspaceTaskSurfacePadClassName,
                            "flex w-full min-w-0 shrink-0 flex-wrap items-center gap-3 border-t border-border/60 bg-card py-3",
                            !layout && "justify-end empty:hidden",
                        )}
                    >
                        {layout.footerLeading}
                        <div
                            ref={setFooterSummaryHost}
                            className="min-w-0 flex-[1_1_24rem] empty:hidden"
                        ></div>
                        <div className="ml-auto flex flex-wrap items-center justify-end gap-2">
                            {layout.footer}
                            <div
                                ref={setFooterHost}
                                className="flex min-w-0 flex-wrap items-center justify-end gap-2 empty:hidden"
                            >
                                {footer}
                            </div>
                        </div>
                    </footer>
                </section>
            </WorkspaceTaskSlotsContext.Provider>
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
