"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import type { PaginationState } from "@tanstack/react-table"
import {
    ExternalLinkIcon,
    PauseIcon,
    RefreshCwIcon,
    SearchIcon,
    ShieldAlertIcon,
    TriangleAlertIcon,
} from "lucide-react"

import {
    DataFreshness,
    FormalActionConfirmDialog,
    FormalActionResult,
    MaintenanceBanner,
    ListToolbar,
    MetricFilterItem,
    MetricStrip,
    OptionCombobox,
    PageHeader,
    PageScaffold,
    surfacePanelClassName,
} from "@/components/business"
import { useAppForm } from "@/components/form"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogClose,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { Label } from "@/components/ui/label"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import type { MallSyncViewName } from "@/features/mall-sync/types"
import {
    DEFER_REASON_OPTIONS,
    DIRECTION_LABEL,
    STAGE_LABEL,
    VIEW_LABEL,
} from "@/features/mall-sync/types"
import {
    useClaimMappingMutation,
    useConfirmMappingMutation,
    useDeferMappingMutation,
    useMallSyncPageQuery,
    useReapplyMutation,
    useResolveUnknownReapplyMutation,
    useRetryJobMutation,
    useTriggerIncrementalMutation,
    useTriggerSingleOrderMutation,
} from "@/features/mall-sync/hooks/queries"
import { useMallSyncColumns } from "@/features/mall-sync/hooks/mall-sync-columns"
import { MallSyncMappingView } from "@/features/mall-sync/components/mall-sync-mapping-view"
import { MallSyncReadViews } from "@/features/mall-sync/components/mall-sync-read-views"
import {
    ALL_OBJECT_PARAMS,
    confirmSchema,
    deferSchema,
    incrementalSchema,
    parseView,
    pullSchema,
    type SessionLease,
    VIEW_OBJECT_PARAMS,
    VIEWS,
} from "@/features/mall-sync/lib/presentation"
import { SourceSystemsCard } from "@/features/mall-sync/components/source-systems-card"
import { formatDateTime } from "@/lib/datetime"
import { getErrorMessage } from "@/lib/api/errors"
import { patchUrl as patchSearchParams } from "@/lib/patch-search-params"
import { type ResultState } from "@/components/business/feedback"
import { workspaceLabel } from "@/lib/ui-text"

