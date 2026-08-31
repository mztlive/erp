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
}

const WorkspaceTaskFooterHostContext =
    createContext<WorkspaceTaskFooterHost | null>(null)

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
    const [footerHost, setFooterHost] = useState<HTMLElement | null>(null)
    const hostValue = useMemo<WorkspaceTaskFooterHost>(
        () => ({ element: footerHost }),
        [footerHost],
    )

    return (
        <WorkspaceTaskFooterHostContext.Provider value={hostValue}>
            <section
                className={cn("flex h-full min-h-0 flex-col", className)}
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
                    className={cn(
                        "min-h-0 flex-1 overflow-auto",
                        "[&>[data-slot=alert]]:mx-5 [&>[data-slot=alert]]:my-5",
                        "[&_[data-slot=page-scaffold]]:h-auto",
                    )}
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
        </WorkspaceTaskFooterHostContext.Provider>
    )
}

/**
 * 把作业面操作按钮送到右侧底栏。不在作业面内时按 `fallback` 或原位置渲染。
 */
export function WorkspaceTaskFooter({
    children,
    fallback,
}: {
    children: ReactNode
    fallback?: ReactNode
}) {
    const host = useContext(WorkspaceTaskFooterHostContext)
    if (!host) return fallback ?? children
    if (!host.element) return null
    return createPortal(children, host.element)
}

/** 返回当前组件是否位于工作台右侧作业面内。 */
export function useWorkspaceTaskPane(): boolean {
    return useContext(WorkspaceTaskFooterHostContext) !== null
}
