"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import type { PaginationState, RowSelectionState } from "@tanstack/react-table"
import {
    ExternalLinkIcon,
    RefreshCwIcon,
    SearchIcon,
    ShieldAlertIcon,
    TriangleAlertIcon,
} from "lucide-react"

import {
    BackgroundJobProgress,
    BusinessEmptyState,
    BusinessFailureState,
    BusinessStatusBadge,
    BusinessTableFrame,
    DataFreshness,
    DataTable,
    DocumentHeader,
    DocumentSection,
    DocumentSummary,
    FormalActionConfirmDialog,
    FormalActionResult,
    ListToolbar,
    MetricFilterItem,
    MetricStrip,
    OptionCombobox,
    PageActions,
    PageHeader,
    PageScaffold,
    QuickPreviewSheet,
    RevisionTimeline,
    StatusTrackSummary,
    surfaceInsetClassName,
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
import { BULK_SELECTION_LIMIT } from "@/features/execution-projections/api/projections"
import { WhitelistContentGrid } from "@/features/execution-projections/components/whitelist-content-grid"
import {
    useBulkProjectionCommandMutation,
    useExecutionProjectionDetailQuery,
    useExecutionProjectionListQuery,
    useProjectionDeliveryCommandMutation,
} from "@/features/execution-projections/hooks/queries"
import {
    useExecutionProjectionColumns,
    type ProjectionRowCommandAction,
} from "@/features/execution-projections/hooks/use-execution-projection-columns"
import { commandToResultState } from "@/features/execution-projections/lib/result-state"
import {
    parseLatency,
    parseMetric,
    parseRecon,
    parseSource,
    w29Href,
} from "@/features/execution-projections/lib/url-state"
import type {
    BulkProjectionJob,
    DeliveryStatus,
    ExecutionProjectionRow,
    LatencyBand,
} from "@/features/execution-projections/types"
import {
    DELIVERY_STATUS_LABEL,
    LATENCY_LABEL,
    RECONCILIATION_LABEL,
    SOURCE_LABEL,
} from "@/features/execution-projections/types"
import { cn } from "@/lib/utils"
import { openWorkspaceLabel, resultText, versionText } from "@/lib/ui-text"
import { formatDateTime } from "@/lib/datetime"
import { getErrorMessage } from "@/lib/api/errors"
import { type ResultState } from "@/components/business/feedback"

type PendingAction =
    | {
          kind: "QUERY_RESULT"
          row: ExecutionProjectionRow
          objectVersion: string
      }
    | {
          kind: "RETRY"
          row: ExecutionProjectionRow
          objectVersion: string
      }
    | {
          kind: "ESCALATE"
          row: ExecutionProjectionRow
          objectVersion: string
      }
    | {
          kind: "BULK_QUERY"
          ids: string[]
      }
    | {
          kind: "BULK_RETRY"
          ids: string[]
      }
    | null

export function ExecutionProjectionsPage() {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()

    const q = searchParams.get("q") ?? ""
    const mallId = searchParams.get("mall") ?? "all"
    const deliveryStatus = searchParams.get("deliveryStatus") ?? "all"
    const source = parseSource(searchParams.get("source"))
    const latency = parseLatency(searchParams.get("latency"))
    const reconciliation = parseRecon(searchParams.get("reconciliation"))
    const metric = parseMetric(searchParams.get("metric"))
    const projectionId = searchParams.get("projectionId") ?? undefined
    const revisionId = searchParams.get("revision") ?? undefined
    const page = Math.max(1, Number(searchParams.get("page") ?? "1") || 1)
    const pageSize = Math.max(
        1,
        Math.min(50, Number(searchParams.get("size") ?? "8") || 8),
    )

    const listQueryInput = React.useMemo(
        () => ({
            q: q || undefined,
            mallId: mallId === "all" ? undefined : mallId,
            deliveryStatus:
                deliveryStatus === "all" ? undefined : deliveryStatus,
            source,
            latency,
            reconciliation,
            metric,
            page,
            pageSize,
        }),
        [
            q,
            mallId,
            deliveryStatus,
            source,
            latency,
            reconciliation,
            metric,
            page,
            pageSize,
        ],
    )

    const listQuery = useExecutionProjectionListQuery(listQueryInput)
    const detailQuery = useExecutionProjectionDetailQuery(
        projectionId,
        revisionId,
    )
    const commandMutation = useProjectionDeliveryCommandMutation()
    const bulkMutation = useBulkProjectionCommandMutation()

    const [searchDraft, setSearchDraft] = React.useState(q)
    const searchInputRef = React.useRef<HTMLInputElement | null>(null)
    React.useEffect(() => {
        // URL 回填时保留焦点保护：输入中不被 URL 旧值覆盖草稿
        const el = searchInputRef.current
        if (el && document.activeElement === el) return
        setSearchDraft(q)
    }, [q])

    // P3：搜索 300ms 防抖自动写 URL（replace），Enter 兜底，`/` 聚焦
    React.useEffect(() => {
        const handle = globalThis.setTimeout(() => {
            if (searchDraft.trim() === q) return
            replaceParams({ q: searchDraft.trim() || null, page: "1" })
        }, 300)
        return () => globalThis.clearTimeout(handle)
        // eslint-disable-next-line react-hooks/exhaustive-deps -- replaceParams 以当前 URL 快照为准
    }, [searchDraft])

    React.useEffect(() => {
        const onKey = (event: KeyboardEvent) => {
            if (
                event.key !== "/" ||
                event.metaKey ||
                event.ctrlKey ||
                event.altKey
            )
                return
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

    const [rowSelection, setRowSelection] = React.useState<RowSelectionState>(
        {},
    )
    const [result, setResult] = React.useState<ResultState>(null)
    const [bulkJob, setBulkJob] = React.useState<BulkProjectionJob | null>(null)
    const [pendingAction, setPendingAction] =
        React.useState<PendingAction>(null)
    const [objectTab, setObjectTab] = React.useState("overview")
    const resultRef = React.useRef<HTMLDivElement>(null)

    const replaceParams = React.useCallback(
        (patch: Record<string, string | null | undefined>) => {
            const next = new URLSearchParams(searchParams.toString())
            for (const [key, value] of Object.entries(patch)) {
                if (value == null || value === "" || value === "all")
                    next.delete(key)
                else next.set(key, value)
            }
            const qs = next.toString()
            router.replace(qs ? `${pathname}?${qs}` : pathname, {
                scroll: false,
            })
        },
        [pathname, router, searchParams],
    )

    const hasActiveFilters = Boolean(
        q ||
        mallId !== "all" ||
        deliveryStatus !== "all" ||
        source !== "all" ||
        latency !== "all" ||
        reconciliation !== "all" ||
        metric !== "all",
    )

    // P4：清搜索词 + 全部筛选参数 + 分页回 1（保留视图/排序/导航上下文参数）
    const clearFilters = React.useCallback(() => {
        replaceParams({
            q: null,
            mall: null,
            deliveryStatus: null,
            source: null,
            latency: null,
            reconciliation: null,
            metric: null,
            page: null,
        })
    }, [replaceParams])

    const view = listQuery.data
    const rows = view?.rows ?? []
    const metrics = view?.metrics ?? []
    const total = view?.pageInfo.total ?? 0
    const selectedIds = React.useMemo(
        () => Object.keys(rowSelection).filter((id) => rowSelection[id]),
        [rowSelection],
    )
    const bulkOverLimit = selectedIds.length > BULK_SELECTION_LIMIT

    const pagination: PaginationState = {
        pageIndex: page - 1,
        pageSize,
    }

    React.useEffect(() => {
        if (result) {
            resultRef.current?.focus()
        }
    }, [result])

    const handleRowCommand = React.useCallback(
        (action: ProjectionRowCommandAction) => {
            setPendingAction(action)
        },
        [],
    )

    const columns = useExecutionProjectionColumns({
        replaceParams,
        commandPending: commandMutation.isPending,
        onRowCommand: handleRowCommand,
    })

    const openConfirmForRow = async (
        kind: "QUERY_RESULT" | "RETRY" | "ESCALATE",
        row: ExecutionProjectionRow,
        objectVersion: string,
    ) => {
        try {
            const detail =
                detailQuery.data?.identity.projectionId === row.projectionId
                    ? detailQuery.data
                    : null
            const version = detail?.objectVersion ?? objectVersion
            const result = await commandMutation.mutateAsync({
                projectionId: row.projectionId,
                projectionRevisionId: row.projectionRevisionId,
                deliveryId: row.delivery.deliveryId,
                action: kind,
                expectedObjectVersion: version,
                requestId: `req-${Date.now().toString(36)}`,
            })
            setResult(commandToResultState(result))
            setPendingAction(null)
        } catch (err) {
            const actionLabel =
                kind === "QUERY_RESULT"
                    ? "查询结果"
                    : kind === "RETRY"
                      ? "重试发送"
                      : "升级到接口错误中心"
            setResult({
                status: "blocked",
                title: resultText.operationBlocked,
                description: getErrorMessage(err, "请刷新后重试"),
                reference: row.projectionNo,
                facts: [
                    { label: "对象", value: row.salesOrderNo },
                    { label: "动作", value: actionLabel },
                ],
            })
            setPendingAction(null)
        }
    }

    const runBulk = async (kind: "BULK_QUERY" | "BULK_RETRY") => {
        try {
            const job = await bulkMutation.mutateAsync({
                action: kind,
                projectionIds: selectedIds,
                requestId: `bulk-${Date.now().toString(36)}`,
            })
            setBulkJob(job)
            setRowSelection({})
            setPendingAction(null)
            if (job.status === "failed") {
                setResult({
                    status: "blocked",
                    title: "批量操作被阻断",
                    description: job.nextAction,
                    reference: "bulk",
                    facts: [
                        {
                            label: "成功/跳过/失败/仍未知",
                            value: `${job.succeeded}/${job.skipped}/${job.failed}/${job.stillUnknown}`,
                        },
                    ],
                })
            }
        } catch (err) {
            setResult({
                status: "blocked",
                title: "批量操作被阻断",
                description: getErrorMessage(err, "请重试"),
                reference: "bulk",
                facts: [],
            })
            setPendingAction(null)
        }
    }

    const detail = detailQuery.data
    const objectOpen = Boolean(projectionId)

    if (listQuery.isPending && !view) {
        return (
            <PageScaffold density="compact">
                <PageHeader title="执行信息" description="正在加载列表…" />
                <div className="space-y-3" aria-busy="true" aria-label="加载中">
                    <div className="h-20 animate-pulse rounded-lg bg-muted" />
                    <div className="h-64 animate-pulse rounded-lg bg-muted" />
                </div>
            </PageScaffold>
        )
    }

    if (listQuery.isError) {
        return (
            <PageScaffold density="compact">
                <PageHeader title="执行信息" description="列表加载失败" />
                <BusinessFailureState
                    error={listQuery.error}
                    onRetry={() => void listQuery.refetch()}
                />
            </PageScaffold>
        )
    }

    return (
        <PageScaffold density="compact">
            <PageHeader
                title="执行信息"
                breadcrumbs={[
                    {
                        id: "com",
                        label: "商城与发布",
                        href: "/commerce/execution-projections",
                    },
                    { id: "ep", label: "执行信息", current: true },
                ]}
                metadata={
                    <DataFreshness
                        updatedAt={
                            view
                                ? formatDateTime(
                                      view.queriedAt,
                                      "monthDay",
                                      "passthrough",
                                  )
                                : "—"
                        }
                        dateTime={view?.queriedAt}
                        state={listQuery.isFetching ? "syncing" : "fresh"}
                        label="发送状态更新于"
                    />
                }
                actions={
                    <PageActions
                        actions={[
                            {
                                actionKey: "refresh",
                                label: "刷新",
                                icon: RefreshCwIcon,
                                variant: "ghost",
                                onClick: () => {
                                    void listQuery.refetch()
                                },
                            },
                            {
                                actionKey: "bulk-query",
                                label: "批量查询",
                                variant: "outline",
                                mobileVisibility: "hide",
                                disabled:
                                    selectedIds.length === 0 ||
                                    bulkOverLimit ||
                                    bulkMutation.isPending,
                                onClick: () =>
                                    setPendingAction({
                                        kind: "BULK_QUERY",
                                        ids: selectedIds,
                                    }),
                            },
                            {
                                actionKey: "bulk-retry",
                                label: "批量重试",
                                mobileVisibility: "hide",
                                disabled:
                                    selectedIds.length === 0 ||
                                    bulkOverLimit ||
                                    bulkMutation.isPending,
                                onClick: () =>
                                    setPendingAction({
                                        kind: "BULK_RETRY",
                                        ids: selectedIds,
                                    }),
                            },
                        ]}
                    />
                }
            />

            <Alert>
                <ShieldAlertIcon aria-hidden="true" />
                <AlertTitle>本页只读</AlertTitle>
                <AlertDescription>
                    执行信息由已生效销售版本自动形成；接收失败不影响销售记录与应收，内容变更须走销售变更单。本页支持查询结果、重试与升级到接口错误中心，不展示金额、税率、开票、应收与玩法等销售明细。
                </AlertDescription>
            </Alert>

            <div ref={resultRef} tabIndex={-1} className="outline-none">
                {result ? (
                    <FormalActionResult
                        status={
                            result.status === "failed"
                                ? "blocked"
                                : result.status
                        }
                        title={result.title}
                        description={result.description}
                        reference={result.reference}
                        facts={result.facts}
                        actions={
                            result.w29Href ? (
                                <Button
                                    type="button"
                                    size="sm"
                                    render={<Link href={result.w29Href} />}
                                >
                                    {openWorkspaceLabel("W29")}
                                </Button>
                            ) : null
                        }
                    />
                ) : null}
            </div>

            {bulkJob ? (
                <BackgroundJobProgress
                    mode="partialAllowed"
                    status={bulkJob.status}
                    total={bulkJob.total}
                    completed={bulkJob.completed}
                    succeeded={bulkJob.succeeded}
                    skipped={bulkJob.skipped + bulkJob.stillUnknown}
                    failed={bulkJob.failed}
                    label={
                        bulkJob.action === "BULK_RETRY"
                            ? "批量重试任务"
                            : "批量查询任务"
                    }
                    description={
                        <>
                            本次选择共 {bulkJob.total} 项。成功{" "}
                            {bulkJob.succeeded} · 跳过 {bulkJob.skipped} ·
                            仍未知 {bulkJob.stillUnknown} · 失败{" "}
                            {bulkJob.failed}。
                            {bulkJob.stillUnknown > 0
                                ? " 仍未知项未按成功处理、未计入已确认。"
                                : null}
                        </>
                    }
                />
            ) : null}

            <MetricStrip columns={5} aria-label="执行信息指标筛选">
                {metrics.map((m) => (
                    <MetricFilterItem
                        key={m.key}
                        label={m.label}
                        value={m.value}
                        detail={m.detail}
                        active={metric === m.key}
                        onClick={() =>
                            replaceParams({
                                metric: metric === m.key ? null : m.key,
                                page: "1",
                            })
                        }
                    />
                ))}
            </MetricStrip>

            {/* D24：ListToolbar 移入 frame 的 toolbar 槽；批量选择条进 selectionBar 槽（表格正上方） */}
            <BusinessTableFrame
                title="执行信息列表"
                description={
                    <span aria-live="polite">
                        销售单身份列与操作列固定；每页条数可在分页条切换。指标与列表数据均受权限范围控制。
                        {view?.filterSummary
                            ? ` 筛选：${view.filterSummary}`
                            : ""}
                    </span>
                }
                toolbar={
                    <ListToolbar
                        search={
                            <InputGroup className="max-w-sm">
                                <InputGroupAddon>
                                    <SearchIcon aria-hidden="true" />
                                </InputGroupAddon>
                                <InputGroupInput
                                    ref={searchInputRef}
                                    value={searchDraft}
                                    onChange={(e) =>
                                        setSearchDraft(e.target.value)
                                    }
                                    onKeyDown={(e) => {
                                        if (e.key === "Enter") {
                                            replaceParams({
                                                q: searchDraft.trim() || null,
                                                page: "1",
                                            })
                                        }
                                    }}
                                    placeholder="销售单号、客户"
                                    aria-label="搜索执行信息"
                                />
                            </InputGroup>
                        }
                        filters={
                            <>
                                <OptionCombobox
                                    aria-label="目标商城"
                                    value={mallId}
                                    onValueChange={(v) =>
                                        replaceParams({
                                            mall: v ?? "all",
                                            page: "1",
                                        })
                                    }
                                    options={[
                                        { value: "all", label: "全部商城" },
                                        ...(view?.malls ?? []).map((m) => ({
                                            value: m.id,
                                            label: m.name,
                                        })),
                                    ]}
                                    className="w-[9rem]"
                                    size="sm"
                                    allowClear={false}
                                    placeholder="全部商城"
                                />
                                <OptionCombobox
                                    aria-label="接收状态"
                                    value={deliveryStatus}
                                    onValueChange={(v) =>
                                        replaceParams({
                                            deliveryStatus: v ?? "all",
                                            page: "1",
                                        })
                                    }
                                    options={[
                                        { value: "all", label: "全部接收状态" },
                                        ...(
                                            [
                                                "UNKNOWN",
                                                "FAILED",
                                                "ESCALATED_MANUAL",
                                                "RETRYING",
                                                "SENDING",
                                                "PENDING",
                                                "ACKED",
                                            ] as DeliveryStatus[]
                                        ).map((s) => ({
                                            value: s,
                                            label: DELIVERY_STATUS_LABEL[s],
                                        })),
                                        {
                                            value: "UNKNOWN,FAILED,ESCALATED_MANUAL",
                                            label: "未知+失败+转人工",
                                        },
                                    ]}
                                    className="w-[11rem]"
                                    size="sm"
                                    allowClear={false}
                                    placeholder="全部接收状态"
                                />
                                <OptionCombobox
                                    aria-label="等待时长分组"
                                    value={latency}
                                    onValueChange={(v) =>
                                        replaceParams({
                                            latency: v ?? "all",
                                            page: "1",
                                        })
                                    }
                                    options={[
                                        {
                                            value: "all",
                                            label: "等待时长：全部",
                                        },
                                        ...(
                                            Object.keys(
                                                LATENCY_LABEL,
                                            ) as LatencyBand[]
                                        ).map((k) => ({
                                            value: k,
                                            label: LATENCY_LABEL[k],
                                        })),
                                    ]}
                                    className="w-[9rem]"
                                    size="sm"
                                    allowClear={false}
                                    placeholder="等待时长：全部"
                                />
                            </>
                        }
                        secondary={
                            <>
                                <OptionCombobox
                                    aria-label="版本差异"
                                    value={reconciliation}
                                    onValueChange={(v) =>
                                        replaceParams({
                                            reconciliation: v ?? "all",
                                            page: "1",
                                        })
                                    }
                                    options={[
                                        { value: "all", label: "对账：全部" },
                                        {
                                            value: "VERSION_MISMATCH",
                                            label: "仅版本差异",
                                        },
                                        { value: "MATCHED", label: "版本一致" },
                                    ]}
                                    className="w-[9rem]"
                                    size="sm"
                                    allowClear={false}
                                    placeholder="对账：全部"
                                />
                                <OptionCombobox
                                    aria-label="数据来源"
                                    value={source}
                                    onValueChange={(v) =>
                                        replaceParams({
                                            source: v ?? "all",
                                            page: "1",
                                        })
                                    }
                                    options={[
                                        { value: "all", label: "来源：全部" },
                                        {
                                            value: "ERP_SALES_REVISION",
                                            label: "ERP 销售版本",
                                        },
                                        {
                                            value: "MIGRATION_BASELINE",
                                            label: "迁移基线",
                                        },
                                    ]}
                                    className="w-[10rem]"
                                    size="sm"
                                    allowClear={false}
                                    placeholder="来源：全部"
                                />
                            </>
                        }
                        actions={
                            <span className="text-xs text-muted-foreground">
                                <span className="num">{total}</span> 条
                            </span>
                        }
                    />
                }
                selectionBar={
                    selectedIds.length > 0 ? (
                        <div
                            role="region"
                            aria-label="批量选择"
                            className={cn(
                                surfaceInsetClassName,
                                "flex flex-wrap items-center justify-between gap-2 px-3 py-2 text-sm",
                            )}
                        >
                            <span>
                                已选择{" "}
                                <span className="num font-medium">
                                    {selectedIds.length}
                                </span>{" "}
                                项（批量操作仅作用于显式选择，不含当前筛选全部）
                                {bulkOverLimit ? (
                                    <span className="ml-2 text-destructive">
                                        批量最多 {BULK_SELECTION_LIMIT}{" "}
                                        条，超出部分请分批
                                    </span>
                                ) : null}
                            </span>
                            <div className="flex flex-wrap gap-2">
                                <Button
                                    type="button"
                                    size="sm"
                                    variant="outline"
                                    onClick={() => setRowSelection({})}
                                >
                                    清除选择
                                </Button>
                                <Button
                                    type="button"
                                    size="sm"
                                    variant="outline"
                                    disabled={
                                        bulkOverLimit || bulkMutation.isPending
                                    }
                                    onClick={() =>
                                        setPendingAction({
                                            kind: "BULK_QUERY",
                                            ids: selectedIds,
                                        })
                                    }
                                >
                                    批量查询
                                </Button>
                                <Button
                                    type="button"
                                    size="sm"
                                    disabled={
                                        bulkOverLimit || bulkMutation.isPending
                                    }
                                    onClick={() =>
                                        setPendingAction({
                                            kind: "BULK_RETRY",
                                            ids: selectedIds,
                                        })
                                    }
                                >
                                    批量重试
                                </Button>
                            </div>
                        </div>
                    ) : undefined
                }
                table={
                    <DataTable
                        columns={columns}
                        data={rows}
                        getRowId={(row) => row.projectionId}
                        rowCount={total}
                        enableRowSelection
                        rowSelection={rowSelection}
                        onRowSelectionChange={setRowSelection}
                        onRowPreview={(row) =>
                            replaceParams({
                                projectionId: row.projectionId,
                                revision: null,
                            })
                        }
                        onRowOpen={(row) =>
                            replaceParams({
                                projectionId: row.projectionId,
                                revision: null,
                            })
                        }
                        pagination={pagination}
                        onPaginationChange={(next) => {
                            const sp = new URLSearchParams(
                                searchParams.toString(),
                            )
                            if (next.pageIndex <= 0) sp.delete("page")
                            else sp.set("page", String(next.pageIndex + 1))
                            if (next.pageSize === 8) sp.delete("size")
                            else sp.set("size", String(next.pageSize))
                            const qs = sp.toString()
                            router.replace(
                                qs ? `${pathname}?${qs}` : pathname,
                                {
                                    scroll: false,
                                },
                            )
                        }}
                        manualPagination
                        layout="flush"
                        density="compact"
                        pageSizeOptions={[8, 20, 50]}
                        defaultColumnPinning={{
                            left: ["select", "salesOrder"],
                            right: ["actions"],
                        }}
                        emptyState={
                            rows.length === 0 ? (
                                hasActiveFilters ? (
                                    <BusinessEmptyState
                                        kind="filter"
                                        title="没有匹配的执行信息"
                                        description={
                                            view?.filterSummary
                                                ? `当前筛选：${view.filterSummary}`
                                                : "可清除筛选或返回销售单查看协同。"
                                        }
                                        className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                                        action={
                                            <Button
                                                type="button"
                                                size="sm"
                                                variant="secondary"
                                                className="rounded-lg shadow-none"
                                                onClick={clearFilters}
                                            >
                                                清除筛选
                                            </Button>
                                        }
                                    />
                                ) : (
                                    // D24：无筛选时空态不引导「清除筛选」，避免误导
                                    <BusinessEmptyState
                                        kind="no-data"
                                        title="当前范围没有执行信息"
                                        description="销售记录尚未形成发送记录；新数据到达后会自动显示。"
                                        className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                                    />
                                )
                            ) : undefined
                        }
                    />
                }
            />

            <p className="text-xs text-muted-foreground">
                结果未知不计入「已确认」指标。
                {view?.defaultViewNote}
            </p>

            {/* 对象中心半屏 / 主区 */}
            <QuickPreviewSheet
                open={objectOpen}
                onOpenChange={(open) => {
                    if (!open)
                        replaceParams({ projectionId: null, revision: null })
                }}
                size="detail"
                title={
                    detail
                        ? `执行信息 · ${detail.identity.salesOrderNo}`
                        : "执行信息对象"
                }
                description={
                    detail
                        ? `${detail.identity.projectionNo} · ${detail.identity.targetMallName}`
                        : "加载中…"
                }
                identity={
                    detail ? (
                        <span className="num">
                            {detail.identity.projectionId}
                        </span>
                    ) : null
                }
            >
                {detailQuery.isPending ? (
                    <div className="h-48 animate-pulse rounded-lg bg-muted" />
                ) : detailQuery.isError ? (
                    <BusinessFailureState
                        error={detailQuery.error}
                        onRetry={() => void detailQuery.refetch()}
                    />
                ) : !detail ? (
                    <BusinessEmptyState
                        kind="no-data"
                        title="无法加载数据"
                        description="数据不存在。"
                    />
                ) : (
                    <div className="flex flex-col gap-4">
                        <DocumentHeader
                            density="compact"
                            title={detail.identity.salesOrderNo}
                            documentNumber={detail.identity.projectionNo}
                            version={`数据 v${detail.selectedRevision.revisionNo} · ERP v${detail.selectedRevision.salesOrderRevisionNo}`}
                            primaryStatus={{
                                label: detail.tracks.projectionDelivery.label,
                                tone: detail.tracks.projectionDelivery.tone,
                            }}
                            meta={
                                <span className="text-muted-foreground">
                                    {detail.identity.targetMallName}
                                </span>
                            }
                            statuses={[
                                {
                                    id: "sales-fact",
                                    label: "销售记录",
                                    status: {
                                        label: detail.tracks.salesFact.label,
                                        tone: detail.tracks.salesFact.tone,
                                    },
                                },
                                {
                                    id: "delivery",
                                    label: "信息发送",
                                    status: {
                                        label: detail.tracks.projectionDelivery
                                            .label,
                                        tone: detail.tracks.projectionDelivery
                                            .tone,
                                    },
                                },
                                {
                                    id: "mall",
                                    label: "商城确认",
                                    status: {
                                        label: detail.tracks.mallConfirm.label,
                                        tone: detail.tracks.mallConfirm.tone,
                                    },
                                },
                            ]}
                            primaryAction={
                                detail.allowedActions.includes(
                                    "QUERY_RESULT",
                                ) ? (
                                    <Button
                                        type="button"
                                        size="sm"
                                        disabled={commandMutation.isPending}
                                        onClick={() => {
                                            const row = rows.find(
                                                (r) =>
                                                    r.projectionId ===
                                                    detail.identity
                                                        .projectionId,
                                            )
                                            if (!row) return
                                            setPendingAction({
                                                kind: "QUERY_RESULT",
                                                row,
                                                objectVersion:
                                                    detail.objectVersion,
                                            })
                                        }}
                                    >
                                        查询结果
                                    </Button>
                                ) : undefined
                            }
                            secondaryActions={
                                <>
                                    <Button
                                        type="button"
                                        size="sm"
                                        variant="outline"
                                        render={
                                            <Link
                                                href={`/sales/orders/${detail.identity.salesOrderId}?section=collaboration`}
                                            />
                                        }
                                    >
                                        打开销售单协同
                                    </Button>
                                    {detail.allowedActions.includes("RETRY") ? (
                                        <Button
                                            type="button"
                                            size="sm"
                                            variant="outline"
                                            disabled={commandMutation.isPending}
                                            onClick={() => {
                                                const row = rows.find(
                                                    (r) =>
                                                        r.projectionId ===
                                                        detail.identity
                                                            .projectionId,
                                                )
                                                if (!row) return
                                                setPendingAction({
                                                    kind: "RETRY",
                                                    row,
                                                    objectVersion:
                                                        detail.objectVersion,
                                                })
                                            }}
                                        >
                                            重试发送
                                        </Button>
                                    ) : null}
                                    {detail.allowedActions.includes(
                                        "ESCALATE",
                                    ) ||
                                    detail.reconciliationStatus ===
                                        "VERSION_MISMATCH" ? (
                                        detail.allowedActions.includes(
                                            "ESCALATE",
                                        ) &&
                                        !detail.deliveries[0]?.workItemId ? (
                                            <Button
                                                type="button"
                                                size="sm"
                                                variant="outline"
                                                disabled={
                                                    commandMutation.isPending
                                                }
                                                onClick={() => {
                                                    const row = rows.find(
                                                        (r) =>
                                                            r.projectionId ===
                                                            detail.identity
                                                                .projectionId,
                                                    )
                                                    if (!row) return
                                                    setPendingAction({
                                                        kind: "ESCALATE",
                                                        row,
                                                        objectVersion:
                                                            detail.objectVersion,
                                                    })
                                                }}
                                            >
                                                升级到接口错误中心
                                            </Button>
                                        ) : (
                                            <Button
                                                type="button"
                                                size="sm"
                                                variant="outline"
                                                render={
                                                    <Link
                                                        href={w29Href(
                                                            detail.deliveries[0]
                                                                ?.workItemId,
                                                            detail.deliveries[0]
                                                                ?.errorTaskId,
                                                        )}
                                                    />
                                                }
                                            >
                                                去接口错误中心处理
                                            </Button>
                                        )
                                    ) : null}
                                </>
                            }
                        />

                        <Alert>
                            <TriangleAlertIcon aria-hidden="true" />
                            <AlertTitle>只读提示</AlertTitle>
                            <AlertDescription>
                                {detail.boundaryNotice}
                            </AlertDescription>
                        </Alert>

                        <StatusTrackSummary
                            aria-label="详情三轨状态"
                            variant="table"
                            tracks={[
                                {
                                    id: "sales-fact",
                                    label: "销售记录",
                                    status: {
                                        label: detail.tracks.salesFact.label,
                                        tone: detail.tracks.salesFact.tone,
                                        description:
                                            detail.tracks.salesFact.description,
                                    },
                                },
                                {
                                    id: "projection-delivery",
                                    label: "信息发送",
                                    status: {
                                        label: detail.tracks.projectionDelivery
                                            .label,
                                        tone: detail.tracks.projectionDelivery
                                            .tone,
                                        description:
                                            detail.tracks.projectionDelivery
                                                .description,
                                    },
                                },
                                {
                                    id: "mall-confirm",
                                    label: "商城确认",
                                    status: {
                                        label: detail.tracks.mallConfirm.label,
                                        tone: detail.tracks.mallConfirm.tone,
                                        description:
                                            detail.tracks.mallConfirm
                                                .description,
                                    },
                                },
                            ]}
                        />

                        <Tabs value={objectTab} onValueChange={setObjectTab}>
                            <TabsList>
                                <TabsTrigger value="overview">概览</TabsTrigger>
                                <TabsTrigger value="content">
                                    执行内容
                                </TabsTrigger>
                                <TabsTrigger value="history">
                                    发送历史
                                </TabsTrigger>
                                <TabsTrigger value="versions">
                                    版本对应
                                </TabsTrigger>
                                <TabsTrigger value="diff">
                                    差异与错误
                                </TabsTrigger>
                            </TabsList>
                        </Tabs>

                        {objectTab === "overview" ? (
                            <DocumentSummary
                                columns="two"
                                items={[
                                    {
                                        id: "source-ver",
                                        label: "来源销售版本",
                                        value: `v${detail.selectedRevision.salesOrderRevisionNo}`,
                                        numeric: true,
                                    },
                                    {
                                        id: "proj-ver",
                                        label: versionText.dataVersion,
                                        value: `v${detail.selectedRevision.revisionNo}`,
                                        numeric: true,
                                    },
                                    {
                                        id: "source",
                                        label: "数据来源",
                                        value: SOURCE_LABEL[
                                            detail.selectedRevision
                                                .projectionSource
                                        ],
                                    },
                                    {
                                        id: "acked",
                                        label: "商城已确认版",
                                        value:
                                            detail.currentAckedRevisionNo !=
                                            null
                                                ? `v${detail.currentAckedRevisionNo}`
                                                : "尚未确认",
                                        numeric: true,
                                    },
                                    {
                                        id: "latency",
                                        label: "等待时长",
                                        value: `${detail.pendingDurationLabel} · ${LATENCY_LABEL[detail.latencyBand]}`,
                                    },
                                    {
                                        id: "owner",
                                        label: "责任",
                                        value: detail.ownerLabel,
                                    },
                                ]}
                            />
                        ) : null}

                        {objectTab === "content" ? (
                            <DocumentSection
                                title="执行内容"
                                description="字段以系统数据修订为准。不含成交金额、配赠、税率、开票、应收、玩法规则。"
                            >
                                <WhitelistContentGrid
                                    content={detail.selectedRevision.content}
                                    revisionNo={
                                        detail.selectedRevision.revisionNo
                                    }
                                />
                            </DocumentSection>
                        ) : null}

                        {objectTab === "history" ? (
                            <DocumentSection title="发送历史">
                                <div className="overflow-x-auto rounded-xl border">
                                    <table className="w-full text-sm">
                                        <thead className="bg-muted/50 text-left text-xs text-muted-foreground">
                                            <tr>
                                                <th className="px-3 py-2">
                                                    状态
                                                </th>
                                                <th className="px-3 py-2">
                                                    尝试
                                                </th>
                                                <th className="px-3 py-2">
                                                    最近
                                                </th>
                                                <th className="px-3 py-2">
                                                    下次
                                                </th>
                                                <th className="px-3 py-2">
                                                    确认
                                                </th>
                                                <th className="px-3 py-2">
                                                    摘要
                                                </th>
                                            </tr>
                                        </thead>
                                        <tbody>
                                            {detail.deliveries.map((d) => (
                                                <tr
                                                    key={d.deliveryId}
                                                    className="border-t"
                                                >
                                                    <td className="px-3 py-2">
                                                        <BusinessStatusBadge
                                                            context="list"
                                                            label={
                                                                d.statusLabel
                                                            }
                                                            tone={d.statusTone}
                                                        />
                                                    </td>
                                                    <td className="num px-3 py-2">
                                                        {d.attemptCount}
                                                    </td>
                                                    <td className="num px-3 py-2">
                                                        {d.lastAttemptAt ?? "—"}
                                                    </td>
                                                    <td className="num px-3 py-2">
                                                        {d.nextAttemptAt ?? "—"}
                                                    </td>
                                                    <td className="num px-3 py-2">
                                                        {d.mallAckAt ?? "—"}
                                                    </td>
                                                    <td className="px-3 py-2 text-xs text-muted-foreground">
                                                        {d.errorSummary ??
                                                            d.mallExecutionBaseline ??
                                                            "—"}
                                                    </td>
                                                </tr>
                                            ))}
                                        </tbody>
                                    </table>
                                </div>
                            </DocumentSection>
                        ) : null}

                        {objectTab === "versions" ? (
                            <DocumentSection
                                title="版本对应"
                                description="历史数据固定显示来源销售版本，不被销售单当前版覆盖。"
                            >
                                <RevisionTimeline
                                    revisions={detail.revisionLinks.map(
                                        (link) => ({
                                            id: link.projectionRevisionId,
                                            version: link.projectionRevisionNo,
                                            source:
                                                detail.selectedRevision
                                                    .projectionSource ===
                                                "MIGRATION_BASELINE"
                                                    ? ("migration-baseline" as const)
                                                    : ("erp-change" as const),
                                            actor: "系统",
                                            effectiveAt: {
                                                dateTime:
                                                    link.mallAckAt ??
                                                    "2026-08-01T00:00:00+08:00",
                                                label: link.mallAckAt
                                                    ? `确认 ${link.mallAckAt}`
                                                    : "尚未确认",
                                            },
                                            isCurrent: link.isCurrentSelection,
                                            status: {
                                                label: link.deliveryStatusLabel,
                                                tone:
                                                    link.deliveryStatus ===
                                                    "ACKED"
                                                        ? ("success" as const)
                                                        : link.deliveryStatus ===
                                                            "FAILED"
                                                          ? ("destructive" as const)
                                                          : ("neutral" as const),
                                            },
                                            reason: (
                                                <span>
                                                    来源销售版本 v
                                                    {link.sourceSalesRevisionNo}
                                                    {link.isCurrentSelection
                                                        ? " · 当前查看"
                                                        : ""}
                                                </span>
                                            ),
                                            action: (
                                                <Button
                                                    type="button"
                                                    size="xs"
                                                    variant="outline"
                                                    onClick={() =>
                                                        replaceParams({
                                                            projectionId:
                                                                detail.identity
                                                                    .projectionId,
                                                            revision:
                                                                link.projectionRevisionId,
                                                        })
                                                    }
                                                >
                                                    查看此修订
                                                </Button>
                                            ),
                                        }),
                                    )}
                                />
                            </DocumentSection>
                        ) : null}

                        {objectTab === "diff" ? (
                            <DocumentSection title="差异与错误">
                                {detail.reconciliationStatus ===
                                "VERSION_MISMATCH" ? (
                                    <Alert variant="warning" className="mb-3">
                                        <TriangleAlertIcon aria-hidden="true" />
                                        <AlertTitle>版本对账差异</AlertTitle>
                                        <AlertDescription>
                                            {
                                                RECONCILIATION_LABEL.VERSION_MISMATCH
                                            }
                                            。请前往接口错误中心核对；本页不提供覆盖任一侧记录。
                                            <div className="mt-2">
                                                <Button
                                                    type="button"
                                                    size="sm"
                                                    variant="outline"
                                                    render={
                                                        <Link
                                                            href={w29Href(
                                                                detail
                                                                    .deliveries[0]
                                                                    ?.workItemId,
                                                                detail
                                                                    .deliveries[0]
                                                                    ?.errorTaskId,
                                                            )}
                                                        />
                                                    }
                                                >
                                                    打开接口错误差异任务
                                                </Button>
                                            </div>
                                        </AlertDescription>
                                    </Alert>
                                ) : (
                                    <p className="text-sm text-muted-foreground">
                                        当前无版本对账差异。
                                    </p>
                                )}
                                {detail.deliveries[0]?.errorSummary ? (
                                    <div className="rounded-xl border p-3 text-sm">
                                        <div className="font-medium">
                                            失败摘要
                                        </div>
                                        <p className="mt-1 text-muted-foreground">
                                            {detail.deliveries[0].errorCode
                                                ? `${detail.deliveries[0].errorCode} · `
                                                : ""}
                                            {detail.deliveries[0].errorSummary}
                                        </p>
                                        {detail.deliveries[0].workItemId ? (
                                            <div className="mt-2 flex flex-wrap items-center gap-2">
                                                <Badge variant="secondary">
                                                    关联错误任务
                                                </Badge>
                                                <Button
                                                    type="button"
                                                    size="xs"
                                                    variant="outline"
                                                    render={
                                                        <Link
                                                            href={w29Href(
                                                                detail
                                                                    .deliveries[0]
                                                                    .workItemId,
                                                                detail
                                                                    .deliveries[0]
                                                                    .errorTaskId,
                                                            )}
                                                        />
                                                    }
                                                >
                                                    <ExternalLinkIcon
                                                        data-icon="inline-start"
                                                        aria-hidden="true"
                                                    />
                                                    在接口错误中心处理
                                                </Button>
                                            </div>
                                        ) : null}
                                    </div>
                                ) : null}
                                <p className="mt-3 text-xs text-muted-foreground">
                                    本页不建立处理责任，也不支持转交或完成处理任务。
                                </p>
                            </DocumentSection>
                        ) : null}
                    </div>
                )}
            </QuickPreviewSheet>

            <FormalActionConfirmDialog
                open={pendingAction != null}
                onOpenChange={(open) => {
                    if (!open) setPendingAction(null)
                }}
                title={
                    pendingAction?.kind === "QUERY_RESULT"
                        ? "查询最终结果"
                        : pendingAction?.kind === "RETRY"
                          ? "重试发送"
                          : pendingAction?.kind === "ESCALATE"
                            ? "升级到接口错误中心"
                            : pendingAction?.kind === "BULK_QUERY"
                              ? "批量查询"
                              : pendingAction?.kind === "BULK_RETRY"
                                ? "批量重试"
                                : "确认操作"
                }
                actionLabel="执行"
                confirmLabel="确认执行"
                cancelLabel="取消"
                fromStatus={
                    pendingAction && "row" in pendingAction && pendingAction.row
                        ? {
                              label: pendingAction.row.delivery.statusLabel,
                              tone: pendingAction.row.delivery.statusTone,
                          }
                        : { label: "当前选择", tone: "neutral" }
                }
                toStatus={
                    pendingAction?.kind === "QUERY_RESULT"
                        ? { label: "明确结果或仍未知", tone: "warning" }
                        : pendingAction?.kind === "RETRY" ||
                            pendingAction?.kind === "BULK_RETRY"
                          ? { label: "按原记录重试", tone: "info" }
                          : pendingAction?.kind === "ESCALATE"
                            ? { label: "错误中心待办", tone: "warning" }
                            : { label: "后台逐项处理", tone: "info" }
                }
                lockedFields={
                    pendingAction && "row" in pendingAction && pendingAction.row
                        ? [
                              `销售版本 v${pendingAction.row.salesOrderRevisionNo}`,
                              `数据版本 v${pendingAction.row.projectionRevisionNo}`,
                              pendingAction.row.targetMallName,
                              `销售单 ${pendingAction.row.salesOrderNo} · v${pendingAction.row.salesOrderRevisionNo} · ${pendingAction.row.targetMallName}`,
                          ]
                        : pendingAction && "ids" in pendingAction
                          ? [
                                `显式选择 ${pendingAction.ids.length} 项`,
                                "系统筛选结果（非当前筛选全部）",
                            ]
                          : []
                }
                effects={
                    pendingAction?.kind === "QUERY_RESULT"
                        ? [
                              "未明确前不显示成功",
                              "不跳过、不计入已确认指标",
                              "超时可再次查询或升级到接口错误中心",
                          ]
                        : pendingAction?.kind === "RETRY"
                          ? [
                                "沿原数据修订继续发送",
                                "不生成新数据修订",
                                "不回退销售记录或应收",
                            ]
                          : pendingAction?.kind === "ESCALATE"
                            ? [
                                  "创建或复用接口错误待办（不会重复建单）",
                                  "本页只返回入口，不建立处理责任或完成任务",
                              ]
                            : pendingAction?.kind === "BULK_RETRY"
                              ? [
                                    "系统按筛选结果逐项核对",
                                    "已确认/结果未知/权限变化项跳过",
                                    "展示成功/跳过/失败/仍未知",
                                ]
                              : ["系统按筛选结果逐项查询", "仍未知不按成功处理"]
                }
                nextDepartment={
                    pendingAction?.kind === "ESCALATE"
                        ? "接口错误中心"
                        : "运营 / 系统"
                }
                pending={commandMutation.isPending || bulkMutation.isPending}
                onConfirm={async () => {
                    const action = pendingAction
                    if (!action) return
                    if (
                        action.kind === "BULK_QUERY" ||
                        action.kind === "BULK_RETRY"
                    ) {
                        await runBulk(action.kind)
                        return
                    }
                    await openConfirmForRow(
                        action.kind,
                        action.row,
                        action.objectVersion,
                    )
                }}
            />
        </PageScaffold>
    )
}
