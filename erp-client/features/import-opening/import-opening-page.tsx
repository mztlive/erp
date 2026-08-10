"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import type { ColumnDef, PaginationState } from "@tanstack/react-table"
import {
    ArrowLeftIcon,
    ExternalLinkIcon,
    SearchIcon,
    ShieldAlertIcon,
    TriangleAlertIcon,
} from "lucide-react"

import {
    BackgroundJobProgress,
    BatchOperationResult,
    BusinessEmptyState,
    BusinessFailureState,
    BusinessStatusBadge,
    BusinessTableFrame,
    DataFreshness,
    DataTable,
    DocumentHeader,
    FormalActionResult,
    ImportIssueTable,
    ImportStageIndicator,
    ListToolbar,
    MetricItem,
    MetricStrip,
    OptionCombobox,
    PageHeader,
    PageScaffold,
    surfacePanelClassName,
    type ImportStageStates,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { Label } from "@/components/ui/label"
import { Separator } from "@/components/ui/separator"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { formatObjectSet } from "@/features/import-opening/api"
import { formatDateTime } from "@/lib/datetime"
import {
    useImportBatchDetailQuery,
    useImportBatchListQuery,
    useImportIssuesQuery,
} from "@/features/import-opening/queries"
import type {
    BatchSection,
    ImportBatchListItem,
    ImportBatchStatus,
    ImportBatchView,
    ImportEnvironment,
    ImportIssueCode,
    ImportObjectCode,
    ImportPipelineStage,
    IssueRowStatus,
} from "@/features/import-opening/types"
import {
    BATCH_STATUS_LABEL,
    BATCH_STATUS_TONE,
    CONFIRMATION_SCOPE_LABEL,
    ENVIRONMENT_LABEL,
    ISSUE_CODE_LABEL,
    OBJECT_CODE_LABEL,
    PIPELINE_STAGE_LABEL,
    PIPELINE_TO_INDICATOR,
    RETENTION_LABEL,
    ROW_STATUS_LABEL,
    WORK_ITEM_TYPE_BLOCKER,
} from "@/features/import-opening/types"
import {
    buildImportOpeningSearchParams,
    parseImportOpeningSearchParams,
    type ImportOpeningUrlState,
} from "@/features/import-opening/url-state"
import { versionText } from "@/lib/ui-text"

const SECTION_TABS: { id: BatchSection; label: string }[] = [
    { id: "overview", label: "概览" },
    { id: "files", label: "文件与规则" },
    { id: "trial", label: "试算与问题" },
    { id: "confirm", label: "责任确认" },
    { id: "progress", label: "执行进度" },
    { id: "result", label: "结果" },
    { id: "audit", label: "审计" },
]

const PIPELINE_ORDER: ImportPipelineStage[] = [
    "RECEIVE",
    "VALIDATE",
    "TRIAL",
    "CONFIRM",
    "APPLY",
    "RESULT",
]

function buildStageStates(current: ImportPipelineStage): ImportStageStates {
    const currentIdx = PIPELINE_ORDER.indexOf(current)
    const states: {
        [K in import("@/components/business").ImportStageKey]: {
            status: "pending" | "current" | "complete" | "failed"
            description?: string
        }
    } = {
        upload: { status: "pending" },
        mapping: { status: "pending" },
        validation: { status: "pending" },
        preview: { status: "pending" },
        submission: { status: "pending" },
        result: { status: "pending" },
    }
    for (let i = 0; i < PIPELINE_ORDER.length; i += 1) {
        const stage = PIPELINE_ORDER[i]!
        const key = PIPELINE_TO_INDICATOR[stage]
        let status: "pending" | "current" | "complete" | "failed" = "pending"
        if (i < currentIdx) status = "complete"
        else if (i === currentIdx) status = "current"
        states[key] = {
            status,
            description: PIPELINE_STAGE_LABEL[stage],
        }
    }
    return states
}

function formatBytes(n: number) {
    if (n < 1024) return `${n} B`
    if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`
    return `${(n / (1024 * 1024)).toFixed(2)} MB`
}

export function ImportOpeningPage() {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()

    const urlState = React.useMemo(
        () => parseImportOpeningSearchParams(searchParams),
        [searchParams],
    )

    const replaceUrl = React.useCallback(
        (next: ImportOpeningUrlState) => {
            const qs = buildImportOpeningSearchParams(next)
            router.replace(`${pathname}${qs}`, { scroll: false })
        },
        [pathname, router],
    )

    const patchUrl = React.useCallback(
        (patch: Partial<ImportOpeningUrlState>) => {
            replaceUrl({ ...urlState, ...patch })
        },
        [replaceUrl, urlState],
    )

    if (urlState.batchId) {
        return (
            <BatchDetailView
                batchId={urlState.batchId}
                urlState={urlState}
                patchUrl={patchUrl}
                replaceUrl={replaceUrl}
            />
        )
    }

    return <BatchListView urlState={urlState} patchUrl={patchUrl} />
}

function BatchListView({
    urlState,
    patchUrl,
}: {
    urlState: ImportOpeningUrlState
    patchUrl: (patch: Partial<ImportOpeningUrlState>) => void
}) {
    const [qDraft, setQDraft] = React.useState(urlState.q ?? "")
    const searchInputRef = React.useRef<HTMLInputElement | null>(null)
    const listQuery = useImportBatchListQuery({
        environment: urlState.environment,
        status: urlState.status,
        objectType: urlState.objectType ?? "all",
        q: urlState.q,
        page: urlState.page,
        pageSize: 20,
    })

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

    const columns = React.useMemo<ColumnDef<ImportBatchListItem>[]>(
        () => [
            {
                id: "batchNo",
                header: "批次号",
                cell: ({ row }) => (
                    <Button
                        variant="link"
                        className="h-auto p-0 font-mono text-sm"
                        onClick={() =>
                            patchUrl({
                                batchId: row.original.batchId,
                                section: "overview",
                                page: 1,
                            })
                        }
                    >
                        {row.original.batchNo}
                    </Button>
                ),
            },
            {
                id: "environment",
                header: "环境",
                cell: ({ row }) => (
                    <Badge
                        variant={
                            row.original.environment === "PRODUCTION"
                                ? "destructive"
                                : "secondary"
                        }
                    >
                        {ENVIRONMENT_LABEL[row.original.environment]}
                    </Badge>
                ),
            },
            {
                id: "objects",
                header: "对象集合",
                cell: ({ row }) => (
                    <span className="text-sm">
                        {formatObjectSet(row.original.sourceObjectSet)}
                    </span>
                ),
            },
            {
                id: "baseline",
                header: "基准日",
                cell: ({ row }) => (
                    <span className="num text-sm">
                        {row.original.baselineDate}
                    </span>
                ),
            },
            {
                id: "stage",
                header: "阶段",
                cell: ({ row }) => (
                    <span className="text-sm">
                        {PIPELINE_STAGE_LABEL[row.original.stage]}
                    </span>
                ),
            },
            {
                id: "rule",
                header: "规则版本",
                cell: ({ row }) => (
                    <span className="num font-mono text-xs">
                        {row.original.importRuleVersion}
                    </span>
                ),
            },
            {
                id: "progress",
                header: "进度",
                cell: ({ row }) => (
                    <span className="text-sm">
                        {row.original.progressLabel}
                    </span>
                ),
            },
            {
                id: "confirm",
                header: "责任确认",
                cell: ({ row }) => (
                    <span className="text-sm">
                        {row.original.confirmationSummary}
                    </span>
                ),
            },
            {
                id: "status",
                header: "状态",
                cell: ({ row }) => (
                    <BusinessStatusBadge
                        context="list"
                        label={BATCH_STATUS_LABEL[row.original.status]}
                        tone={BATCH_STATUS_TONE[row.original.status]}
                    />
                ),
            },
            {
                id: "updated",
                header: "更新时间",
                cell: ({ row }) => (
                    <span className="num text-xs text-muted-foreground">
                        {formatDateTime(
                            row.original.updatedAt,
                            "dateStyle",
                            "passthrough",
                        )}
                    </span>
                ),
            },
        ],
        [patchUrl],
    )

    const data = listQuery.data
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
        urlState.q || urlState.status || urlState.objectType,
    )

    const clearListFilters = () => {
        setQDraft("")
        patchUrl({
            q: undefined,
            status: undefined,
            objectType: undefined,
            page: 1,
        })
    }

    return (
        <PageScaffold>
            <PageHeader
                title="导入与期初"
                breadcrumbs={[
                    {
                        id: "gov",
                        label: "治理",
                        href: "/governance/imports",
                        current: false,
                    },
                    { id: "imp", label: "导入与期初", current: true },
                ]}
                metadata={
                    <DataFreshness
                        updatedAt={
                            data?.queriedAt
                                ? formatDateTime(
                                      data.queriedAt,
                                      "dateStyle",
                                      "passthrough",
                                  )
                                : "刚刚"
                        }
                        dateTime={data?.queriedAt}
                        state={listQuery.isFetching ? "stale" : "fresh"}
                        label="导入批次"
                    />
                }
            />

            <div className="flex flex-wrap items-center gap-3">
                <Label className="text-sm text-muted-foreground">环境</Label>
                <Tabs
                    value={urlState.environment}
                    onValueChange={(v) => {
                        if (v == null) return
                        patchUrl({
                            environment: v as ImportEnvironment,
                            page: 1,
                            batchId: undefined,
                        })
                    }}
                >
                    <TabsList>
                        <TabsTrigger value="VALIDATION">验证环境</TabsTrigger>
                        <TabsTrigger value="PRODUCTION">生产环境</TabsTrigger>
                    </TabsList>
                </Tabs>
                {urlState.environment === "PRODUCTION" ? (
                    <Badge variant="destructive">
                        生产环境 · 操作需显著确认
                    </Badge>
                ) : (
                    <Badge variant="secondary">验证环境</Badge>
                )}
            </div>

            <MetricStrip columns={4} aria-label="导入批次指标">
                <MetricItem
                    label="待校验"
                    value={data?.metrics.pendingValidate ?? "—"}
                />
                <MetricItem
                    label="待业务确认"
                    value={data?.metrics.pendingConfirm ?? "—"}
                />
                <MetricItem
                    label="执行中"
                    value={data?.metrics.applying ?? "—"}
                />
                <MetricItem
                    label="失败/部分失败"
                    value={data?.metrics.failedOrPartial ?? "—"}
                />
            </MetricStrip>

            <Alert>
                <ShieldAlertIcon />
                <AlertTitle>安全边界</AlertTitle>
                <AlertDescription>
                    本页不展示原始
                    SQL、数据库连接头、禁止字段或存储对象键。不合规导出只能在受控临时区清洗后，以白名单合规包进入安全接收。
                </AlertDescription>
            </Alert>

            <BusinessTableFrame
                title="导入批次"
                description={
                    listQuery.isError
                        ? "列表加载失败，可调整筛选后重试"
                        : `${ENVIRONMENT_LABEL[urlState.environment]} · 共 ${data?.totalCount ?? 0} 批`
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
                                        placeholder="批次号（精确/前缀匹配）"
                                        aria-label="搜索批次"
                                    />
                                </InputGroup>
                            </form>
                        }
                        filters={
                            <>
                                <OptionCombobox
                                    value={urlState.objectType ?? "all"}
                                    onValueChange={(v) => {
                                        if (v == null) return
                                        patchUrl({
                                            objectType:
                                                v === "all"
                                                    ? undefined
                                                    : (v as ImportObjectCode),
                                            page: 1,
                                        })
                                    }}
                                    options={[
                                        { value: "all", label: "全部对象" },
                                        ...(
                                            Object.keys(
                                                OBJECT_CODE_LABEL,
                                            ) as ImportObjectCode[]
                                        ).map((code) => ({
                                            value: code,
                                            label: OBJECT_CODE_LABEL[code],
                                        })),
                                    ]}
                                    inputClassName="w-[10rem]"
                                    size="sm"
                                    placeholder="对象：全部"
                                    aria-label="对象集合"
                                    allowClear={false}
                                />
                                <OptionCombobox
                                    value={urlState.status ?? "all"}
                                    onValueChange={(v) => {
                                        if (v == null) return
                                        patchUrl({
                                            status: v === "all" ? undefined : v,
                                            page: 1,
                                        })
                                    }}
                                    options={[
                                        { value: "all", label: "全部状态" },
                                        ...(
                                            Object.keys(
                                                BATCH_STATUS_LABEL,
                                            ) as ImportBatchStatus[]
                                        ).map((s) => ({
                                            value: s,
                                            label: BATCH_STATUS_LABEL[s],
                                        })),
                                    ]}
                                    inputClassName="w-[11rem]"
                                    size="sm"
                                    placeholder="状态：全部"
                                    aria-label="批次状态"
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
                                    批
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
                            title="批次列表加载失败"
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
                            getRowId={(row) => row.batchId}
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
        </PageScaffold>
    )
}

function BatchDetailView({
    batchId,
    urlState,
    patchUrl,
    replaceUrl,
}: {
    batchId: string
    urlState: ImportOpeningUrlState
    patchUrl: (patch: Partial<ImportOpeningUrlState>) => void
    replaceUrl: (next: ImportOpeningUrlState) => void
}) {
    const detailQuery = useImportBatchDetailQuery(batchId)
    const issueQuery = useImportIssuesQuery(
        {
            batchId,
            issueCode: urlState.issueCode ?? "all",
            objectType: urlState.issueObjectType ?? "all",
            rowStatus: urlState.rowStatus ?? "all",
            page: Math.max(1, urlState.page),
            pageSize: 20,
        },
        Boolean(batchId),
    )

    const batch = detailQuery.data
    const section = urlState.section

    if (detailQuery.isPending) {
        return (
            <PageScaffold>
                <div className="h-10 w-48 animate-pulse rounded-lg bg-muted" />
                <div className="h-24 animate-pulse rounded-lg bg-muted" />
                <div className="h-40 animate-pulse rounded-lg bg-muted" />
            </PageScaffold>
        )
    }

    if (detailQuery.isError) {
        return (
            <PageScaffold>
                <BusinessFailureState
                    title="批次加载失败"
                    error={detailQuery.error}
                    onRetry={() => void detailQuery.refetch()}
                />
            </PageScaffold>
        )
    }

    if (!batch) {
        return (
            <PageScaffold>
                <BusinessEmptyState
                    kind="no-data"
                    title="批次不存在"
                    description="请返回列表或检查批次身份。"
                    className="rounded-lg border-0 bg-transparent shadow-none ring-0"
                    action={
                        <Button
                            type="button"
                            variant="secondary"
                            className="rounded-lg shadow-none"
                            onClick={() =>
                                replaceUrl({
                                    ...urlState,
                                    batchId: undefined,
                                    section: "overview",
                                })
                            }
                        >
                            返回列表
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }

    const stageStates = buildStageStates(batch.stage)
    const importStageLabels = {
        upload: PIPELINE_STAGE_LABEL.RECEIVE,
        mapping: PIPELINE_STAGE_LABEL.VALIDATE,
        validation: PIPELINE_STAGE_LABEL.TRIAL,
        preview: PIPELINE_STAGE_LABEL.CONFIRM,
        submission: PIPELINE_STAGE_LABEL.APPLY,
        result: PIPELINE_STAGE_LABEL.RESULT,
    }
    const confirmBlocked = batch.actionBlockers.filter(
        (b) => b.action === "CONFIRM_SCOPE",
    )
    const applyBlocked = batch.actionBlockers.filter(
        (b) => b.action === "START_APPLY",
    )
    const workItemTypeMissing = !batch.productionGates.workItemTypeRegistered

    return (
        <PageScaffold>
            <PageHeader
                variant="object-chrome"
                breadcrumbs={[
                    { id: "gov", label: "治理", href: "/governance/imports" },
                    {
                        id: "imp",
                        label: "导入与期初",
                        href: "/governance/imports",
                    },
                    {
                        id: "batch",
                        label: batch.batchNo,
                        current: true,
                    },
                ]}
                actions={
                    <Button
                        type="button"
                        variant="ghost"
                        size="sm"
                        onClick={() =>
                            replaceUrl({
                                ...urlState,
                                batchId: undefined,
                                section: "overview",
                                issueCode: undefined,
                                issueObjectType: undefined,
                                rowStatus: undefined,
                            })
                        }
                    >
                        <ArrowLeftIcon className="size-4" />
                        返回批次列表
                    </Button>
                }
            />

            <DocumentHeader
                density="compact"
                title={batch.sourceSystem.name}
                documentNumber={batch.batchNo}
                primaryStatus={{
                    label: BATCH_STATUS_LABEL[batch.status],
                    tone: BATCH_STATUS_TONE[batch.status],
                }}
                version={batch.version}
                meta={
                    <span className="text-muted-foreground">
                        {ENVIRONMENT_LABEL[batch.environment]} · 基准日{" "}
                        {batch.baselineDate}
                    </span>
                }
                statuses={[
                    {
                        id: "env",
                        label: "环境",
                        status: {
                            label: ENVIRONMENT_LABEL[batch.environment],
                            tone:
                                batch.environment === "PRODUCTION"
                                    ? "destructive"
                                    : "info",
                        },
                    },
                    {
                        id: "baseline",
                        label: "基准日",
                        status: {
                            label: batch.baselineDate,
                            tone: "neutral",
                        },
                    },
                    {
                        id: "rule",
                        label: "规则版本",
                        status: {
                            label: batch.importRuleVersion,
                            tone: "neutral",
                        },
                    },
                    {
                        id: "stage",
                        label: "当前阶段",
                        status: {
                            label: PIPELINE_STAGE_LABEL[batch.stage],
                            tone: "info",
                        },
                    },
                ]}
            />

            {/* 批次身份摘要：对象集、试算版本、来源系统等（环境/基准日/规则版本见页头） */}
            <Card size="sm" className={surfacePanelClassName}>
                <CardContent className="grid gap-3 pt-4 sm:grid-cols-2 lg:grid-cols-4">
                    <Fact
                        label="对象集合"
                        value={formatObjectSet(batch.sourceObjectSet)}
                    />
                    <Fact label="试算版本" value={batch.trialVersion} mono />
                    <Fact label="来源系统" value={batch.sourceSystem.name} />
                    <Fact label="发起人" value={batch.initiatorLabel} />
                    <Fact
                        label="更新时间"
                        value={formatDateTime(
                            batch.updatedAt,
                            "dateStyle",
                            "passthrough",
                        )}
                    />
                </CardContent>
            </Card>

            {!batch.formalDataFormed ? (
                <Alert variant="warning">
                    <TriangleAlertIcon />
                    <AlertTitle>尚未形成业务数据</AlertTitle>
                    <AlertDescription>
                        {batch.notFormalDataMessage}
                    </AlertDescription>
                </Alert>
            ) : (
                <Alert variant="success">
                    <AlertTitle>已形成业务对象（部分或全部）</AlertTitle>
                    <AlertDescription>
                        {batch.notFormalDataMessage}
                    </AlertDescription>
                </Alert>
            )}

            <ImportStageIndicator
                stages={stageStates}
                stageLabels={importStageLabels}
                aria-label="导入六段流水线"
            />

            <Tabs
                value={section}
                onValueChange={(v) => {
                    if (v == null) return
                    patchUrl({ section: v as BatchSection })
                }}
            >
                <TabsList className="flex h-auto flex-wrap">
                    {SECTION_TABS.map((tab) => (
                        <TabsTrigger key={tab.id} value={tab.id}>
                            {tab.label}
                        </TabsTrigger>
                    ))}
                </TabsList>
            </Tabs>

            {section === "overview" ? (
                <OverviewSection
                    batch={batch}
                    onGoSection={(s) => patchUrl({ section: s })}
                />
            ) : null}

            {section === "files" ? <FilesSection batch={batch} /> : null}

            {section === "trial" ? (
                <TrialSection
                    batch={batch}
                    urlState={urlState}
                    patchUrl={patchUrl}
                    issueQuery={issueQuery}
                />
            ) : null}

            {section === "confirm" ? (
                <ConfirmSection
                    batch={batch}
                    workItemTypeMissing={workItemTypeMissing}
                    confirmBlocked={confirmBlocked}
                />
            ) : null}

            {section === "progress" ? <ProgressSection batch={batch} /> : null}

            {section === "result" ? (
                <ResultSection
                    batch={batch}
                    onOpenRepair={(id) =>
                        patchUrl({ batchId: id, section: "progress", page: 1 })
                    }
                />
            ) : null}

            {section === "audit" ? <AuditSection batch={batch} /> : null}

            {/* 生产应用门禁：仅提交应用前阶段展示 */}
            {batch.stage !== "RESULT" && batch.stage !== "APPLY" ? (
                <Card size="sm" className={surfacePanelClassName}>
                    <CardHeader className="border-b border-border/30">
                        <CardTitle>提交前检查</CardTitle>
                        <CardDescription>
                            验证环境校验与业务确认是生产应用前置条件；系统管理员不能代替确认。
                        </CardDescription>
                    </CardHeader>
                    <CardContent className="space-y-3 pt-4">
                        <GateRow
                            ok={batch.productionGates.validationEnvPassed}
                            label="验证环境校验与确认已通过并关联"
                        />
                        <GateRow
                            ok={batch.productionGates.allConfirmationsComplete}
                            label="全部必要责任确认完成"
                        />
                        <GateRow
                            ok={batch.productionGates.noBlockingIssues}
                            label="无阻塞校验问题"
                        />
                        <GateRow
                            ok={batch.productionGates.trialVersionMatches}
                            label="试算版本与确认一致（未因规则变化失效）"
                        />
                        <GateRow
                            ok={batch.productionGates.ruleVersionStable}
                            label="规则版本稳定"
                        />
                        <GateRow
                            ok={batch.productionGates.workItemTypeRegistered}
                            label="导入确认任务类型已登记"
                        />
                        {applyBlocked.length > 0 ? (
                            <FormalActionResult
                                status="blocked"
                                title="提交生产应用已阻断"
                                description={applyBlocked
                                    .map((b) => b.message)
                                    .join(" ")}
                                facts={applyBlocked.map((b) => ({
                                    label: b.code,
                                    value: b.message,
                                }))}
                            />
                        ) : (
                            <FormalActionResult
                                status="succeeded"
                                title={
                                    batch.stage === "CONFIRM"
                                        ? "检查已完成，可提交应用"
                                        : "检查已完成"
                                }
                                description="提交时系统会再次核验权限与数据，确认无误后开始导入。"
                            />
                        )}
                    </CardContent>
                </Card>
            ) : null}
        </PageScaffold>
    )
}

function Fact({
    label,
    value,
    mono,
}: {
    label: string
    value: React.ReactNode
    mono?: boolean
}) {
    return (
        <div className="space-y-0.5">
            <div className="text-xs text-muted-foreground">{label}</div>
            <div className={mono ? "num font-mono text-sm" : "text-sm"}>
                {value}
            </div>
        </div>
    )
}

function GateRow({ ok, label }: { ok: boolean; label: string }) {
    return (
        <div className="flex items-start gap-2 text-sm">
            <Badge variant={ok ? "secondary" : "destructive"}>
                {ok ? "已满足" : "未满足"}
            </Badge>
            <span className={ok ? "text-foreground" : "text-muted-foreground"}>
                {label}
            </span>
        </div>
    )
}

function OverviewSection({
    batch,
    onGoSection,
}: {
    batch: ImportBatchView
    onGoSection: (s: BatchSection) => void
}) {
    return (
        <div className="grid gap-4 lg:grid-cols-[minmax(0,1.4fr)_minmax(0,1fr)]">
            <Card size="sm" className={surfacePanelClassName}>
                <CardHeader className="border-b border-border/30">
                    <CardTitle>试算摘要</CardTitle>
                    <CardDescription>
                        试算统计由系统统一计算，与问题明细可能因筛选存在差异。
                    </CardDescription>
                </CardHeader>
                <CardContent className="pt-4">
                    <MetricStrip columns={4} aria-label="试算指标">
                        <MetricItem
                            label="总行数"
                            value={batch.metrics.total}
                        />
                        <MetricItem
                            label="可应用"
                            value={batch.metrics.valid}
                        />
                        <MetricItem
                            label="冲突"
                            value={batch.metrics.conflict}
                        />
                        <MetricItem label="问题" value={batch.metrics.failed} />
                    </MetricStrip>
                    <div className="mt-4 flex flex-wrap gap-2">
                        <Button
                            type="button"
                            size="sm"
                            variant="secondary"
                            onClick={() => onGoSection("trial")}
                        >
                            查看问题明细
                        </Button>
                        <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            onClick={() => onGoSection("confirm")}
                        >
                            责任确认
                        </Button>
                    </div>
                </CardContent>
            </Card>

            <Card size="sm" className={surfacePanelClassName}>
                <CardHeader className="border-b border-border/30">
                    <CardTitle>期初口径</CardTitle>
                    <CardDescription>
                        提示按本批对象固定生成，不可修改。
                    </CardDescription>
                </CardHeader>
                <CardContent className="space-y-3 pt-4">
                    {batch.openingPolicyHints.map((hint) => (
                        <div
                            key={hint.objectCode}
                            className="space-y-1 text-sm"
                        >
                            <div className="font-medium">
                                {OBJECT_CODE_LABEL[hint.objectCode]}
                            </div>
                            <p className="text-muted-foreground">
                                {hint.message}
                            </p>
                        </div>
                    ))}
                    {batch.sourceObjectSet.includes("CARD_OPENING_AR") ||
                    batch.sourceObjectSet.includes("CARD_SALES_ORDER") ? (
                        <Button
                            size="sm"
                            variant="outline"
                            render={<Link href="/finance/card-funds-review" />}
                        >
                            前往卡券票款复核
                            <ExternalLinkIcon className="size-4" />
                        </Button>
                    ) : null}
                    {batch.sourceObjectSet.includes("OPENING_STOCK") ? (
                        <Button
                            size="sm"
                            variant="outline"
                            render={<Link href="/inventory?view=balance" />}
                        >
                            查看库存台账
                            <ExternalLinkIcon className="size-4" />
                        </Button>
                    ) : null}
                </CardContent>
            </Card>

            {batch.invalidation ? (
                <Alert variant="warning" className="lg:col-span-2">
                    <TriangleAlertIcon />
                    <AlertTitle>旧确认已失效</AlertTitle>
                    <AlertDescription>
                        {batch.invalidation.reason}（
                        {formatDateTime(
                            batch.invalidation.invalidatedAt,
                            "dateStyle",
                            "passthrough",
                        )}
                        ）。禁止按旧试算版本{" "}
                        <span className="num font-mono">
                            {batch.invalidation.previousTrialVersion}
                        </span>{" "}
                        提交应用。
                    </AlertDescription>
                </Alert>
            ) : null}
        </div>
    )
}

function FilesSection({ batch }: { batch: ImportBatchView }) {
    const [previewAsset, setPreviewAsset] = React.useState<string | null>(null)
    return (
        <div className="grid gap-4 lg:grid-cols-2">
            <Card size="sm" className={surfacePanelClassName}>
                <CardHeader className="border-b border-border/30">
                    <CardTitle>合规输入包</CardTitle>
                    <CardDescription>
                        仅展示白名单包元数据；不展示原始存储键、签名 URL
                        或文件正文。
                    </CardDescription>
                </CardHeader>
                <CardContent className="space-y-3 pt-4 text-sm">
                    {batch.inputAsset ? (
                        <>
                            <Fact
                                label="文件名"
                                value={batch.inputAsset.fileName}
                            />
                            <Fact
                                label="大小"
                                value={formatBytes(batch.inputAsset.byteSize)}
                                mono
                            />
                            <Fact
                                label="安全检查"
                                value={
                                    batch.inputAsset.securityScanStatus ===
                                    "PASSED"
                                        ? "通过"
                                        : batch.inputAsset
                                                .securityScanStatus ===
                                            "PENDING"
                                          ? "待扫描"
                                          : batch.inputAsset
                                                  .securityScanStatus ===
                                              "REJECTED"
                                            ? "拒绝"
                                            : "隔离"
                                }
                            />
                            {batch.inputAsset.contentHmacShort ? (
                                <Fact
                                    label={versionText.checksumShort}
                                    value={batch.inputAsset.contentHmacShort}
                                    mono
                                />
                            ) : null}
                            <Fact
                                label="保留策略"
                                value={
                                    RETENTION_LABEL[
                                        batch.inputAsset.retentionClass
                                    ]
                                }
                            />
                        </>
                    ) : (
                        <p className="text-muted-foreground">
                            尚未接收合规包。
                        </p>
                    )}
                    <Separator />
                    <p className="text-xs text-muted-foreground">
                        禁止内容：原始
                        SQL、数据库连接头、商城禁止字段导出。此类文件不得长期留存，也不能在本页展示。
                    </p>
                </CardContent>
            </Card>

            <Card size="sm" className={surfacePanelClassName}>
                <CardHeader className="border-b border-border/30">
                    <CardTitle>结果与诊断资产保留</CardTitle>
                    <CardDescription>
                        成功审计长期 · 失败诊断 30 天 · 导出 7
                        天；下载前重鉴权。
                    </CardDescription>
                </CardHeader>
                <CardContent className="space-y-3 pt-4">
                    {batch.resultAssets.length === 0 ? (
                        <p className="text-sm text-muted-foreground">
                            尚无结果资产。
                        </p>
                    ) : (
                        batch.resultAssets.map((a) => (
                            <div
                                key={a.assetId}
                                className="rounded-lg border px-3 py-2 text-sm"
                            >
                                <div className="font-medium">{a.fileName}</div>
                                <div className="mt-1 text-xs text-muted-foreground">
                                    {RETENTION_LABEL[a.retentionClass]}
                                    {a.expiresAt
                                        ? ` · 到期 ${formatDateTime(a.expiresAt, "dateStyle", "passthrough")}`
                                        : " · 无到期"}
                                    {" · "}
                                    {formatBytes(a.byteSize)}
                                </div>
                                <Button
                                    type="button"
                                    size="sm"
                                    variant="outline"
                                    className="mt-2 h-7 text-xs"
                                    onClick={() => setPreviewAsset(a.fileName)}
                                >
                                    查看（示例）
                                </Button>
                                {previewAsset === a.fileName ? (
                                    <p className="mt-2 text-xs text-muted-foreground">
                                        示例：文件正文不在此处展示，仅保留元数据与保留策略。
                                    </p>
                                ) : null}
                            </div>
                        ))
                    )}
                </CardContent>
            </Card>
        </div>
    )
}

function TrialSection({
    batch,
    urlState,
    patchUrl,
    issueQuery,
}: {
    batch: ImportBatchView
    urlState: ImportOpeningUrlState
    patchUrl: (patch: Partial<ImportOpeningUrlState>) => void
    issueQuery: ReturnType<typeof useImportIssuesQuery>
}) {
    const issues = issueQuery.data?.rows ?? []

    return (
        <div className="space-y-4">
            <Alert>
                <AlertTitle>问题表范围</AlertTitle>
                <AlertDescription>
                    仅展示失败、冲突、跳过与待映射行；不混入成功长表。筛选写入
                    URL，刷新可恢复。
                </AlertDescription>
            </Alert>

            <div className="flex flex-wrap items-end gap-2">
                <div className="space-y-1">
                    <Label className="text-xs">错误码</Label>
                    <OptionCombobox
                        value={urlState.issueCode ?? "all"}
                        onValueChange={(v) => {
                            if (v == null) return
                            patchUrl({
                                issueCode:
                                    v === "all"
                                        ? undefined
                                        : (v as ImportIssueCode),
                                section: "trial",
                            })
                        }}
                        options={[
                            { value: "all", label: "全部错误码" },
                            ...(
                                Object.keys(
                                    ISSUE_CODE_LABEL,
                                ) as ImportIssueCode[]
                            ).map((code) => ({
                                value: code,
                                label: ISSUE_CODE_LABEL[code],
                            })),
                        ]}
                        className="w-[12rem]"
                        size="sm"
                        allowClear={false}
                    />
                </div>
                <div className="space-y-1">
                    <Label className="text-xs">对象</Label>
                    <OptionCombobox
                        value={urlState.issueObjectType ?? "all"}
                        onValueChange={(v) => {
                            if (v == null) return
                            patchUrl({
                                issueObjectType:
                                    v === "all"
                                        ? undefined
                                        : (v as ImportObjectCode),
                                section: "trial",
                            })
                        }}
                        options={[
                            { value: "all", label: "全部对象" },
                            ...batch.sourceObjectSet.map((code) => ({
                                value: code,
                                label: OBJECT_CODE_LABEL[code],
                            })),
                        ]}
                        className="w-[10rem]"
                        size="sm"
                        allowClear={false}
                    />
                </div>
                <div className="space-y-1">
                    <Label className="text-xs">处理状态</Label>
                    <OptionCombobox
                        value={urlState.rowStatus ?? "all"}
                        onValueChange={(v) => {
                            if (v == null) return
                            patchUrl({
                                rowStatus:
                                    v === "all"
                                        ? undefined
                                        : (v as IssueRowStatus),
                                section: "trial",
                            })
                        }}
                        options={[
                            { value: "all", label: "全部状态" },
                            ...(
                                Object.keys(
                                    ROW_STATUS_LABEL,
                                ) as IssueRowStatus[]
                            ).map((s) => ({
                                value: s,
                                label: ROW_STATUS_LABEL[s],
                            })),
                        ]}
                        className="w-[10rem]"
                        size="sm"
                        allowClear={false}
                    />
                </div>
                <Button
                    type="button"
                    size="sm"
                    variant="ghost"
                    onClick={() =>
                        patchUrl({
                            issueCode: undefined,
                            issueObjectType: undefined,
                            rowStatus: undefined,
                            section: "trial",
                        })
                    }
                >
                    清除筛选
                </Button>
            </div>

            <ImportIssueTable
                caption="导入问题明细（不含成功行）"
                emptyMessage={
                    issueQuery.isPending
                        ? "问题明细加载中…"
                        : "当前筛选下没有问题行"
                }
                repairableLabel="可在修复后重试"
                externalLabel="需外部处理后再试"
                issues={issues.map((issue) => ({
                    id: issue.issueId,
                    rowNumber: issue.sourceRowNo,
                    field: `${OBJECT_CODE_LABEL[issue.objectType]} · ${issue.sourceColumnName}`,
                    errorCode: issue.issueCode,
                    message: (
                        <span>
                            <span className="text-muted-foreground">
                                [{ROW_STATUS_LABEL[issue.rowStatus]}]{" "}
                            </span>
                            {issue.errorDetail}
                        </span>
                    ),
                    repairable: issue.repairable,
                }))}
            />
            <p className="text-xs text-muted-foreground">
                共 {issueQuery.data?.totalCount ?? 0} 条问题
            </p>
            <div className="flex items-center gap-2 text-xs text-muted-foreground">
                <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    disabled={urlState.page <= 1 || issueQuery.isFetching}
                    onClick={() =>
                        patchUrl({ page: urlState.page - 1, section: "trial" })
                    }
                >
                    上一页
                </Button>
                <span>
                    第 {urlState.page} /{" "}
                    {Math.max(
                        1,
                        Math.ceil((issueQuery.data?.totalCount ?? 0) / 20),
                    )}{" "}
                    页
                </span>
                <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    disabled={
                        issueQuery.isFetching ||
                        urlState.page * 20 >= (issueQuery.data?.totalCount ?? 0)
                    }
                    onClick={() =>
                        patchUrl({ page: urlState.page + 1, section: "trial" })
                    }
                >
                    下一页
                </Button>
            </div>
        </div>
    )
}

function ConfirmSection({
    batch,
    workItemTypeMissing,
    confirmBlocked,
}: {
    batch: ImportBatchView
    workItemTypeMissing: boolean
    confirmBlocked: ImportBatchView["actionBlockers"]
}) {
    return (
        <div className="space-y-4">
            {workItemTypeMissing ? (
                <FormalActionResult
                    status="blocked"
                    title="业务确认入口暂不可用"
                    description={WORK_ITEM_TYPE_BLOCKER.message}
                    facts={[
                        {
                            label: "待配置项",
                            value: WORK_ITEM_TYPE_BLOCKER.requiredRegistration.join(
                                "、",
                            ),
                        },
                        {
                            label: "禁止项",
                            value: "不得借用异常通道充当必经确认；不得上线页面私有任务类型",
                        },
                    ]}
                />
            ) : null}

            <div className="grid gap-3 md:grid-cols-2">
                {batch.confirmations.map((c) => {
                    const canAttempt =
                        c.inViewerResponsibility &&
                        !workItemTypeMissing &&
                        c.result === "PENDING"
                    return (
                        <Card
                            key={c.scope}
                            size="sm"
                            className={surfacePanelClassName}
                        >
                            <CardHeader className="border-b border-border/30">
                                <div className="flex flex-wrap items-center justify-between gap-2">
                                    <CardTitle className="text-base">
                                        {CONFIRMATION_SCOPE_LABEL[c.scope]}
                                    </CardTitle>
                                    <BusinessStatusBadge
                                        context="detail"
                                        label={
                                            c.result === "CONFIRMED"
                                                ? "已确认"
                                                : c.result === "REJECTED"
                                                  ? "已退回"
                                                  : c.result === "INVALIDATED"
                                                    ? "已失效"
                                                    : "待确认"
                                        }
                                        tone={
                                            c.result === "CONFIRMED"
                                                ? "success"
                                                : c.result === "REJECTED" ||
                                                    c.result === "INVALIDATED"
                                                  ? "destructive"
                                                  : "warning"
                                        }
                                    />
                                </div>
                                <CardDescription>
                                    试算版本{" "}
                                    <span className="num font-mono">
                                        {c.trialVersion}
                                    </span>
                                    {c.inViewerResponsibility
                                        ? " · 属于当前角色责任范围"
                                        : " · 非当前角色责任范围"}
                                </CardDescription>
                            </CardHeader>
                            <CardContent className="space-y-3 pt-4 text-sm">
                                {c.confirmedByLabel ? (
                                    <p>
                                        确认人 {c.confirmedByLabel}
                                        {c.confirmedAt
                                            ? ` · ${formatDateTime(c.confirmedAt, "dateStyle", "passthrough")}`
                                            : ""}
                                    </p>
                                ) : null}
                                {c.comment ? (
                                    <p className="text-muted-foreground">
                                        {c.comment}
                                    </p>
                                ) : null}
                                <div className="flex flex-wrap gap-2">
                                    {workItemTypeMissing ? (
                                        <Button
                                            type="button"
                                            size="sm"
                                            variant="outline"
                                            render={
                                                <Link href="/workspace/tasks" />
                                            }
                                        >
                                            去待办队列处理
                                            <ExternalLinkIcon className="size-4" />
                                        </Button>
                                    ) : (
                                        <>
                                            <Button
                                                type="button"
                                                size="sm"
                                                disabled={!canAttempt}
                                            >
                                                确认本范围
                                            </Button>
                                            <Button
                                                type="button"
                                                size="sm"
                                                variant="outline"
                                                disabled={!canAttempt}
                                            >
                                                退回修复
                                            </Button>
                                        </>
                                    )}
                                </div>
                                {!canAttempt ? (
                                    <p className="text-xs text-muted-foreground">
                                        {workItemTypeMissing
                                            ? "确认与退回任务尚未配置，入口暂不可用"
                                            : !c.inViewerResponsibility
                                              ? "非本人责任范围，只读"
                                              : c.result !== "PENDING"
                                                ? "本范围已有结论或已失效"
                                                : "当前不可操作"}
                                    </p>
                                ) : null}
                            </CardContent>
                        </Card>
                    )
                })}
            </div>

            {confirmBlocked.length > 0 ? (
                <ul className="space-y-1 text-sm text-muted-foreground">
                    {confirmBlocked.map((b) => (
                        <li key={`${b.action}-${b.code}`}>{b.message}</li>
                    ))}
                </ul>
            ) : null}
        </div>
    )
}

function ProgressSection({ batch }: { batch: ImportBatchView }) {
    const job = batch.backgroundJob
    if (!job) {
        return (
            <BusinessEmptyState
                kind="no-data"
                title="暂无导入任务"
                description="提交应用后将在此展示任务号、成功/跳过/失败计数与最近进度时间；刷新可恢复进度。"
            />
        )
    }

    return (
        <div className="space-y-4">
            <BackgroundJobProgress
                mode="partialAllowed"
                status={job.status}
                total={job.total}
                completed={job.processed}
                succeeded={job.succeeded}
                skipped={job.skipped}
                failed={job.failed}
                label={`导入执行进度 · ${batch.batchNo}`}
                description={
                    <span>
                        最近进度{" "}
                        {formatDateTime(
                            job.updatedAt,
                            "dateStyle",
                            "passthrough",
                        )}{" "}
                        ·
                        允许部分成功；已形成的处理结果不会因同批其它失败而回退。
                    </span>
                }
            />
            <Card size="sm" className={surfacePanelClassName}>
                <CardContent className="grid gap-3 pt-4 sm:grid-cols-4">
                    <Fact
                        label="已处理"
                        value={`${job.processed}/${job.total}`}
                        mono
                    />
                    <Fact label="成功" value={job.succeeded} mono />
                    <Fact label="跳过" value={job.skipped} mono />
                    <Fact label="失败" value={job.failed} mono />
                </CardContent>
            </Card>
        </div>
    )
}

function ResultSection({
    batch,
    onOpenRepair,
}: {
    batch: ImportBatchView
    onOpenRepair: (batchId: string) => void
}) {
    const partitions = batch.applyPartitions

    if (!partitions && batch.stage !== "RESULT") {
        return (
            <BusinessEmptyState
                kind="no-data"
                title="结果尚未形成"
                description="导入完成后，此处将列出成功、跳过与失败项，失败的记录可在此重新处理。"
            />
        )
    }

    return (
        <div className="space-y-4">
            {partitions ? (
                <BatchOperationResult
                    title="应用结果分区"
                    succeeded={partitions.succeeded.map((i) => ({
                        id: i.id,
                        label: i.label,
                        detail: i.detail,
                        code: i.code,
                    }))}
                    skipped={partitions.skipped.map((i) => ({
                        id: i.id,
                        label: i.label,
                        detail: i.detail,
                        code: i.code,
                    }))}
                    failed={partitions.failed.map((i) => ({
                        id: i.id,
                        label: i.label,
                        detail: i.detail,
                        code: i.code,
                    }))}
                    retryAction={
                        batch.repairBatchId ? (
                            <Button
                                type="button"
                                size="sm"
                                onClick={() =>
                                    onOpenRepair(batch.repairBatchId!)
                                }
                            >
                                打开修复批次
                                {batch.repairBatchNo
                                    ? ` ${batch.repairBatchNo}`
                                    : ""}
                            </Button>
                        ) : undefined
                    }
                />
            ) : null}

            {batch.backgroundJob ? (
                <BackgroundJobProgress
                    mode="partialAllowed"
                    status={batch.backgroundJob.status}
                    total={batch.backgroundJob.total}
                    completed={batch.backgroundJob.processed}
                    succeeded={batch.backgroundJob.succeeded}
                    skipped={batch.backgroundJob.skipped}
                    failed={batch.backgroundJob.failed}
                    label="最终应用进度"
                />
            ) : null}

            <Alert>
                <AlertTitle>防重复与不可覆盖</AlertTitle>
                <AlertDescription>
                    已导入成功的数据不会因取消、重试或上传新文件而被覆盖或删除；重新处理仅针对失败项。
                </AlertDescription>
            </Alert>
        </div>
    )
}

function AuditSection({ batch }: { batch: ImportBatchView }) {
    return (
        <Card size="sm" className={surfacePanelClassName}>
            <CardHeader className="border-b border-border/30">
                <CardTitle>可追溯谱系</CardTitle>
                <CardDescription>
                    来源身份、规则版本、manifest、成功结果与映射谱系可审计；详细事件在权限与审计中。
                </CardDescription>
            </CardHeader>
            <CardContent className="grid gap-3 pt-4 sm:grid-cols-2">
                <Fact label="批次号" value={batch.batchNo} mono />
                <Fact label="规则版本" value={batch.importRuleVersion} mono />
                <Fact label="试算版本" value={batch.trialVersion} mono />
                <Fact label="批次版本" value={batch.version} mono />
                {batch.inputAsset?.contentHmacShort ? (
                    <Fact
                        label={versionText.packageChecksum}
                        value={batch.inputAsset.contentHmacShort}
                        mono
                    />
                ) : null}
                {batch.linkedValidationBatchNo ? (
                    <Fact
                        label="关联验证/源批次"
                        value={batch.linkedValidationBatchNo}
                    />
                ) : null}
                <div className="sm:col-span-2">
                    <Button
                        size="sm"
                        variant="outline"
                        render={
                            <Link
                                href={`/system/access-audit?objectType=legacy_import_batch&objectId=${encodeURIComponent(batch.batchId)}`}
                            />
                        }
                    >
                        在权限与审计中查看
                        <ExternalLinkIcon className="size-4" />
                    </Button>
                </div>
            </CardContent>
        </Card>
    )
}
