"use client"

import { useRef, useState } from "react"
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
import {
    WorkspaceOverviewControls,
    WorkspaceOverviewMetrics,
    WorkspaceInsights,
} from "../components/workspace-overview"
import { useWorkspaceOverview } from "../hooks/use-workspace-overview"
import { useWorkspaceHome } from "../hooks/use-workspace-home"
import { deriveWorkItemsFreshness } from "../lib/freshness"
import type { OverviewMetric } from "../lib/overview"
import { WorkspaceOverviewDetail } from "../components/workspace-overview-detail"

export function WorkspaceHomePage() {
    const params = useSearchParams()
    const profile = useAccountProfileQuery().data
    if (params.get("view") === "copied") return <ApprovalCopiesPage />
    return (
        <WorkspaceTaskHome
            key={`${profile?.userid}:${profile?.policy_version}:${profile?.organization_version}:${profile?.permissions.join(",")}`}
        />
    )
}
function WorkspaceTaskHome() {
    return <WorkspaceHomeView home={useWorkspaceHome()} />
}

/** 单页承载岗位概览、任务队列与业务分析；事项沿用独立详情页。 */
export function WorkspaceHomeView({
    home,
}: {
    home: ReturnType<typeof useWorkspaceHome>
}) {
    const profile = home.accountProfileQuery.data
    const overview = useWorkspaceOverview(
        profile,
        home.urlState.overviewView,
        home.urlState.overviewPeriod,
    )
    const [metric, setMetric] = useState<OverviewMetric | null>(null)
    // Sheet 壳常驻；先改变 open，关闭动画完成后再清理正文与恢复焦点。
    const [metricOpen, setMetricOpen] = useState(false)
    const navigatingToTask = useRef(false)
    const focusOrigin = useRef<HTMLElement | null>(null)
    const rememberFocus = () => {
        focusOrigin.current =
            document.activeElement instanceof HTMLElement
                ? document.activeElement
                : null
    }
    const restoreFocus = () => {
        requestAnimationFrame(() =>
            (focusOrigin.current?.isConnected
                ? focusOrigin.current
                : document.getElementById("workspace-queue-heading")
            )?.focus({ preventScroll: true }),
        )
    }
    const openMetric = (next: OverviewMetric) => {
        navigatingToTask.current = false
        rememberFocus()
        setMetric(next)
        setMetricOpen(true)
    }
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
                title="工作台"
                actions={
                    <div className="flex flex-wrap items-center gap-3">
                        <WorkspaceOverviewControls
                            overview={overview}
                            onChange={(view, preset) => {
                                setMetric(null)
                                home.onOverviewChange(view, preset)
                            }}
                        />
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
                            disabled={
                                home.refreshing ||
                                overview.queries.some(
                                    (query) => query.isFetching,
                                )
                            }
                            onClick={home.refresh}
                        >
                            <RefreshCwIcon />
                            刷新
                        </Button>
                    </div>
                }
            />
            <WorkspaceOverviewMetrics
                overview={overview}
                onSelect={(next) => {
                    const index = overview.metrics.findIndex(
                        (item) => item.key === next.key,
                    )
                    if (overview.queries[index]?.isError)
                        void overview.queries[index]?.refetch()
                    openMetric(next)
                }}
            />
            <div className="grid min-h-0 flex-1 grid-cols-1 items-start gap-5 xl:grid-cols-[minmax(0,2fr)_minmax(18rem,1fr)]">
                <WorkspaceQueuePanel
                    home={home}
                    onSelectTask={home.onSelectTask}
                />
                <WorkspaceInsights
                    queueFailed={home.dashboardQuery.isError}
                    queueLoading={home.dashboardQuery.isFetching && !home.view}
                    overview={overview}
                    queue={home.dashboardQuery.isError ? undefined : home.view}
                    onMetricClick={home.onMetricClick}
                    onSelect={openMetric}
                />
            </div>
            <WorkspaceOverviewDetail
                open={metricOpen}
                metric={metric}
                profile={profile}
                onClose={() => setMetricOpen(false)}
                onClosed={() => {
                    if (metricOpen || !metric) return
                    setMetric(null)
                    if (!navigatingToTask.current) restoreFocus()
                }}
                onTask={(id) => {
                    navigatingToTask.current = true
                    setMetricOpen(false)
                    home.onSelectTask(id)
                }}
            />
        </PageScaffold>
    )
}
