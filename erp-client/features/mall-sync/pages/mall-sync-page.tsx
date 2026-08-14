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
import type { ResponsibilityStatus } from "@/components/business/workflow-actions"
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
    DIRECTION_LABEL,
    SOURCE_FIX_REASON_OPTIONS,
    STAGE_LABEL,
    VIEW_LABEL,
} from "@/features/mall-sync/types"
import {
    useConfirmMappingMutation,
    useMallSyncPageQuery,
    useReapplyMutation,
    useResolveUnknownReapplyMutation,
    useRetryJobMutation,
    useRequestSourceFixMutation,
    useTriggerIncrementalMutation,
    useTriggerSingleOrderMutation,
} from "@/features/mall-sync/hooks/queries"
import { useMallSyncColumns } from "@/features/mall-sync/hooks/mall-sync-columns"
import { MallSyncMappingView } from "@/features/mall-sync/components/mall-sync-mapping-view"
import { MallSyncReadViews } from "@/features/mall-sync/components/mall-sync-read-views"
import {
    ALL_OBJECT_PARAMS,
    confirmSchema,
    incrementalSchema,
    parseView,
    pullSchema,
    releaseSchema,
    sourceFixSchema,
    VIEW_OBJECT_PARAMS,
    VIEWS,
} from "@/features/mall-sync/lib/presentation"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { useWorkItemResponsibilityMutation } from "@/features/work-items"
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
    const [selectedCandidateId, setSelectedCandidateId] = React.useState<
        string | null
    >(null)
    const [result, setResult] = React.useState<ResultState>(null)
    const [confirmOpen, setConfirmOpen] = React.useState(false)
    const [sourceFixOpen, setSourceFixOpen] = React.useState(false)
    const [releaseOpen, setReleaseOpen] = React.useState(false)
    const [pullOpen, setPullOpen] = React.useState(false)
    const [incrementalOpen, setIncrementalOpen] = React.useState(false)
    const [retryConfirmOpen, setRetryConfirmOpen] = React.useState(false)
    const [actionError, setActionError] = React.useState<string | null>(null)
    const commandIdentities = React.useRef(
        new Map<string, { idempotencyKey: string; operationId: string }>(),
    )

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
            owner: "all" as const,
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
    const profileQuery = useAccountProfileQuery()
    const triggerInc = useTriggerIncrementalMutation()
    const triggerSo = useTriggerSingleOrderMutation()
    const retryJob = useRetryJobMutation()
    const confirmMutation = useConfirmMappingMutation()
    const sourceFixMutation = useRequestSourceFixMutation()
    const responsibilityMutation = useWorkItemResponsibilityMutation()
    const reapplyMutation = useReapplyMutation()
    const resolveReapply = useResolveUnknownReapplyMutation()

    const data = pageQuery.data
    const context = data?.context
    const ownership = context?.ownership
    const stage = ownership?.stage ?? "ARCHIVED"
    const firstPhase = stage === "FIRST_PHASE_MALL_OWNED"
    const sealed = stage === "ARCHIVED"

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

    // 切换映射任务时重置候选与动作错误；责任始终从服务端重取。
    React.useEffect(() => {
        setSelectedCandidateId(null)
        setActionError(null)
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

    const sourceFixForm = useAppForm({
        defaultValues: {
            reasonCode: "SOURCE_FIELD_MISSING" as
                | "SOURCE_FIELD_MISSING"
                | "SOURCE_FIELD_CONFLICT"
                | "SOURCE_EVIDENCE_REQUIRED"
                | "OTHER",
            note: "",
            requestedEvidence: "",
        },
        validators: { onChange: sourceFixSchema },
        onSubmit: async ({ value }) => {
            await handleRequestSourceFix(
                value.reasonCode,
                value.note,
                value.requestedEvidence,
            )
        },
    })

    const releaseForm = useAppForm({
        defaultValues: { reason: "" },
        validators: { onChange: releaseSchema },
        onSubmit: async ({ value }) => {
            await handleReleaseToTeam(value.reason)
        },
    })

    const pullForm = useAppForm({
        defaultValues: { externalOrderNo: "", reason: "" },
        validators: { onChange: pullSchema },
        onSubmit: async ({ value }) => {
            const identity = commandIdentity(
                "single-order",
                value.externalOrderNo.trim(),
            )
            const res = await triggerSo.mutateAsync({
                externalOrderNo: value.externalOrderNo,
                reason: value.reason,
                stage,
                idempotencyKey: identity.idempotencyKey,
            })
            if (res.status === "succeeded") {
                commandIdentities.current.delete(identity.key)
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
            const identity = commandIdentity("incremental", "manual")
            const res = await triggerInc.mutateAsync({
                reason: value.reason,
                stage,
                idempotencyKey: identity.idempotencyKey,
            })
            if (res.status === "succeeded") {
                commandIdentities.current.delete(identity.key)
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

    function commandIdentity(kind: string, objectId: string) {
        const key = `${kind}:${objectId}`
        const existing = commandIdentities.current.get(key)
        if (existing) return { key, ...existing }
        const identity = {
            idempotencyKey: `w17:${kind}:${objectId}:${crypto.randomUUID()}`,
            operationId: `w17:${kind}:${crypto.randomUUID()}`,
        }
        commandIdentities.current.set(key, identity)
        return { key, ...identity }
    }

    async function handleStartProcessing() {
        if (mappingTask?.ownerRoutingState !== "CONFIGURED") return
        setActionError(null)
        const identity = commandIdentity(
            "start-processing",
            mappingTask.workItem.workItemId,
        )
        try {
            await responsibilityMutation.mutateAsync({
                kind: "START_PROCESSING",
                workItemId: mappingTask.workItem.workItemId,
                expectedTaskVersion: mappingTask.workItem.taskVersion,
                idempotencyKey: identity.idempotencyKey,
            })
            commandIdentities.current.delete(identity.key)
            await pageQuery.refetch()
        } catch (e) {
            setActionError(getErrorMessage(e, "开始处理失败"))
        }
    }

    async function handleConfirm() {
        if (
            mappingTask?.ownerRoutingState !== "CONFIGURED" ||
            responsibilityStatus !== "assigned_to_me" ||
            !firstPhase
        )
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
        const identity = commandIdentity(
            "confirm-mapping",
            mappingTask.mappingTaskId,
        )
        const res = await confirmMutation.mutateAsync({
            mappingTaskId: mappingTask.mappingTaskId,
            sourceSnapshotId: mappingTask.sourceSnapshotId,
            externalIdentityMapId: mappingTask.externalIdentityMapId,
            workItemId: mappingTask.workItem.workItemId,
            expectedTaskVersion: mappingTask.workItem.taskVersion,
            expectedSubjectVersion: mappingTask.workItem.subjectVersion,
            expectedMappingTaskVersion: mappingTask.lockVersion,
            mappingOperationId: identity.operationId,
            targetObjectType: candidate.objectType,
            targetObjectId: candidate.objectId,
            relationRole: mappingTask.mappingType,
            evidenceNote,
            executionStage: "FIRST_PHASE_MALL_OWNED",
            idempotencyKey: identity.idempotencyKey,
        })
        setConfirmOpen(false)
        if (res.status === "succeeded") {
            commandIdentities.current.delete(identity.key)
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
            void pageQuery.refetch()
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

    async function handleRequestSourceFix(
        reasonCode: string,
        reasonText: string,
        requestedEvidence: string,
    ) {
        if (
            mappingTask?.ownerRoutingState !== "CONFIGURED" ||
            responsibilityStatus !== "assigned_to_me"
        ) {
            setActionError("当前责任不允许提交来源修复说明")
            return
        }
        const identity = commandIdentity(
            "request-source-fix",
            mappingTask.mappingTaskId,
        )
        const res = await sourceFixMutation.mutateAsync({
            mappingTaskId: mappingTask.mappingTaskId,
            sourceSnapshotId: mappingTask.sourceSnapshotId,
            workItemId: mappingTask.workItem.workItemId,
            expectedTaskVersion: mappingTask.workItem.taskVersion,
            expectedSubjectVersion: mappingTask.workItem.subjectVersion,
            expectedMappingTaskVersion: mappingTask.lockVersion,
            requestOperationId: identity.operationId,
            reasonCode,
            reasonText,
            requestedEvidence: requestedEvidence
                .split(/[，,\n]/)
                .map((value) => value.trim())
                .filter(Boolean),
            idempotencyKey: identity.idempotencyKey,
        })
        setSourceFixOpen(false)
        if (res.status === "succeeded") {
            commandIdentities.current.delete(identity.key)
            setResult({
                status: "succeeded",
                title: "来源修复说明已记录",
                description: res.message,
                reference: res.mappingEvidenceEntryId,
            })
            void pageQuery.refetch()
        } else {
            setActionError(res.message)
        }
    }

    async function handleReleaseToTeam(reason: string) {
        if (mappingTask?.ownerRoutingState !== "CONFIGURED") return
        const identity = commandIdentity(
            "release-to-team",
            mappingTask.workItem.workItemId,
        )
        try {
            await responsibilityMutation.mutateAsync({
                kind: "RELEASE_TO_TEAM",
                workItemId: mappingTask.workItem.workItemId,
                expectedTaskVersion: mappingTask.workItem.taskVersion,
                reason,
                idempotencyKey: identity.idempotencyKey,
            })
            commandIdentities.current.delete(identity.key)
            setReleaseOpen(false)
            setResult({
                status: "succeeded",
                title: "已退回团队",
                description:
                    "当前映射仍待处理，个人责任已释放；可继续浏览下一项。",
            })
            await pageQuery.refetch()
        } catch (error) {
            setActionError(getErrorMessage(error, "退回团队失败"))
        }
    }

    async function handleReapply() {
        if (!mappingTask || !firstPhase) return
        const identity = commandIdentity("reapply", mappingTask.mappingTaskId)
        const res = await reapplyMutation.mutateAsync({
            mappingTaskId: mappingTask.mappingTaskId,
            sourceSnapshotId: mappingTask.sourceSnapshotId,
            expectedMappingVersion: mappingTask.lockVersion,
            operationId: identity.operationId,
            executionStage: "FIRST_PHASE_MALL_OWNED",
            idempotencyKey: identity.idempotencyKey,
        })
        if (res.status === "succeeded") {
            commandIdentities.current.delete(identity.key)
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
        const identity = commandIdentity("retry-job", data.selectedJob.jobId)
        const res = await retryJob.mutateAsync({
            jobId: data.selectedJob.jobId,
            reason: "重试未成功部分的分页",
            stage,
            idempotencyKey: identity.idempotencyKey,
        })
        if (res.status === "succeeded") {
            commandIdentities.current.delete(identity.key)
        }
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

    const canManualSync = firstPhase && !context?.sourceUnavailable
    const manualSyncDisabledReason = !firstPhase
        ? "已封存：无第一期写动作"
        : context?.sourceUnavailable
          ? "来源不可用时不新建推进任务（可重试既有失败）"
          : null

    const { diffColumns, jobColumns, mappingColumns, snapshotColumns } =
        useMallSyncColumns({ patchUrl, searchParams })
    const responsibilityStatus: ResponsibilityStatus = (() => {
        if (mappingTask?.ownerRoutingState !== "CONFIGURED") return "blocked"
        const workItem = mappingTask.workItem
        if (workItem.status === "COMPLETED") return "completed"
        if (workItem.status === "CLOSED") return "closed"
        if (workItem.processingState === "APPROVAL_BLOCKED") return "blocked"
        if (workItem.assignmentMode === "POOL" && !workItem.ownerUser) {
            return "pool_available"
        }
        return workItem.ownerUser?.id === profileQuery.data?.userid
            ? "assigned_to_me"
            : "assigned_to_other"
    })()

    const canConfirmMapping =
        firstPhase &&
        mappingTask?.ownerRoutingState === "CONFIGURED" &&
        mappingTask.mappingTaskStatus === "PENDING" &&
        mappingTask.allowedActions.includes("CONFIRM_TARGET") &&
        responsibilityStatus === "assigned_to_me" &&
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
                        disabled={
                            responsibilityStatus !== "assigned_to_me" ||
                            !mappingTask.allowedActions.includes(
                                "REQUEST_SOURCE_FIX",
                            )
                        }
                        onClick={() => setSourceFixOpen(true)}
                    >
                        <PauseIcon className="size-4" />
                        请求来源修复
                    </Button>
                    {mappingTask.workItem.assignmentMode === "POOL" &&
                    mappingTask.workItem.allowedActions.includes(
                        "RELEASE_TO_TEAM",
                    ) ? (
                        <Button
                            type="button"
                            variant="outline"
                            size="sm"
                            disabled={responsibilityStatus !== "assigned_to_me"}
                            onClick={() => setReleaseOpen(true)}
                        >
                            退回团队
                        </Button>
                    ) : null}
                </div>
                {!selectedCandidateId ? (
                    <p className="text-xs text-muted-foreground">
                        请先选择左侧 ERP 候选后即可确认。
                    </p>
                ) : mappingTask.hasConflict ? (
                    <p className="text-xs text-muted-foreground">
                        冲突未解决前确认禁用。
                    </p>
                ) : responsibilityStatus !== "assigned_to_me" ? (
                    <p className="text-xs text-muted-foreground">
                        当前责任人开始处理后才可确认。
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

            <Alert>
                <AlertTitle>人工同步审计边界</AlertTitle>
                <AlertDescription>
                    授权管理员可直接提交带理由的立即增量与按单补拉；服务端重读执行阶段、来源身份与水位，封存后拒绝。
                    {context?.scheduledIncrementalNote}
                </AlertDescription>
            </Alert>

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
                    responsibilityStatus={responsibilityStatus}
                    canConfirmMapping={canConfirmMapping}
                    responsibilityPending={responsibilityMutation.isPending}
                    reapplyPending={reapplyMutation.isPending}
                    onReapply={handleReapply}
                    onResolveUnknownReapply={handleResolveUnknownReapply}
                    onBackToQueue={() =>
                        router.push(
                            `/workspace/tasks?queueContextId=${encodeURIComponent(queueContextId)}`,
                        )
                    }
                    onConfirm={() => confirmForm.handleSubmit()}
                    onStartProcessing={handleStartProcessing}
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
                    {firstPhase ? (
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
                    ) : (
                        <Alert variant="destructive">
                            <AlertTitle>阶段不可用</AlertTitle>
                            <AlertDescription>
                                {manualSyncDisabledReason}
                            </AlertDescription>
                        </Alert>
                    )}
                </DialogContent>
            </Dialog>

            {/* 按单补拉 */}
            <Dialog open={pullOpen} onOpenChange={setPullOpen}>
                <DialogContent>
                    <DialogHeader>
                        <DialogTitle>按单号补拉</DialogTitle>
                        <DialogDescription>
                            使用原来源身份；不创建第二张销售单。仅第一阶段（商城开单）可用。
                        </DialogDescription>
                    </DialogHeader>
                    {!firstPhase ? (
                        <Alert variant="destructive">
                            <AlertTitle>阶段不可用</AlertTitle>
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

            <Dialog open={sourceFixOpen} onOpenChange={setSourceFixOpen}>
                <DialogContent>
                    <DialogHeader>
                        <DialogTitle>请求来源修复</DialogTitle>
                        <DialogDescription>
                            只向当前映射追加说明和证据要求；任务保持待处理，不创建新的协同任务。
                        </DialogDescription>
                    </DialogHeader>
                    <form
                        className="space-y-3"
                        onSubmit={(e) => {
                            e.preventDefault()
                            void sourceFixForm.handleSubmit()
                        }}
                    >
                        <sourceFixForm.AppField
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
                                                        | "SOURCE_FIELD_MISSING"
                                                        | "SOURCE_FIELD_CONFLICT"
                                                        | "SOURCE_EVIDENCE_REQUIRED"
                                                        | "OTHER",
                                                )
                                        }}
                                        options={SOURCE_FIX_REASON_OPTIONS.map(
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
                        <sourceFixForm.AppField
                            name="note"
                            children={(field) => (
                                <field.TextareaField label="修复说明" />
                            )}
                        />
                        <sourceFixForm.AppField
                            name="requestedEvidence"
                            children={(field) => (
                                <field.TextareaField
                                    label="需要补充的来源证据"
                                    placeholder="多项可用逗号或换行分隔"
                                />
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
                            <sourceFixForm.AppForm>
                                <sourceFixForm.SubmitButton label="记录修复要求" />
                            </sourceFixForm.AppForm>
                        </DialogFooter>
                    </form>
                </DialogContent>
            </Dialog>

            <Dialog open={releaseOpen} onOpenChange={setReleaseOpen}>
                <DialogContent>
                    <DialogHeader>
                        <DialogTitle>退回团队</DialogTitle>
                        <DialogDescription>
                            清除当前个人责任，原映射任务保持待处理，不改变映射状态。
                        </DialogDescription>
                    </DialogHeader>
                    <form
                        className="space-y-3"
                        onSubmit={(event) => {
                            event.preventDefault()
                            void releaseForm.handleSubmit()
                        }}
                    >
                        <releaseForm.AppField
                            name="reason"
                            children={(field) => (
                                <field.TextareaField label="退回原因" />
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
                            <releaseForm.AppForm>
                                <releaseForm.SubmitButton label="确认退回团队" />
                            </releaseForm.AppForm>
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
