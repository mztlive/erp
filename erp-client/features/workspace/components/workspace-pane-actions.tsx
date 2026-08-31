"use client"

import { createContext, useContext, type ReactNode } from "react"

const WorkspacePaneActionsContext = createContext<ReactNode>(null)

/**
 * 工作台右侧作业面的页头动作（全屏等）。由工作台壳提供，各作业面标题栏消费。
 */
export function WorkspacePaneActionsProvider({
    actions,
    children,
}: {
    actions: ReactNode
    children: ReactNode
}) {
    return (
        <WorkspacePaneActionsContext.Provider value={actions}>
            {children}
        </WorkspacePaneActionsContext.Provider>
    )
}

/** 渲染工作台壳提供的页头动作；没有提供时不占位。 */
export function WorkspacePaneActions() {
    return useContext(WorkspacePaneActionsContext)
}
