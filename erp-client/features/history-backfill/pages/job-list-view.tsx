"use client"

import * as React from "react"
import type { ColumnDef, PaginationState } from "@tanstack/react-table"
import { RefreshCwIcon, SearchIcon, ShieldAlertIcon, XIcon } from "lucide-react"

import {
    BusinessFailureState,
    BusinessStatusBadge,
    BusinessTableFrame,
    DataFreshness,
    DataTable,
    ListToolbar,
    MetricItem,
    MetricStrip,
    OptionCombobox,
    PageHeader,
    PageScaffold,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { MallSearchCombobox } from "@/features/entity-selectors"
import { CreateBackfillSheet } from "@/features/history-backfill/components/create-backfill-sheet"
import { HistoryBackfillResultBanner as FormalResultBanner } from "@/features/history-backfill/components/history-backfill-result-banner"
import { newRequestId } from "@/features/history-backfill/presentation"
import {
    useHistoryBackfillCommandMutation,
    useHistoryBackfillListQuery,
} from "@/features/history-backfill/queries"
import type {
    CostBasis,
    HistoryBackfillCommandResult,
    HistoryBackfillEnvironment,
    HistoryBackfillListItem,
    HistoryBackfillProcessingStatus,
    HistoryBackfillReportReviewStatus,
    HistoryBackfillView,
} from "@/features/history-backfill/types"
import {
    COST_BASIS_LABEL,
    ENVIRONMENT_LABEL,
    PROCESSING_STATUS_LABEL,
    PROCESSING_STATUS_TONE,
    REPORT_REVIEW_STATUS_LABEL,
    REPORT_REVIEW_STATUS_TONE,
    VIEW_LABEL,
} from "@/features/history-backfill/types"
import type { HistoryBackfillUrlState } from "@/features/history-backfill/url-state"
import { formatDateTime } from "@/lib/datetime"

function JobListView({
    urlState,
    patchUrl,
    onOpenJob,
}: {
    urlState: HistoryBackfillUrlState
    patchUrl: (patch: Partial<HistoryBackfillUrlState>) => void
    onOpenJob: (id: string) => void
    pathname: string
}) {
    const [qDraft, setQDraft] = React.useState(urlState.q ?? "")
    const searchInputRef = React.useRef<HTMLInputElement | null>(null)
    const [createOpen, setCreateOpen] = React.useState(false)
    const [scopeAlertDismissed, setScopeAlertDismissed] = React.useState(false)
    const [actionResult, setActionResult] =
        React.useState<HistoryBackfillCommandResult | null>(null)
    const commandMutation = useHistoryBackfillCommandMutation()

    React.useEffect(() => {
        setQDraft(urlState.q ?? "")
    }, [urlState.q])

    // P3 搜索：300ms 防抖写 URL，Enter 兜底，/ 聚焦
    React.useEffect(() => {
        const handle = globalThis.setTimeout(() => {
            if (qDraft.trim() === (urlState.q ?? "")) return
            patchUrl({ q: qDraft.trim() || undefined, page: 1 })
        }, 300)
        return () => globalThis.clearTimeout(handle)
        // eslint-disable-next-line react-hooks/exhaustive-deps -- patchUrl 以当前 URL 快照为准
    }, [qDraft])

    React.useEffect(() => {
        const onKey = (event: KeyboardEvent) => {
            if (
                event.key !== "/" ||
                event.metaKey ||
                event.ctrlKey ||
                event.altKey
            ) {
                return
            }
            const target = event.target as HTMLElement | null
            const tag = target?.tagName
            if (
                tag === "INPUT" ||
                tag === "TEXTAREA" ||
                tag === "SELECT" ||
                target?.isContentEditable
            ) {
                return
            }
            event.preventDefault()
            searchInputRef.current?.focus()
        }
        window.addEventListener("keydown", onKey)
        return () => window.removeEventListener("keydown", onKey)
    }, [])

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

    const columns = React.useMemo<ColumnDef<HistoryBackfillListItem>[]>(
        () => [
            {
                id: "jobNo",
                header: "任务号",
                cell: ({ row }) => (
                    <Button
                        variant="link"
                        className="h-auto p-0 font-mono text-sm"
                        onClick={() => onOpenJob(row.original.id)}
                    >
                        {row.original.jobNo}
                    </Button>
                ),
            },
            {
                id: "mall",
                header: "商城",
                cell: ({ row }) => (
                    <div className="space-y-0.5">
                        <div className="text-sm">{row.original.mallName}</div>
                        <Badge
                            variant={
                                row.original.environment === "production"
                                    ? "destructive"
                                    : "secondary"
                            }
                            className="text-2xs"
                        >
                            {ENVIRONMENT_LABEL[row.original.environment]}
                        </Badge>
                    </div>
                ),
            },
            {
                id: "range",
                header: "范围起点至截止时点",
                cell: ({ row }) => (
                    <span className="num font-mono text-xs">
                        {row.original.rangeLabel}
                    </span>
                ),
            },
            {
                id: "processing",
                header: "处理状态",
                cell: ({ row }) => (
                    <BusinessStatusBadge
                        context="list"
                        label={
                            PROCESSING_STATUS_LABEL[
                                row.original.processingStatus
                            ]
                        }
                        tone={
                            PROCESSING_STATUS_TONE[
                                row.original.processingStatus
                            ]
                        }
                    />
                ),
            },
            {
                id: "reportReview",
                header: "报告确认",
                cell: ({ row }) => (
                    <BusinessStatusBadge
                        context="list"
                        label={
                            REPORT_REVIEW_STATUS_LABEL[
                                row.original.reportReviewStatus
                            ]
                        }
                        tone={
                            REPORT_REVIEW_STATUS_TONE[
                                row.original.reportReviewStatus
                            ]
                        }
                    />
                ),
            },
            {
                id: "progress",
                header: "进度",
                cell: ({ row }) => (
                    <span className="num text-sm">
                        {row.original.progressLabel}
                    </span>
                ),
            },
            {
                id: "dedupe",
                header: "去重",
                cell: ({ row }) => (
                    <span className="num text-sm">
                        {row.original.deduplicatedCount.toLocaleString("zh-CN")}
                    </span>
                ),
            },
            {
                id: "unattr",
                header: "未归集",
                cell: ({ row }) => (
                    <span className="num text-sm">
                        {row.original.unattributedCount.toLocaleString("zh-CN")}
                    </span>
                ),
            },
            {
                id: "cost",
                header: "成本覆盖",
                cell: ({ row }) => (
                    <span className="text-xs">
                        {row.original.costCoverageLabel}
                    </span>
                ),
            },
            {
                id: "actions",
                header: "操作",
                cell: ({ row }) => (
                    <Button
                        type="button"
                        size="sm"
                        variant="outline"
                        onClick={() => onOpenJob(row.original.id)}
                    >
                        打开
                    </Button>
                ),
            },
        ],
        [onOpenJob],
    )

    const [pagination, setPagination] = React.useState<PaginationState>({
        pageIndex: Math.max(0, urlState.page - 1),
        pageSize: 20,
    })

    React.useEffect(() => {
        setPagination((p) => ({
            ...p,
            pageIndex: Math.max(0, urlState.page - 1),
        }))
    }, [urlState.page])

    const hasListFilters = Boolean(
        urlState.mallId ||
        urlState.processingStatus ||
        urlState.reportReviewStatus ||
        urlState.basis ||
        urlState.q,
    )

    const clearListFilters = () => {
        setQDraft("")
        patchUrl({
            mallId: undefined,
            processingStatus: undefined,
            reportReviewStatus: undefined,
            basis: undefined,
            q: undefined,
            page: 1,
        })
    }

    return (
        <PageScaffold>
            <PageHeader
                title="历史消费回填"
                breadcrumbs={[
                    {
                        id: "gov",
                        label: "治理",
                        href: "/governance/history-backfill",
                        current: false,
                    },
                    { id: "hb", label: "历史消费回填", current: true },
                ]}
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
                            <TabsTrigger key={v} value={v}>
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

            <BusinessTableFrame
                title="回填任务"
                description={
                    listQuery.isError
                        ? "列表加载失败，可调整筛选后重试"
                        : `共 ${data?.totalCount ?? 0} 个任务 · 处理状态与报告确认状态分列`
                }
                toolbar={
                    <ListToolbar
                        search={
                            <form
                                className="flex gap-1"
                                onSubmit={(e) => {
                                    e.preventDefault()
                                    patchUrl({
                                        q: qDraft.trim() || undefined,
                                        page: 1,
                                    })
                                }}
                            >
                                <InputGroup>
                                    <InputGroupAddon>
                                        <SearchIcon aria-hidden="true" />
                                    </InputGroupAddon>
                                    <InputGroupInput
                                        ref={searchInputRef}
                                        value={qDraft}
                                        onChange={(e) =>
                                            setQDraft(e.target.value)
                                        }
                                        placeholder="任务号 / 商城"
                                        aria-label="搜索"
                                    />
                                </InputGroup>
                            </form>
                        }
                        filters={
                            <>
                                <MallSearchCombobox
                                    value={urlState.mallId ?? null}
                                    onValueChange={(v) => {
                                        patchUrl({
                                            mallId: v ?? undefined,
                                            page: 1,
                                        })
                                    }}
                                    inputClassName="w-[10rem]"
                                    size="sm"
                                    placeholder="商城：全部"
                                    aria-label="商城"
                                    allowClear={false}
                                />
                                <OptionCombobox
                                    value={urlState.processingStatus ?? "all"}
                                    onValueChange={(v) => {
                                        if (v == null) return
                                        patchUrl({
                                            processingStatus:
                                                v === "all"
                                                    ? undefined
                                                    : (v as HistoryBackfillProcessingStatus),
                                            page: 1,
                                        })
                                    }}
                                    options={[
                                        { value: "all", label: "全部处理状态" },
                                        ...(
                                            Object.keys(
                                                PROCESSING_STATUS_LABEL,
                                            ) as HistoryBackfillProcessingStatus[]
                                        ).map((s) => ({
                                            value: s,
                                            label: PROCESSING_STATUS_LABEL[s],
                                        })),
                                    ]}
                                    inputClassName="w-[11rem]"
                                    size="sm"
                                    placeholder="处理状态：全部"
                                    aria-label="处理状态"
                                    allowClear={false}
                                />
                                <OptionCombobox
                                    value={urlState.environment ?? "all"}
                                    onValueChange={(v) => {
                                        if (v == null) return
                                        patchUrl({
                                            environment:
                                                v === "all"
                                                    ? undefined
                                                    : (v as HistoryBackfillEnvironment),
                                            page: 1,
                                        })
                                    }}
                                    options={[
                                        { value: "all", label: "全部环境" },
                                        {
                                            value: "production",
                                            label: "生产环境",
                                        },
                                        {
                                            value: "verification",
                                            label: "验证环境",
                                        },
                                    ]}
                                    inputClassName="w-[9rem]"
                                    size="sm"
                                    placeholder="环境：全部"
                                    aria-label="环境"
                                    allowClear={false}
                                />
                            </>
                        }
                        secondary={
                            <>
                                <OptionCombobox
                                    value={urlState.reportReviewStatus ?? "all"}
                                    onValueChange={(v) => {
                                        if (v == null) return
                                        patchUrl({
                                            reportReviewStatus:
                                                v === "all"
                                                    ? undefined
                                                    : (v as HistoryBackfillReportReviewStatus),
                                            page: 1,
                                        })
                                    }}
                                    options={[
                                        { value: "all", label: "全部确认状态" },
                                        ...(
                                            Object.keys(
                                                REPORT_REVIEW_STATUS_LABEL,
                                            ) as HistoryBackfillReportReviewStatus[]
                                        ).map((s) => ({
                                            value: s,
                                            label: REPORT_REVIEW_STATUS_LABEL[
                                                s
                                            ],
                                        })),
                                    ]}
                                    inputClassName="w-[11rem]"
                                    size="sm"
                                    placeholder="报告确认：全部"
                                    aria-label="报告确认"
                                    allowClear={false}
                                />
                                <OptionCombobox
                                    value={urlState.basis ?? "all"}
                                    onValueChange={(v) => {
                                        if (v == null) return
                                        patchUrl({
                                            basis:
                                                v === "all"
                                                    ? undefined
                                                    : (v as CostBasis),
                                            page: 1,
                                        })
                                    }}
                                    options={[
                                        { value: "all", label: "全部口径" },
                                        ...(
                                            Object.keys(
                                                COST_BASIS_LABEL,
                                            ) as CostBasis[]
                                        ).map((b) => ({
                                            value: b,
                                            label: COST_BASIS_LABEL[b],
                                        })),
                                    ]}
                                    inputClassName="w-[10rem]"
                                    size="sm"
                                    placeholder="成本口径：全部"
                                    aria-label="成本口径"
                                    allowClear={false}
                                />
                            </>
                        }
                        actions={
                            <>
                                <span
                                    className="text-xs text-muted-foreground"
                                    aria-live="polite"
                                >
                                    共{" "}
                                    {(data?.totalCount ?? 0).toLocaleString(
                                        "zh-CN",
                                    )}{" "}
                                    个
                                </span>
                                {hasListFilters ? (
                                    <Button
                                        type="button"
                                        size="sm"
                                        variant="ghost"
                                        onClick={clearListFilters}
                                    >
                                        清除筛选
                                    </Button>
                                ) : null}
                            </>
                        }
                    />
                }
                table={
                    listQuery.isError ? (
                        <BusinessFailureState
                            title="任务列表加载失败"
                            error={listQuery.error}
                            className="rounded-lg border-0 bg-transparent shadow-none ring-0"
                            action={
                                <Button
                                    type="button"
                                    variant="secondary"
                                    className="rounded-lg shadow-none"
                                    onClick={() => void listQuery.refetch()}
                                >
                                    重试
                                </Button>
                            }
                        />
                    ) : (
                        <DataTable
                            data={[...(data?.rows ?? [])]}
                            columns={columns}
                            getRowId={(row) => row.id}
                            rowCount={data?.totalCount ?? 0}
                            pagination={pagination}
                            onPaginationChange={(next) => {
                                setPagination(next)
                                patchUrl({ page: next.pageIndex + 1 })
                            }}
                            layout="flush"
                            density="compact"
                            loading={listQuery.isPending}
                        />
                    )
                }
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
