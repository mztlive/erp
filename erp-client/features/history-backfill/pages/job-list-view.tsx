"use client"

import * as React from "react"
import { RefreshCwIcon, ShieldAlertIcon, XIcon } from "lucide-react"

import {
    DataFreshness,
    MetricItem,
    MetricStrip,
    PageHeader,
    PageScaffold,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { CreateBackfillSheet } from "@/features/history-backfill/components/create-backfill-sheet"
import { HistoryBackfillResultBanner as FormalResultBanner } from "@/features/history-backfill/components/history-backfill-result-banner"
import { JobTable } from "@/features/history-backfill/components/job-table"
import {
    useHistoryBackfillCommandMutation,
    useHistoryBackfillListQuery,
} from "@/features/history-backfill/hooks/queries"
import { newRequestId } from "@/features/history-backfill/lib/presentation"
import type { HistoryBackfillUrlState } from "@/features/history-backfill/lib/url-state"
import type {
    HistoryBackfillCommandResult,
    HistoryBackfillView,
} from "@/features/history-backfill/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { VIEW_LABEL } from "@/features/history-backfill/types"
import { formatDateTime } from "@/lib/datetime"

function JobListView({
    urlState,
    patchUrl,
    onOpenJob,
}: {
    urlState: HistoryBackfillUrlState
    patchUrl: (patch: Partial<HistoryBackfillUrlState>) => void
    onOpenJob: (id: string) => void
}) {
    const [createOpen, setCreateOpen] = React.useState(false)
    const [scopeAlertDismissed, setScopeAlertDismissed] = React.useState(false)
    const [actionResult, setActionResult] =
        React.useState<HistoryBackfillCommandResult | null>(null)
    const commandMutation = useHistoryBackfillCommandMutation()

    const listQuery = useHistoryBackfillListQuery({
        view: urlState.view,
        mallId: urlState.mallId,
        environment: urlState.environment,
        processingStatus: urlState.processingStatus,
        reportReviewStatus: urlState.reportReviewStatus,
        basis: urlState.basis,
        q: urlState.q,
        page: urlState.page,
        pageSize: 20,
    })

    const data = listQuery.data

    return (
        <PageScaffold>
            <PageHeader
                title="历史消费回填"
                metadata={
                    <DataFreshness
                        updatedAt={
                            data?.queriedAt
                                ? formatDateTime(data.queriedAt, "dateStyle")
                                : "刚刚"
                        }
                        dateTime={data?.queriedAt}
                        state={listQuery.isFetching ? "stale" : "fresh"}
                        label="回填任务"
                    />
                }
                actions={
                    <Button
                        id="operations-history-backfill-list-create"
                        type="button"
                        className="max-sm:hidden"
                        onClick={() => setCreateOpen(true)}
                    >
                        创建回填任务
                    </Button>
                }
            />

            <div className="flex flex-wrap items-center gap-2">
                <Button
                    id="operations-history-backfill-list-refresh"
                    type="button"
                    size="sm"
                    variant="ghost"
                    disabled={listQuery.isFetching}
                    onClick={() => void listQuery.refetch()}
                >
                    <RefreshCwIcon className="size-4" aria-hidden />
                    刷新
                </Button>
            </div>

            <FormalResultBanner result={actionResult} />

            <Tabs
                value={urlState.view}
                onValueChange={(v) => {
                    if (v == null) return
                    patchUrl({ view: v as HistoryBackfillView, page: 1 })
                }}
            >
                <TabsList>
                    {(Object.keys(VIEW_LABEL) as HistoryBackfillView[]).map(
                        (v) => (
                            <TabsTrigger
                                key={v}
                                id={`operations-history-backfill-list-view-${toAutomationIdSegment(v)}-trigger`}
                                value={v}
                            >
                                {VIEW_LABEL[v]}
                            </TabsTrigger>
                        ),
                    )}
                </TabsList>
            </Tabs>

            <MetricStrip columns={5} aria-label="回填任务指标">
                <MetricItem
                    label="执行中"
                    value={data?.metrics.running ?? "—"}
                />
                <MetricItem
                    label="待归集"
                    value={data?.metrics.unattributed ?? "—"}
                />
                <MetricItem
                    label="重叠去重"
                    value={data?.metrics.deduplicated ?? "—"}
                />
                <MetricItem
                    label="未覆盖消费"
                    value={data?.metrics.noneConsumption ?? "—"}
                />
                <MetricItem
                    label="失败项"
                    value={data?.metrics.failed ?? "—"}
                />
            </MetricStrip>

            {!scopeAlertDismissed ? (
                <Alert>
                    <ShieldAlertIcon />
                    <AlertTitle className="flex items-center justify-between gap-2">
                        范围与敏感边界
                        <button
                            id="operations-history-backfill-list-scope-alert-close"
                            type="button"
                            aria-label="关闭提示"
                            className="text-muted-foreground hover:text-foreground"
                            onClick={() => setScopeAlertDismissed(true)}
                        >
                            <XIcon className="size-4" aria-hidden />
                        </button>
                    </AlertTitle>
                    <AlertDescription>
                        从范围起点至截止时点（截止时点当天除外），截止时点当天发生的记录不进历史回填。技术处理完成
                        ≠ 报告已确认 ≠
                        全历史业务完成。页面与导出不含卡号、卡密、绑定手机、完整地址或原始消息内容。
                    </AlertDescription>
                </Alert>
            ) : null}

            <JobTable
                listQuery={listQuery}
                urlState={urlState}
                patchUrl={patchUrl}
                onOpenJob={onOpenJob}
            />

            <CreateBackfillSheet
                open={createOpen}
                onOpenChange={setCreateOpen}
                context={data?.createContext}
                pending={commandMutation.isPending}
                result={actionResult}
                onSubmit={async () => {
                    const ctx = data?.createContext
                    if (!ctx) return
                    const operationId = newRequestId("op")
                    const idempotencyKey = newRequestId("idem_create")
                    const result = await commandMutation.mutateAsync({
                        action: "CREATE_DRAFT",
                        cutoverId: ctx.cutoverId,
                        rangeStart: ctx.requiredHistoryStart,
                        rangeEnd: ctx.rangeEnd,
                        operationId,
                        idempotencyKey,
                    })
                    setActionResult(result)
                    if (result.status === "COMMITTED" && result.jobId) {
                        setCreateOpen(false)
                        onOpenJob(result.jobId)
                    }
                }}
            />
        </PageScaffold>
    )
}

export { JobListView }
