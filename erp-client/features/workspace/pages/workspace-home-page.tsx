"use client"

import { useSearchParams } from "next/navigation"
import { RefreshCwIcon } from "lucide-react"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { ApprovalCopiesPage } from "@/features/approval-copies/page"
import {
    BusinessFailureState,
    DataFreshness,
    PageHeader,
    PageScaffold,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { WorkspaceQueuePanel } from "../components/workspace-queue-panel"
import { useWorkspaceHome } from "../hooks/use-workspace-home"
import { deriveWorkItemsFreshness } from "../lib/freshness"
import { RoleWorkbenchEntry } from "./role-workbench-entry"
import { hasWorkspaceQueueParams } from "@/lib/workspace-detail-href"

export function WorkspaceHomePage() {
    const params = useSearchParams()
    const profile = useAccountProfileQuery().data
    if (params.get("view") === "copied") return <ApprovalCopiesPage />
    if (!hasWorkspaceQueueParams(params)) return <RoleWorkbenchEntry />
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
        <PageScaffold density="compact" className="min-h-0 gap-5 overflow-auto">
            <PageHeader
                title="我的全部待办"
                actions={
                    <div className="flex flex-wrap items-center gap-3">
                        {freshness && (
                            <DataFreshness
                                updatedAt={freshness.updatedAtLabel}
                                dateTime={freshness.dateTime}
                                state={freshness.state}
                                statusLabel={freshness.statusLabel}
                            />
                        )}
                        <Button
                            id="workspace-home-refresh"
                            variant="ghost"
                            size="sm"
                            disabled={home.refreshing}
                            onClick={home.refresh}
                        >
                            <RefreshCwIcon />
                            刷新
                        </Button>
                    </div>
                }
            />
            <WorkspaceQueuePanel home={home} onSelectTask={home.onSelectTask} />
        </PageScaffold>
    )
}