export function MallSyncPage() {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()

    const view = parseView(searchParams.get("view"))
    const q = searchParams.get("q") ?? ""
    const jobId = searchParams.get("jobId") ?? undefined
    const snapshotId = searchParams.get("snapshotId") ?? undefined
    const mappingTaskId = searchParams.get("mappingTaskId") ?? undefined
    const workItemId =
        searchParams.get("workItemId") ??
        searchParams.get("currentWorkItemId") ??
        undefined
    const differenceId = searchParams.get("differenceId") ?? undefined
    const queueContextId =
        searchParams.get("queueContextId") ?? "queue:W17:mall-sync"

    const [searchInput, setSearchInput] = React.useState(q)
    const searchInputRef = React.useRef<HTMLInputElement | null>(null)
    const [pagination, setPagination] = React.useState<PaginationState>({
        pageIndex: 0,
        pageSize: 20,
    })
    const [sessionLease, setSessionLease] = React.useState<SessionLease | null>(
        null,
    )
    const [selectedCandidateId, setSelectedCandidateId] = React.useState<
        string | null
    >(null)
    const [result, setResult] = React.useState<ResultState>(null)
    const [confirmOpen, setConfirmOpen] = React.useState(false)
    const [deferOpen, setDeferOpen] = React.useState(false)
    const [pullOpen, setPullOpen] = React.useState(false)
    const [incrementalOpen, setIncrementalOpen] = React.useState(false)
    const [retryConfirmOpen, setRetryConfirmOpen] = React.useState(false)
    const [actionError, setActionError] = React.useState<string | null>(null)

    const queryInput = React.useMemo(
        () => ({
            view,
            q: q || undefined,
            jobId,
            snapshotId,
            mappingTaskId,
            workItemId,
            differenceId,
            queueContextId,
        }),
        [
            view,
            q,
            jobId,
            snapshotId,
            mappingTaskId,
            workItemId,
            differenceId,
            queueContextId,
        ],
    )

    const pageQuery = useMallSyncPageQuery(queryInput)
    const triggerInc = useTriggerIncrementalMutation()
    const triggerSo = useTriggerSingleOrderMutation()
    const retryJob = useRetryJobMutation()
    const claimMutation = useClaimMappingMutation()
    const confirmMutation = useConfirmMappingMutation()
    const deferMutation = useDeferMappingMutation()
    const reapplyMutation = useReapplyMutation()
    const resolveReapply = useResolveUnknownReapplyMutation()

    const data = pageQuery.data
    const context = data?.context
    const ownership = context?.ownership
    const policyState = context?.manualGovernancePolicy
    const policyMissing = policyState?.state === "MISSING"
    const stage = ownership?.stage ?? "FIRST_PHASE_MALL_OWNED"
    const firstPhase = stage === "FIRST_PHASE_MALL_OWNED"
    const sealed = stage === "SECOND_PHASE_ERP_OWNED"

    const mappingTask = data?.selectedMappingTask
    const mappingIndex = React.useMemo(() => {
        if (!data?.mappingTasks.length || !mappingTask)
            return { current: 0, total: 0 }
        const idx = data.mappingTasks.findIndex(
            (t) => t.mappingTaskId === mappingTask.mappingTaskId,
        )
        return {
            current: idx >= 0 ? idx + 1 : 1,
            total: data.mappingTasks.length,
        }
    }, [data?.mappingTasks, mappingTask])

    React.useEffect(() => {
        setSearchInput(q)
    }, [q])

    React.useEffect(() => {
        const handle = globalThis.setTimeout(() => {
            if (searchInput === q) return
            patchUrl({ q: searchInput.trim() || null }, { replace: true })
        }, 300)
        return () => globalThis.clearTimeout(handle)
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [searchInput])

    // / 聚焦搜索
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

    // 封存后默认引导 history
    React.useEffect(() => {
        if (
            sealed &&
            view !== "history" &&
            !jobId &&
            !snapshotId &&
            !mappingTaskId
        ) {
            // 不强制 replace 每次；仅当无对象 id 时提示在 banner
        }
    }, [sealed, view, jobId, snapshotId, mappingTaskId])

    // 切换映射任务时重置候选与租约绑定
    React.useEffect(() => {
        setSelectedCandidateId(null)
        setActionError(null)
        if (
            sessionLease &&
            mappingTask?.ownerRoutingState === "CONFIGURED" &&
            sessionLease.workItemId !== mappingTask.workItem.workItemId
        ) {
            setSessionLease(null)
        }
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [mappingTask?.mappingTaskId])

    function patchUrl(
        patch: Record<string, string | null | undefined>,
        options?: { replace?: boolean },
    ) {
        patchSearchParams(
            { router, pathname, searchParams, view },
            patch,
            options,
        )
    }

    const clearObjectParamsForView = React.useCallback(
        (next: MallSyncViewName) => {
            const keep = new Set(VIEW_OBJECT_PARAMS[next])
            const patch: Record<string, null> = {}
            for (const key of ALL_OBJECT_PARAMS) {
                if (!keep.has(key)) patch[key] = null
            }
            return patch
        },
        [],
    )

    const hasActiveFilters = Boolean(
        q || jobId || snapshotId || mappingTaskId || workItemId || differenceId,
    )

    const clearAllFilters = () => {
        setSearchInput("")
        patchUrl(
            {
                q: null,
                jobId: null,
                snapshotId: null,
                mappingTaskId: null,
                workItemId: null,
                currentWorkItemId: null,
                differenceId: null,
            },
            { replace: true },
        )
    }

    const confirmForm = useAppForm({
        defaultValues: { evidenceNote: "" },
        validators: { onChange: confirmSchema },
        onSubmit: async () => {
            setConfirmOpen(true)
        },
    })

    const deferForm = useAppForm({
        defaultValues: {
            reasonCode: "WAITING_SOURCE" as
                | "WAITING_SOURCE"
                | "NEED_CLARIFICATION"
                | "WAITING_MASTER_DATA"
                | "OTHER",
            note: "",
        },
        validators: { onChange: deferSchema },
        onSubmit: async ({ value }) => {
            await handleDefer(value.reasonCode, value.note)
        },
    })

    const pullForm = useAppForm({
        defaultValues: { externalOrderNo: "", reason: "" },
        validators: { onChange: pullSchema },
        onSubmit: async ({ value }) => {
            const res = await triggerSo.mutateAsync({
                externalOrderNo: value.externalOrderNo,
                reason: value.reason,
                policyConfigured: !policyMissing,
                stage,
            })
            if (res.status === "succeeded") {
                setResult({
                    status: "succeeded",
                    title: "按单补拉已受理",
                    description: res.message,
                    reference: res.jobNo,
                })
                setPullOpen(false)
                patchUrl({ view: "jobs", jobId: res.jobId })
            } else {
                setActionError(res.message)
            }
        },
    })

    const incrementalForm = useAppForm({
        defaultValues: { reason: "" },
        validators: { onChange: incrementalSchema },
        onSubmit: async ({ value }) => {
            const res = await triggerInc.mutateAsync({
                reason: value.reason,
                policyConfigured: !policyMissing,
                stage,
            })
            if (res.status === "succeeded") {
                setResult({
                    status: "succeeded",
                    title: "立即增量已受理",
                    description: res.message,
                    reference: res.jobNo,
                })
                setIncrementalOpen(false)
                patchUrl({ view: "jobs", jobId: res.jobId })
            } else {
                setActionError(res.message)
            }
        },
    })

    async function handleClaim() {
        if (mappingTask?.ownerRoutingState !== "CONFIGURED") return
        setActionError(null)
        try {
            const lease = await claimMutation.mutateAsync({
                workItemId: mappingTask.workItem.workItemId,
                subjectVersion: mappingTask.workItem.subjectVersion,
            })
            setSessionLease({
                workItemId: lease.workItemId,
                subjectVersion: lease.subjectVersion,
            })
        } catch (e) {
            setActionError(getErrorMessage(e, "领取失败"))
        }
    }

    async function handleConfirm() {
        if (mappingTask?.ownerRoutingState !== "CONFIGURED" || !sessionLease)
            return
        const candidate = mappingTask.candidateTargets.find(
            (c) => c.objectId === selectedCandidateId,
        )
        if (!candidate || candidate.eligibility !== "ELIGIBLE") {
            setActionError("请选择可用的 ERP 候选（相似不自动确认）")
            return
        }
        const evidenceNote = String(
            confirmForm.getFieldValue("evidenceNote") ?? "",
        ).trim()
        const res = await confirmMutation.mutateAsync({
            mappingTaskId: mappingTask.mappingTaskId,
            workItemId: mappingTask.workItem.workItemId,
            expectedSubjectVersion: mappingTask.workItem.subjectVersion,
            expectedLockVersion: mappingTask.lockVersion,
            targetObjectId: candidate.objectId,
            targetLabel: `${candidate.stableNo} ${candidate.label}`,
            evidenceNote,
            stage,
        })
        setConfirmOpen(false)
        if (res.status === "succeeded") {
            setResult({
                status: "succeeded",
                title: "映射已确认",
                description: res.message,
                facts: [
                    {
                        label: "已确认目标",
                        value: `${candidate.stableNo} ${candidate.label}`,
                    },
                ],
            })
            setSessionLease(null)
            void pageQuery.refetch()
            // 与「先跳过」一致：自动定位到下一项
            const tasks = data?.mappingTasks ?? []
            const idx = tasks.findIndex(
                (t) => t.mappingTaskId === mappingTask.mappingTaskId,
            )
            const next = tasks[idx + 1]
            if (next) {
                patchUrl({
                    view: "mapping",
                    mappingTaskId: next.mappingTaskId,
                    workItemId:
                        next.ownerRoutingState === "CONFIGURED"
                            ? next.workItem.workItemId
                            : null,
                })
            }
        } else {
            setActionError(res.message)
        }
    }

    async function handleDefer(reasonCode: string, note?: string) {
        if (mappingTask?.ownerRoutingState !== "CONFIGURED" || !sessionLease) {
            setActionError("请先领取任务")
            return
        }
        const res = await deferMutation.mutateAsync({
            mappingTaskId: mappingTask.mappingTaskId,
            workItemId: mappingTask.workItem.workItemId,
            expectedSubjectVersion: mappingTask.workItem.subjectVersion,
            reasonCode,
            note,
            queueContextId,
        })
        setDeferOpen(false)
        if (res.status === "succeeded") {
            setResult({
                status: "succeeded",
                title: "已跳过（任务仍在待处理列表）",
                description: res.message,
            })
            setSessionLease(null)
            // 移动到下一项（仅浏览游标）
            const tasks = data?.mappingTasks ?? []
            const idx = tasks.findIndex(
                (t) => t.mappingTaskId === mappingTask.mappingTaskId,
            )
            const next = tasks[idx + 1]
            if (next) {
                patchUrl({
                    view: "mapping",
                    mappingTaskId: next.mappingTaskId,
                    workItemId:
                        next.ownerRoutingState === "CONFIGURED"
                            ? next.workItem.workItemId
                            : null,
                })
            }
        } else {
            setActionError(res.message)
        }
    }

    async function handleReapply() {
        if (!mappingTask) return
        const res = await reapplyMutation.mutateAsync({
            mappingTaskId: mappingTask.mappingTaskId,
            sourceSnapshotId: mappingTask.sourceSnapshotId,
            stage,
        })
        if (res.status === "succeeded") {
            setResult({
                status: "succeeded",
                title: "重新归集成功",
                description: res.message,
                reference: res.salesOrderNo,
            })
            void pageQuery.refetch()
        } else if (res.status === "unknown") {
            setResult({
                status: "unknown",
                title: "重新归集结果未知",
                description: res.message,
                stayOnItem: true,
                pendingIdempotencyKey: res.idempotencyKey,
                reference: res.operationId,
            })
            void pageQuery.refetch()
        } else {
            setActionError(res.message)
        }
    }

    async function handleRetryJob() {
        if (!data?.selectedJob) return
        const res = await retryJob.mutateAsync({
            jobId: data.selectedJob.jobId,
            reason: "重试未成功部分的分页",
            stage,
        })
        setRetryConfirmOpen(false)
        if (res.status === "succeeded") {
            setResult({
                status: "succeeded",
                title: "重试已创建",
                description: res.message,
                reference: res.jobNo,
            })
            void pageQuery.refetch()
        } else {
            setActionError(res.message)
        }
    }

    async function handleResolveUnknownReapply() {
        if (!mappingTask?.reapplyOperation) return
        const res = await resolveReapply.mutateAsync({
            mappingTaskId: mappingTask.mappingTaskId,
            operationId: mappingTask.reapplyOperation.operationId,
            settle: true,
        })
        if (res.status === "succeeded") {
            setResult({
                status: "succeeded",
                title: "重新归集结果已确认",
                description: res.message,
                reference: res.salesOrderNo,
            })
        } else if (res.status === "unknown") {
            setResult({
                status: "unknown",
                title: "仍为结果未知",
                description: res.message,
                stayOnItem: true,
            })
        } else {
            setActionError(res.message)
        }
    }

    const canManualSync =
        firstPhase && !policyMissing && !context?.sourceUnavailable
    const manualSyncDisabledReason = !firstPhase
        ? "已封存：无第一期写动作"
        : policyMissing
          ? "人工治理策略未配置：立即增量/按单补拉已禁用"
          : context?.sourceUnavailable
            ? "来源不可用时不新建推进任务（可重试既有失败）"
            : null

    const { diffColumns, jobColumns, mappingColumns, snapshotColumns } =
        useMallSyncColumns({ patchUrl, searchParams })
    const leaseStatus: "active" | "unclaimed" | "lost" | "released" =
        mappingTask?.ownerRoutingState !== "CONFIGURED"
            ? "released"
            : mappingTask.mappingTaskStatus === "RESOLVED"
              ? "released"
              : sessionLease?.workItemId === mappingTask.workItem.workItemId
                ? "active"
                : "unclaimed"

    const canConfirmMapping =
        mappingTask?.ownerRoutingState === "CONFIGURED" &&
        mappingTask.mappingTaskStatus === "PENDING" &&
        mappingTask.allowedActions.includes("CONFIRM_TARGET") &&
        leaseStatus === "active" &&
        !!selectedCandidateId &&
        !mappingTask.hasConflict

    const mappingConfirmForm =
        mappingTask?.ownerRoutingState === "CONFIGURED" &&
        mappingTask.mappingTaskStatus === "PENDING" ? (
            <form
                className="space-y-2"
                onSubmit={(e) => {
                    e.preventDefault()
                    void confirmForm.handleSubmit()
                }}
            >
                <confirmForm.AppField
                    name="evidenceNote"
                    children={(field) => (
                        <field.TextareaField
                            label="确认依据"
                            placeholder="说明选择该 ERP 对象的业务依据"
                        />
                    )}
                />
                <div className="flex flex-wrap gap-2">
                    <confirmForm.AppForm>
                        <confirmForm.SubmitButton
                            label="确认映射"
                            disabled={!canConfirmMapping}
                        />
                    </confirmForm.AppForm>
                    <Button
                        type="button"
                        variant="outline"
                        size="sm"
                        disabled={leaseStatus !== "active"}
                        onClick={() => setDeferOpen(true)}
                    >
                        <PauseIcon className="size-4" />
                        先跳过
                    </Button>
                </div>
                {!selectedCandidateId ? (
                    <p className="text-xs text-muted-foreground">
                        请先选择左侧 ERP 候选后即可确认。
                    </p>
                ) : mappingTask.hasConflict ? (
                    <p className="text-xs text-muted-foreground">
                        冲突未解决前确认禁用。
                    </p>
                ) : leaseStatus !== "active" ? (
                    <p className="text-xs text-muted-foreground">
                        请先领取任务后确认。
                    </p>
                ) : null}
            </form>
        ) : null

    const pageJobs = React.useMemo(() => {
        const rows = data?.jobs ?? []
        const start = pagination.pageIndex * pagination.pageSize
        return rows.slice(start, start + pagination.pageSize)
    }, [data?.jobs, pagination])

    if (pageQuery.isPending && !data) {
        return (
            <PageScaffold>
                <div className="h-10 w-56 animate-pulse rounded-lg bg-muted" />
                <div className="h-16 animate-pulse rounded-lg bg-muted" />
                <div className="h-24 animate-pulse rounded-lg bg-muted" />
                <div className="grid gap-4 lg:grid-cols-2">
                    <div className="h-72 animate-pulse rounded-lg bg-muted" />
                    <div className="h-72 animate-pulse rounded-lg bg-muted" />
                </div>
            </PageScaffold>
        )
    }

    if (pageQuery.isError) {
        return (
            <PageScaffold>
                <PageHeader title="商城同步与映射" description="加载失败" />
                <Alert variant="destructive">
                    <AlertTitle>查询失败</AlertTitle>
                    <AlertDescription>
                        {(pageQuery.error as Error)?.message ?? "请重试"}
                    </AlertDescription>
                </Alert>
                <Button
                    type="button"
                    variant="secondary"
                    className="rounded-lg shadow-none"
                    onClick={() => void pageQuery.refetch()}
                >
                    重试
                </Button>
            </PageScaffold>
        )
    }

    return (
        <PageScaffold>
            <PageHeader
                title="商城同步与映射"
                breadcrumbs={[
                    {
                        id: "gov",
                        label: "治理",
                        href: "/governance",
                        current: false,
                    },
                    { id: "sync", label: "商城同步与映射", current: true },
                ]}
                metadata={
                    <div className="flex flex-wrap items-center gap-3">
                        <DataFreshness
                            updatedAt={
                                context?.freshness.latestSuccessfulJobAt
                                    ? formatDateTime(
                                          context.freshness
                                              .latestSuccessfulJobAt,
                                          "default",
                                      )
                                    : "—"
                            }
                            dateTime={context?.freshness.latestSuccessfulJobAt}
                            state={
                                context?.sourceUnavailable ? "stale" : "fresh"
                            }
                            label="同步数据"
                        />
                        <Badge variant="outline">
                            {context?.sourceSystem.name} ·{" "}
                            {context?.sourceSystem.environmentLabel}
                        </Badge>
                    </div>
                }
                actions={
                    <div className="flex flex-wrap items-center gap-2">
                        <Button
                            type="button"
                            variant="secondary"
                            size="sm"
                            className="rounded-lg shadow-none"
                            disabled={!canManualSync}
                            title={
                                manualSyncDisabledReason ?? "立即增量（按策略）"
                            }
                            onClick={() => {
                                setActionError(null)
                                setIncrementalOpen(true)
                            }}
                        >
                            立即增量
                        </Button>
                        <Button
                            type="button"
                            size="sm"
                            disabled={!canManualSync}
                            title={manualSyncDisabledReason ?? "按单号补拉"}
                            onClick={() => {
                                setActionError(null)
                                setPullOpen(true)
                            }}
                        >
                            按单补拉
                        </Button>
                        <Button
                            type="button"
                            variant="ghost"
                            size="sm"
                            className="text-muted-foreground"
                            onClick={() => void pageQuery.refetch()}
                        >
                            <RefreshCwIcon className="size-4" aria-hidden />
                            刷新
                        </Button>
                    </div>
                }
            />

            {/* OwnershipBanner — 始终可见 */}
            {ownership ? (
                <MaintenanceBanner
                    tone={sealed ? "info" : "info"}
                    icon={sealed ? ShieldAlertIcon : undefined}
                    title={
                        sealed
                            ? `第一期已封存 · ${DIRECTION_LABEL[ownership.syncDirection]}`
                            : `当前主责：${STAGE_LABEL[ownership.stage]} · 方向 ${DIRECTION_LABEL[ownership.syncDirection]}`
                    }
                    description={
                        <div className="space-y-1 text-sm">
                            <p>
                                <span className="font-medium">商城边界：</span>
                                {ownership.mallWriteBoundary}
                            </p>
                            <p>
                                <span className="font-medium">ERP 边界：</span>
                                {ownership.erpWriteBoundary}
                            </p>
                            {ownership.sealedAt ? (
                                <p>
                                    封存时间{" "}
                                    {formatDateTime(
                                        ownership.sealedAt,
                                        "default",
                                    )}
                                    {ownership.finalWatermark
                                        ? ` · 最终同步点 ${formatDateTime(ownership.finalWatermark, "default")}`
                                        : ""}
                                </p>
                            ) : null}
                            <p className="text-muted-foreground">
                                无「编辑来源数据」「向商城回写商业修改」「手工标记同步成功」入口。
                            </p>
                        </div>
                    }
                />
            ) : null}

            {sealed && (
                <div className="flex flex-wrap gap-2 text-sm">
                    <Button
                        variant="link"
                        size="sm"
                        render={<Link href="/commerce/execution-projections" />}
                    >
                        {workspaceLabel("W23")}
                        <ExternalLinkIcon className="size-3.5" />
                    </Button>
                    <Button
                        variant="link"
                        size="sm"
                        render={<Link href="/governance/integration-errors" />}
                    >
                        {workspaceLabel("W29")}
                        <ExternalLinkIcon className="size-3.5" />
                    </Button>
                    {sealed && view !== "history" ? (
                        <Button
                            type="button"
                            variant="secondary"
                            size="sm"
                            onClick={() => patchUrl({ view: "history" })}
                        >
                            进入历史只读
                        </Button>
                    ) : null}
                </div>
            )}

            {policyMissing ? (
                <Alert variant="warning">
                    <TriangleAlertIcon />
                    <AlertTitle>人工同步治理策略未配置</AlertTitle>
                    <AlertDescription className="space-y-1">
                        <p>
                            「立即增量」「按单补拉」已禁用（界面与系统均拒绝）。
                        </p>
                        <p className="text-muted-foreground">
                            {context?.scheduledIncrementalNote}
                        </p>
                    </AlertDescription>
                </Alert>
            ) : (
                <Alert>
                    <AlertTitle>人工同步治理</AlertTitle>
                    <AlertDescription>
                        策略已配置 · 版本{" "}
                        {policyState?.state === "CONFIGURED"
                            ? policyState.policyVersion
                            : "—"}{" "}
                        · 模式{" "}
                        {policyState?.state === "CONFIGURED"
                            ? policyState.executionMode ===
                              "SINGLE_OPERATOR_REASON"
                                ? "单人执行"
                                : "双人复核"
                            : "—"}
                        。定时增量仍按调度独立运行。
                    </AlertDescription>
                </Alert>
            )}

            {context?.sourceUnavailable ? (
                <Alert variant="destructive">
                    <AlertTitle>来源商城不可用</AlertTitle>
                    <AlertDescription>
                        {context.sourceUnavailableMessage}
                    </AlertDescription>
                </Alert>
            ) : null}

            {/* 来源系统列表 */}
            <SourceSystemsCard />

            <MetricStrip
                columns={
                    Math.min(5, Math.max(2, context?.metrics.length ?? 4)) as
                        | 2
                        | 3
                        | 4
                        | 5
                }
                aria-label="商城同步指标"
            >
                {(context?.metrics ?? []).map((m) => (
                    <MetricFilterItem
                        key={m.key}
                        label={m.label}
                        value={m.count != null ? m.count : (m.value ?? "—")}
                        detail={m.detail}
                        active={view === m.targetView}
                        onClick={() => {
                            patchUrl({
                                view: m.targetView,
                                ...clearObjectParamsForView(m.targetView),
                            })
                        }}
                    />
                ))}
            </MetricStrip>

            <div
                className={`${surfacePanelClassName} sticky top-0 z-10 space-y-2.5 px-3 py-2.5`}
            >
                <Tabs
                    value={view}
                    onValueChange={(v) => {
                        const next = parseView(v)
                        patchUrl({
                            view: next,
                            // 清理跨视图残留的对象定位参数；保留当前视图归属的对象参数
                            ...clearObjectParamsForView(next),
                        })
                    }}
                >
                    <TabsList
                        variant="line"
                        className="w-full justify-start overflow-x-auto"
                    >
                        {VIEWS.map((v) => (
                            <TabsTrigger key={v} value={v}>
                                {VIEW_LABEL[v]}
                            </TabsTrigger>
                        ))}
                    </TabsList>
                </Tabs>
                <ListToolbar
                    aria-label="商城同步筛选"
                    search={
                        <InputGroup>
                            <InputGroupAddon>
                                <SearchIcon aria-hidden="true" />
                            </InputGroupAddon>
                            <InputGroupInput
                                ref={searchInputRef}
                                placeholder={
                                    view === "snapshots" || view === "mapping"
                                        ? "商城单号 / ERP 单号 / 任务号"
                                        : view === "jobs"
                                          ? "任务号"
                                          : "搜索仅对来源数据、同步任务与映射任务生效"
                                }
                                value={searchInput}
                                onChange={(e) => setSearchInput(e.target.value)}
                                aria-label="搜索"
                            />
                        </InputGroup>
                    }
                    actions={
                        hasActiveFilters ? (
                            <Button
                                type="button"
                                variant="ghost"
                                size="sm"
                                onClick={clearAllFilters}
                            >
                                清除筛选
                            </Button>
                        ) : null
                    }
                />
            </div>

            {result ? (
                <FormalActionResult
                    status={
                        result.status === "succeeded"
                            ? "succeeded"
                            : result.status === "unknown"
                              ? "unknown"
                              : result.status === "blocked"
                                ? "blocked"
                                : "rejected"
                    }
                    title={result.title}
                    description={result.description}
                    reference={result.reference}
                    facts={result.facts}
                    actions={
                        result.status === "unknown" &&
                        mappingTask?.reapplyOperation?.status === "UNKNOWN" ? (
                            <Button
                                type="button"
                                size="sm"
                                onClick={() =>
                                    void handleResolveUnknownReapply()
                                }
                            >
                                查询重新归集处理结果
                            </Button>
                        ) : undefined
                    }
                />
            ) : null}

            {actionError ? (
                <Alert variant="destructive">
                    <AlertTitle>动作失败</AlertTitle>
                    <AlertDescription>{actionError}</AlertDescription>
                </Alert>
            ) : null}

            {/* ── 子视图内容 ── */}
            <MallSyncReadViews
                view={view}
                context={context}
                ownership={ownership}
                data={data}
                pageJobs={pageJobs}
                jobColumns={jobColumns}
                snapshotColumns={snapshotColumns}
                diffColumns={diffColumns}
                pagination={pagination}
                onPaginationChange={setPagination}
                retryPending={retryJob.isPending}
                onRetryJob={() => setRetryConfirmOpen(true)}
                patchUrl={patchUrl}
                firstPhase={firstPhase}
                policyMissing={policyMissing}
                sealed={sealed}
                onPullDifference={(externalOrderNo) => {
                    setPullOpen(true)
                    pullForm.setFieldValue("externalOrderNo", externalOrderNo)
                }}
            />

            {view === "mapping" ? (
                <MallSyncMappingView
                    data={data}
                    mappingTask={mappingTask}
                    mappingColumns={mappingColumns}
                    selectedCandidateId={selectedCandidateId}
                    onSelectCandidate={setSelectedCandidateId}
                    confirmFormContent={mappingConfirmForm}
                    mappingIndex={mappingIndex}
                    leaseStatus={leaseStatus}
                    canConfirmMapping={canConfirmMapping}
                    reapplyPending={reapplyMutation.isPending}
                    onReapply={handleReapply}
                    onResolveUnknownReapply={handleResolveUnknownReapply}
                    onBackToQueue={() =>
                        router.push(
                            `/workspace/tasks?queueContextId=${encodeURIComponent(queueContextId)}`,
                        )
                    }
                    onConfirm={() => confirmForm.handleSubmit()}
                    onClaim={handleClaim}
                />
            ) : null}
            {/* 立即增量 */}
            <Dialog open={incrementalOpen} onOpenChange={setIncrementalOpen}>
                <DialogContent>
                    <DialogHeader>
                        <DialogTitle>立即执行增量</DialogTitle>
                        <DialogDescription>
                            不修改来源；范围由系统按当前同步进度计算。禁止页面改写同步进度。
                        </DialogDescription>
                    </DialogHeader>
                    {policyMissing ? (
                        <Alert variant="destructive">
                            <AlertTitle>人工治理策略未配置</AlertTitle>
                            <AlertDescription>
                                策略未配置，动作禁用。定时增量说明仍可读：
                                {context?.scheduledIncrementalNote}
                            </AlertDescription>
                        </Alert>
                    ) : (
                        <form
                            className="space-y-3"
                            onSubmit={(e) => {
                                e.preventDefault()
                                void incrementalForm.handleSubmit()
                            }}
                        >
                            <p className="text-sm text-muted-foreground">
                                同步至{" "}
                                {context?.freshness.currentWatermark
                                    ? formatDateTime(
                                          context.freshness.currentWatermark,
                                          "default",
                                      )
                                    : "—"}{" "}
                                · 阶段 {STAGE_LABEL[stage]}
                            </p>
                            <incrementalForm.AppField
                                name="reason"
                                children={(field) => (
                                    <field.TextField label="触发理由" />
                                )}
                            />
                            <DialogFooter>
                                <DialogClose
                                    render={
                                        <Button
                                            type="button"
                                            variant="outline"
                                        />
                                    }
                                >
                                    取消
                                </DialogClose>
                                <incrementalForm.AppForm>
                                    <incrementalForm.SubmitButton label="创建增量任务" />
                                </incrementalForm.AppForm>
                            </DialogFooter>
                        </form>
                    )}
                </DialogContent>
            </Dialog>

            {/* 按单补拉 */}
            <Dialog open={pullOpen} onOpenChange={setPullOpen}>
                <DialogContent>
                    <DialogHeader>
                        <DialogTitle>按单号补拉</DialogTitle>
                        <DialogDescription>
                            使用原来源身份；不创建第二张销售单。仅第一阶段（商城开单）且策略已配置时可用。
                        </DialogDescription>
                    </DialogHeader>
                    {policyMissing || !firstPhase ? (
                        <Alert variant="destructive">
                            <AlertTitle>
                                {policyMissing
                                    ? "人工治理策略未配置"
                                    : "阶段不可用"}
                            </AlertTitle>
                            <AlertDescription>
                                {manualSyncDisabledReason}
                            </AlertDescription>
                        </Alert>
                    ) : (
                        <form
                            className="space-y-3"
                            onSubmit={(e) => {
                                e.preventDefault()
                                void pullForm.handleSubmit()
                            }}
                        >
                            <pullForm.AppField
                                name="externalOrderNo"
                                children={(field) => (
                                    <field.TextField label="商城销售单号" />
                                )}
                            />
                            <pullForm.AppField
                                name="reason"
                                children={(field) => (
                                    <field.TextField label="补拉理由" />
                                )}
                            />
                            <DialogFooter>
                                <DialogClose
                                    render={
                                        <Button
                                            type="button"
                                            variant="outline"
                                        />
                                    }
                                >
                                    取消
                                </DialogClose>
                                <pullForm.AppForm>
                                    <pullForm.SubmitButton label="创建补拉任务" />
                                </pullForm.AppForm>
                            </DialogFooter>
                        </form>
                    )}
                </DialogContent>
            </Dialog>

            {/* 暂挂 */}
            <Dialog open={deferOpen} onOpenChange={setDeferOpen}>
                <DialogContent>
                    <DialogHeader>
                        <DialogTitle>先跳过当前映射</DialogTitle>
                        <DialogDescription>
                            只记录结构化原因与当前处理位置；不会暂停或完成任务。
                        </DialogDescription>
                    </DialogHeader>
                    <form
                        className="space-y-3"
                        onSubmit={(e) => {
                            e.preventDefault()
                            void deferForm.handleSubmit()
                        }}
                    >
                        <deferForm.AppField
                            name="reasonCode"
                            children={(field) => (
                                <div className="space-y-1.5">
                                    <Label>原因</Label>
                                    <OptionCombobox
                                        value={field.state.value}
                                        onValueChange={(v) => {
                                            if (v)
                                                field.handleChange(
                                                    v as
                                                        | "WAITING_SOURCE"
                                                        | "NEED_CLARIFICATION"
                                                        | "WAITING_MASTER_DATA"
                                                        | "OTHER",
                                                )
                                        }}
                                        options={DEFER_REASON_OPTIONS.map(
                                            (o) => ({
                                                value: o.value,
                                                label: o.label,
                                            }),
                                        )}
                                        allowClear={false}
                                    />
                                </div>
                            )}
                        />
                        <deferForm.AppField
                            name="note"
                            children={(field) => (
                                <field.TextareaField label="备注（可选）" />
                            )}
                        />
                        <DialogFooter>
                            <DialogClose
                                render={
                                    <Button type="button" variant="outline" />
                                }
                            >
                                取消
                            </DialogClose>
                            <deferForm.AppForm>
                                <deferForm.SubmitButton label="确认跳过" />
                            </deferForm.AppForm>
                        </DialogFooter>
                    </form>
                </DialogContent>
            </Dialog>

            <FormalActionConfirmDialog
                open={retryConfirmOpen}
                onOpenChange={setRetryConfirmOpen}
                actionLabel="重试失败任务"
                title="确认重试失败任务"
                description="沿原任务范围与同步规则重试未成功部分；不回退已捕获的同步进度。"
                fromStatus={{
                    label: data?.selectedJob?.statusLabel ?? "失败",
                    tone: "warning",
                }}
                toStatus={{ label: "重试中", tone: "info" }}
                effects={["仅重试未成功的分页", "不修改来源数据"]}
                irreversibleEffects={["重试记录进入任务审计"]}
                pending={retryJob.isPending}
                onConfirm={() => handleRetryJob()}
            />

            <FormalActionConfirmDialog
                open={confirmOpen}
                onOpenChange={setConfirmOpen}
                actionLabel="确认映射"
                fromStatus={{ label: "待处理", tone: "warning" }}
                toStatus={{ label: "映射已解决", tone: "success" }}
                description="确认身份关系后，映射任务将标为已解决并完成待办；不立即形成销售版本。"
                effects={[
                    "追加可审计映射目标",
                    "完成当前任务",
                    "不向商城回写",
                    "重新归集为独立下一步",
                ]}
                irreversibleEffects={["映射结论进入不可变处理审计"]}
                pending={confirmMutation.isPending}
                onConfirm={() => handleConfirm()}
            />
        </PageScaffold>
    )
}
