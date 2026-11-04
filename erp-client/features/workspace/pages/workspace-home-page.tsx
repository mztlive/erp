"use client"

import { usePathname } from "@/lib/navigation"
import { WORKSPACE_QUEUE_ROUTES } from "@/lib/workspace-queue-routes"
import { RefreshCwIcon } from "lucide-react"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { BusinessFailureState, PageScaffold } from "@/components/business"
import {
    ListWorkspaceHeader,
    listWorkspaceStyles,
} from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { WorkspaceQueuePanel } from "../components/workspace-queue-panel"
import { useWorkspaceHome } from "../hooks/use-workspace-home"
import { deriveWorkItemsFreshness } from "../lib/freshness"
import { RoleWorkbenchEntry } from "./role-workbench-entry"

const QUEUE_DESCRIPTIONS = {
    inbox: "查看并处理需要你办理的事项。",
    managed: "查看责任范围内的待办事项。",
    started: "查看已发起审批的当前进度与结果。",
    history: "查看已处理的事项与处理结果。",
} as const

export function WorkspaceHomePage() {
    const pathname = usePathname()
    const profile = useAccountProfileQuery().data
    if (pathname === "/workspace") return <RoleWorkbenchEntry />
    return (
        <WorkspaceTaskHome
            key={`${profile?.userid}:${profile?.policy_version}:${profile?.organization_version}:${profile?.permissions.join(",")}`}
        />
    )
}
function WorkspaceTaskHome() {
    return <WorkspaceHomeView home={useWorkspaceHome()} />
}

/** 统一待办保持全部责任范围；岗位筛选只通过显式查询参数进入。 */
export function WorkspaceHomeView({
    home,
}: {
    home: ReturnType<typeof useWorkspaceHome>
}) {
    const freshness = home.view
        ? deriveWorkItemsFreshness(home.view.freshness, {
              refreshing: home.refreshing,
          })
        : undefined
    if (home.accountProfileQuery.isError)
        return (
            <PageScaffold>
                <BusinessFailureState
                    error={home.accountProfileQuery.error}
                    onRetry={home.refresh}
                />
            </PageScaffold>
        )
    return (
        <PageScaffold
            density="compact"
            className={`${listWorkspaceStyles.page} min-h-0 overflow-auto`}
        >
            <ListWorkspaceHeader
                eyebrow="工作"
                title={
                    home.urlState.view === "managed"
                        ? "范围内待办"
                        : WORKSPACE_QUEUE_ROUTES[home.urlState.view].title
                }
                description={
                    <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
                        <span>{QUEUE_DESCRIPTIONS[home.urlState.view]}</span>
                        <span role="status" className="text-xs">
                            {home.dashboardQuery.isError ? (
                                "更新失败，请重试"
                            ) : home.refreshing ? (
                                "正在更新…"
                            ) : freshness ? (
                                <time dateTime={freshness.dateTime}>
                                    更新于 {freshness.updatedAtLabel}
                                </time>
                            ) : (
                                "正在查询"
                            )}
                        </span>
                    </div>
                }
            >
                <Button
                    id="workspace-home-refresh"
                    variant="outline"
                    disabled={home.refreshing}
                    onClick={home.refresh}
                >
                    <RefreshCwIcon aria-hidden="true" />
                    刷新
                </Button>
            </ListWorkspaceHeader>
            <WorkspaceQueuePanel home={home} onSelectTask={home.onSelectTask} />
        </PageScaffold>
    )
}
