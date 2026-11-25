"use client"

import * as React from "react"

import type { WorkspaceNavBadgeKey } from "@/lib/workspace-registry"

/** 工作区壳层消费的最小账号资料视图（不依赖具体 auth feature 类型）。 */
export type WorkspaceAccountProfile = Readonly<{
    userid?: string
    account?: string
    name?: string
    account_kind?: string
    policy_version?: number
    organization_version?: number
    permissions?: readonly string[]
}>

/** 工作区壳层账号资料查询状态快照。 */
export type WorkspaceProfileSnapshot = Readonly<{
    data: WorkspaceAccountProfile | undefined
    isPending: boolean
    isError: boolean
    isSuccess: boolean
    error: unknown
}>

/** 左侧业务导航与业务域角标计数。 */
export type WorkspaceNavBadgeCounts = {
    backgroundJobs: number | undefined
    started?: number
    copied?: number
    todo: number | undefined
    selectionActionable: number
    inquiryFeedback: number | undefined
    procurementInquiry: number | undefined
}

/** 单类角标查询的错误快照。 */
export type WorkspaceBadgeQueryState = Readonly<{
    isError: boolean
    error?: unknown
}>

/** 侧栏角标数据与错误集合。 */
export type WorkspaceNavBadgeData = Readonly<{
    counts: WorkspaceNavBadgeCounts
    errors: Partial<Record<WorkspaceNavBadgeKey, WorkspaceBadgeQueryState>>
}>

/** 快捷搜索（⌘K）展示的任务与往来单位直达条目。 */
export type WorkspaceQuickSearchTaskItem = Readonly<{
    id: string
    primary: string
    secondary?: string
    typeLabel: string
    searchMatches?: readonly string[]
}>

export type WorkspaceQuickSearchTaskParams = Readonly<{
    query: string
    limit: number
}>

/**
 * 工作区壳层与具体业务 feature 之间的纯桥接契约。
 * 宿主壳层通过 app/(workspace) 注入具体实现；独立页面或未包裹场景自动回退到安全默认值。
 */
export type WorkspaceShellBridgeValue = Readonly<{
    profile: WorkspaceProfileSnapshot
    navBadges: WorkspaceNavBadgeData
    searchQuickTasks: (
        params: WorkspaceQuickSearchTaskParams,
    ) => Promise<readonly WorkspaceQuickSearchTaskItem[]>
    inboxComponent?: React.ComponentType
}>

export const EMPTY_WORKSPACE_NAV_BADGE_COUNTS: WorkspaceNavBadgeCounts = {
    backgroundJobs: undefined,
    started: undefined,
    copied: undefined,
    todo: undefined,
    selectionActionable: 0,
    inquiryFeedback: undefined,
    procurementInquiry: undefined,
}

const EMPTY_WORKSPACE_SHELL_BRIDGE: WorkspaceShellBridgeValue = {
    profile: {
        data: undefined,
        isPending: false,
        isError: false,
        isSuccess: false,
        error: null,
    },
    navBadges: {
        counts: EMPTY_WORKSPACE_NAV_BADGE_COUNTS,
        errors: {},
    },
    searchQuickTasks: async () => [],
    inboxComponent: undefined,
}

const WorkspaceShellBridgeContext =
    React.createContext<WorkspaceShellBridgeValue>(EMPTY_WORKSPACE_SHELL_BRIDGE)

export function WorkspaceShellBridgeProvider({
    value,
    children,
}: {
    value: WorkspaceShellBridgeValue
    children: React.ReactNode
}) {
    return (
        <WorkspaceShellBridgeContext.Provider value={value}>
            {children}
        </WorkspaceShellBridgeContext.Provider>
    )
}

export function useWorkspaceShellBridge(): WorkspaceShellBridgeValue {
    return React.useContext(WorkspaceShellBridgeContext)
}
