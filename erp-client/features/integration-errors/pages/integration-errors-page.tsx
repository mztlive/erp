"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import { PauseIcon, RefreshCwIcon, SearchIcon } from "lucide-react"
import {
    BusinessEmptyState,
    BusinessFailureState,
    DataFreshness,
    FormalActionResult,
    InterfaceErrorResolutionPanel,
    ListToolbar,
    MetricFilterItem,
    MetricItem,
    MetricStrip,
    OptionCombobox,
    PageHeader,
    PageScaffold,
    SequentialProcessBar,
    surfacePanelClassName,
    type InterfaceErrorClass,
    type InterfaceErrorStatus,
} from "@/components/business"
import type { ResponsibilityStatus } from "@/components/business/workflow-actions"
import { formatDateTime } from "@/lib/datetime"
import { getErrorMessage } from "@/lib/api/errors"
import { freshnessText } from "@/lib/ui-text"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
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
import { Switch } from "@/components/ui/switch"
import { Textarea } from "@/components/ui/textarea"
import { cn } from "@/lib/utils"

import {
    useDirectReconciliationMutation,
    useIntegrationActionMutation,
    useIntegrationItemQuery,
    useIntegrationQueueQuery,
    useResolveIntegrationMutation,
} from "../hooks/queries"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { useWorkItemResponsibilityMutation } from "@/features/work-items"
import { ReplacementWorkItemSearchCombobox } from "../components/replacement-work-item-search-combobox"
import type {
    IntegrationActionKind,
    IntegrationFormalResult,
    IntegrationResolutionItemView,
    IntegrationView,
} from "../types"
import {
    ENV_LABEL,
    ERROR_CLASS_LABEL,
    MODE_LABEL,
    OWNER_LABEL,
    VIEW_LABEL,
} from "../types"
import {
    buildIntegrationSearchParams,
    parseIntegrationSearchParams,
    toResolutionQuery,
} from "../lib/url-state"
import { IntegrationEvidencePanel } from "../components/integration-evidence-panel"
import { IntegrationItemSummary } from "../components/integration-item-summary"
import { IntegrationQueuePanel } from "../components/integration-queue-panel"
import {
    TerminalActionDialog,
    type TerminalConfirm,
} from "../components/terminal-action-dialog"
import { INTEGRATION_ACTION_LABEL } from "../lib/presentation"

function newKey(prefix: string) {
    return `${prefix}:${crypto.randomUUID()}`
}

function mapPanelStatus(
    item: IntegrationResolutionItemView,
): InterfaceErrorStatus {
    if (item.status.code === "AUTO_RETRYING") return "auto-retrying"
    if (
        item.status.label.includes("人工") ||
        item.status.code === "MANUAL_REQUIRED"
    )
        return "manual-required"
    if (
        item.status.code === "COMPLETED" ||
        item.status.label.includes("已解决")
    )
        return "resolved"
    if (item.status.code === "CLOSED" || item.status.label.includes("关闭"))
        return "closed"
    return "pending"
}

function isPanelErrorClass(
    c: IntegrationResolutionItemView["classification"]["errorClass"],
): c is InterfaceErrorClass {
    return c !== "reconciliation-difference"
}

function formalStatus(
    s: IntegrationFormalResult["status"],
): "succeeded" | "blocked" | "rejected" | "unknown" | "processing" {
    if (s === "failed") return "rejected"
    return s
}

export function IntegrationErrorsPage({
    forcedTaskId,
    forcedDifferenceId,
}: {
    forcedTaskId?: string
    forcedDifferenceId?: string
} = {}) {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()
    const urlState = React.useMemo(
        () => parseIntegrationSearchParams(searchParams),
        [searchParams],
    )

    const currentTaskId = forcedTaskId ?? urlState.currentTaskId
    const currentDifferenceId =
        forcedDifferenceId ?? urlState.currentDifferenceId
    const focusMode = Boolean(forcedTaskId || forcedDifferenceId)

    const query = React.useMemo(
        () =>
            toResolutionQuery({
                ...urlState,
                currentTaskId,
                currentDifferenceId,
            }),
        [urlState, currentTaskId, currentDifferenceId],
    )

    const queueQuery = useIntegrationQueueQuery(query)
    const profileQuery = useAccountProfileQuery()
    const responsibilityMutation = useWorkItemResponsibilityMutation()
    const actionMutation = useIntegrationActionMutation()
    const resolveMutation = useResolveIntegrationMutation()
    const directMutation = useDirectReconciliationMutation()

    const view = queueQuery.data
    const queueItems = React.useMemo(() => view?.items ?? [], [view?.items])
    const metrics = view?.metrics

    const queueSelection = React.useMemo(() => {
        if (currentTaskId) {
            return (
                queueItems.find(
                    (candidate) =>
                        candidate.identity.itemType === "ERROR_TASK" &&
                        candidate.identity.id === currentTaskId,
                ) ??
                queueItems.find(
                    (candidate) => candidate.identity.id === currentTaskId,
                )
            )
        }
        if (currentDifferenceId) {
            return queueItems.find(
                (candidate) =>
                    candidate.identity.itemType ===
                        "RECONCILIATION_DIFFERENCE" &&
                    candidate.identity.id === currentDifferenceId,
            )
        }
        return queueItems[0]
    }, [currentDifferenceId, currentTaskId, queueItems])

    const detailTarget = React.useMemo(
        () =>
            forcedTaskId
                ? { itemType: "ERROR_TASK" as const, id: forcedTaskId }
                : forcedDifferenceId
                  ? {
                        itemType: "RECONCILIATION_DIFFERENCE" as const,
                        id: forcedDifferenceId,
                    }
                  : queueSelection
                    ? {
                          itemType: queueSelection.identity.itemType,
                          id: queueSelection.identity.id,
                      }
                    : null,
        [forcedDifferenceId, forcedTaskId, queueSelection],
    )

    const detailItemQuery = useIntegrationItemQuery({
        itemType: detailTarget?.itemType ?? "ERROR_TASK",
        id: detailTarget?.id ?? "",
        enabled: detailTarget !== null,
    })

    const item: IntegrationResolutionItemView | undefined =
        React.useMemo(() => {
            if (
                detailTarget &&
                detailItemQuery.data?.identity.itemType ===
                    detailTarget.itemType &&
                detailItemQuery.data.identity.id === detailTarget.id
            ) {
                return detailItemQuery.data
            }
            return queueSelection
        }, [detailTarget, detailItemQuery.data, queueSelection])

    const items = React.useMemo(
        () => (focusMode && item ? [item] : queueItems),
        [focusMode, item, queueItems],
    )

    const currentIndex = item
        ? Math.max(
              0,
              items.findIndex((i) => i.identity.id === item.identity.id),
          )
        : 0

    const queueIndex = item
        ? queueItems.findIndex((i) => i.identity.id === item.identity.id)
        : -1
    const positionIndex = focusMode
        ? queueIndex >= 0
            ? queueIndex + 1
            : 1
        : currentIndex + 1
    const positionTotal = focusMode
        ? queueIndex >= 0
            ? queueItems.length
            : 1
        : items.length

    const [lastResult, setLastResult] =
        React.useState<IntegrationFormalResult | null>(null)
    const [actionError, setActionError] = React.useState<string | null>(null)
    const [searchDraft, setSearchDraft] = React.useState(urlState.q ?? "")
    const searchInputRef = React.useRef<HTMLInputElement | null>(null)
    const [replacementTaskId, setReplacementTaskId] = React.useState("")
    const [reconReasonId, setReconReasonId] = React.useState("")
    const [comment, setComment] = React.useState("")
    const [terminalConfirm, setTerminalConfirm] =
        React.useState<TerminalConfirm | null>(null)

    const commandIdentities = React.useRef(
        new Map<string, { idempotencyKey: string; operationId: string }>(),
    )
    const resultRef = React.useRef<HTMLDivElement>(null)
    const headingRef = React.useRef<HTMLHeadingElement>(null)
    const actionZoneRef = React.useRef<HTMLDivElement>(null)

    const focusFirstAction = React.useCallback(() => {
        actionZoneRef.current?.scrollIntoView({
            behavior: "smooth",
            block: "start",
        })
        window.setTimeout(() => {
            const zone = actionZoneRef.current
            const btn = zone?.querySelector<HTMLButtonElement>(
                "button:not([disabled])",
            )
            if (btn) {
                btn.focus()
            } else {
                headingRef.current?.focus()
            }
        }, 250)
    }, [])

    const autoNext = urlState.autoNext

    const replaceUrl = React.useCallback(
        (patch: Record<string, string | null | undefined>) => {
            if (focusMode && (forcedTaskId || forcedDifferenceId)) {
                // detail routes keep path; still allow query prefs
                const params = new URLSearchParams(searchParams.toString())
                for (const [k, v] of Object.entries(patch)) {
                    if (
                        k === "taskId" ||
                        k === "differenceId" ||
                        k === "currentTaskId" ||
                        k === "currentDifferenceId"
                    )
                        continue
                    if (v == null || v === "") params.delete(k)
                    else params.set(k, v)
                }
                const qs = params.toString()
                router.replace(qs ? `${pathname}?${qs}` : pathname, {
                    scroll: false,
                })
                return
            }
            const base = parseIntegrationSearchParams(searchParams)
            const next = {
                ...base,
                view: (patch.view as IntegrationView | undefined) ?? base.view,
                mode: (patch.mode as typeof base.mode | undefined) ?? base.mode,
                environment:
                    (patch.environment as
                        | typeof base.environment
                        | undefined) ?? base.environment,
                errorClass:
                    patch.errorClass === null
                        ? undefined
                        : (patch.errorClass ?? base.errorClass),
                owner:
                    (patch.owner as typeof base.owner | undefined) ??
                    base.owner,
                q: patch.q === null ? undefined : (patch.q ?? base.q),
                queueContextId: patch.queueContextId ?? base.queueContextId,
                resolveWorkItemId:
                    patch.resolveWorkItemId === null
                        ? undefined
                        : (patch.resolveWorkItemId ?? base.resolveWorkItemId),
                currentTaskId:
                    patch.taskId === null
                        ? undefined
                        : (patch.taskId ?? base.currentTaskId),
                currentDifferenceId:
                    patch.differenceId === null
                        ? undefined
                        : (patch.differenceId ?? base.currentDifferenceId),
                autoNext:
                    patch.autoNext === "0"
                        ? false
                        : patch.autoNext === "1"
                          ? true
                          : base.autoNext,
            }
            const params = buildIntegrationSearchParams(next)
            // clear resolve after apply
            if (patch.resolveWorkItemId === null) {
                params.delete("resolveWorkItemId")
            }
            const qs = params.toString()
            router.replace(
                qs
                    ? `/governance/integration-errors?${qs}`
                    : "/governance/integration-errors",
                { scroll: false },
            )
        },
        [
            focusMode,
            forcedDifferenceId,
            forcedTaskId,
            pathname,
            router,
            searchParams,
        ],
    )

    // resolveWorkItemId → domain detail replace
    React.useEffect(() => {
        if (!view?.resolvedEntry) return
        if (!urlState.resolveWorkItemId) return
        const entry = view.resolvedEntry
        if (entry.itemType === "ERROR_TASK") {
            router.replace(
                `/governance/integration-errors/errors/${entry.id}?queueContextId=${encodeURIComponent(urlState.queueContextId)}&view=${urlState.view}&autoNext=${autoNext ? "1" : "0"}`,
            )
        } else {
            router.replace(
                `/governance/integration-errors/differences/${entry.id}?queueContextId=${encodeURIComponent(urlState.queueContextId)}&view=${urlState.view}&autoNext=${autoNext ? "1" : "0"}`,
            )
        }
    }, [
        view?.resolvedEntry,
        urlState.resolveWorkItemId,
        urlState.queueContextId,
        urlState.view,
        autoNext,
        router,
    ])

    // URL defaults for current item
    React.useEffect(() => {
        if (queueQuery.isPending || !view || focusMode) return
        if (urlState.resolveWorkItemId) return
        const hasTask = searchParams.has("taskId")
        const hasDiff = searchParams.has("differenceId")
        const hasView = searchParams.has("view")
        const hasCtx = searchParams.has("queueContextId")
        if (hasView && hasCtx && (hasTask || hasDiff || items.length === 0))
            return
        const params = buildIntegrationSearchParams({
            ...urlState,
            currentTaskId:
                item?.identity.itemType === "ERROR_TASK"
                    ? item.identity.id
                    : urlState.currentTaskId,
            currentDifferenceId:
                item?.identity.itemType === "RECONCILIATION_DIFFERENCE"
                    ? item.identity.id
                    : urlState.currentDifferenceId,
        })
        router.replace(`/governance/integration-errors?${params.toString()}`, {
            scroll: false,
        })
    }, [
        queueQuery.isPending,
        view,
        focusMode,
        urlState,
        item,
        items.length,
        searchParams,
        router,
    ])

    // Reset UI on item switch
    const firstReasonId =
        item?.reconciliationReasonRegistry?.registeredReasons[0]
            ?.registeredReasonId
    React.useEffect(() => {
        setActionError(null)
        setComment("")
        setReplacementTaskId("")
        setReconReasonId(firstReasonId ?? "")
    }, [item?.identity.id, firstReasonId])

    // URL 搜索变化时回写输入框（浏览器前进/后退同步）
    React.useEffect(() => {
        setSearchDraft(urlState.q ?? "")
    }, [urlState.q])

    // P3 搜索：300ms 防抖写 URL，Enter 兜底，/ 聚焦
    React.useEffect(() => {
        const handle = globalThis.setTimeout(() => {
            if (searchDraft.trim() === (urlState.q ?? "")) return
            replaceUrl({
                q: searchDraft.trim() || null,
                taskId: null,
                differenceId: null,
            })
        }, 300)
        return () => globalThis.clearTimeout(handle)
        // eslint-disable-next-line react-hooks/exhaustive-deps -- replaceUrl 以当前 URL 快照为准
    }, [searchDraft])

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

    React.useEffect(() => {
        if (lastResult) resultRef.current?.focus()
        else if (item) headingRef.current?.focus()
    }, [item, lastResult])

    const goToItem = React.useCallback(
        (next: IntegrationResolutionItemView | null | undefined) => {
            setLastResult(null)
            setActionError(null)
            if (!next) {
                replaceUrl({ taskId: null, differenceId: null })
                return
            }
            if (next.identity.itemType === "ERROR_TASK") {
                replaceUrl({
                    taskId: next.identity.id,
                    differenceId: null,
                })
            } else {
                replaceUrl({
                    differenceId: next.identity.id,
                    taskId: null,
                })
            }
        },
        [replaceUrl],
    )

    const neighbor = React.useCallback(
        (delta: number) => {
            const idx = currentIndex + delta
            if (idx < 0 || idx >= items.length) return null
            return items[idx] ?? null
        },
        [currentIndex, items],
    )

    const afterResult = React.useCallback(
        (result: IntegrationFormalResult) => {
            setLastResult(result)
            // 详情模式（focusMode）无队列导航控件，autoNext 不得静默自动跳转；
            // 自动下一项仅在带队列的列表模式生效，避免 URL 隐形状态驱动用户预期外的跳转。
            if (
                !focusMode &&
                result.terminal &&
                !result.stayOnItem &&
                autoNext &&
                result.status === "succeeded"
            ) {
                const next = neighbor(1) ?? neighbor(-1)
                if (next) {
                    window.setTimeout(() => goToItem(next), 400)
                }
            }
        },
        [autoNext, focusMode, goToItem, neighbor],
    )

    function commandIdentity(kind: string, objectId: string) {
        const key = `${kind}:${objectId}`
        const existing = commandIdentities.current.get(key)
        if (existing) return { key, ...existing }
        const identity = {
            idempotencyKey: newKey(`w29:${kind}:${objectId}`),
            operationId: newKey(`w29:${kind}`),
        }
        commandIdentities.current.set(key, identity)
        return { key, ...identity }
    }

    const responsibilityStatus: ResponsibilityStatus = (() => {
        if (!item?.workItem) {
            return item?.identity.itemType === "RECONCILIATION_DIFFERENCE"
                ? "assigned_to_me"
                : "blocked"
        }
        const workItem = item.workItem
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

    async function handleStartProcessing() {
        if (!item?.workItem) return
        const identity = commandIdentity(
            "start-processing",
            item.workItem.workItemId,
        )
        try {
            await responsibilityMutation.mutateAsync({
                kind: "START_PROCESSING",
                workItemId: item.workItem.workItemId,
                expectedTaskVersion: item.workItem.taskVersion,
                idempotencyKey: identity.idempotencyKey,
            })
            commandIdentities.current.delete(identity.key)
            await returnRefresh()
        } catch (error) {
            setActionError(getErrorMessage(error, "开始处理失败"))
        }
    }

    async function handleReleaseToTeam() {
        if (!item?.workItem || !comment.trim()) {
            setActionError("请先填写退回原因")
            return
        }
        const identity = commandIdentity(
            "release-to-team",
            item.workItem.workItemId,
        )
        try {
            await responsibilityMutation.mutateAsync({
                kind: "RELEASE_TO_TEAM",
                workItemId: item.workItem.workItemId,
                expectedTaskVersion: item.workItem.taskVersion,
                reason: comment.trim(),
                idempotencyKey: identity.idempotencyKey,
            })
            commandIdentities.current.delete(identity.key)
            setLastResult({
                status: "succeeded",
                title: "已退回团队",
                description:
                    "当前事项仍待处理，个人责任已释放；可继续浏览下一项。",
                workItemStatus: "OPEN",
                stayOnItem: false,
                terminal: false,
            })
            await returnRefresh()
        } catch (error) {
            setActionError(getErrorMessage(error, "退回团队失败"))
        }
    }

    const can = (action: IntegrationActionKind) =>
        Boolean(item?.allowedActions.includes(action))

    const runTaskAction = async (
        kind:
            | "QUERY_ORIGINAL_RESULT"
            | "REPLAY_ORIGINAL"
            | "REATTRIBUTE"
            | "LINK_COMPENSATION"
            | "ADD_EVIDENCE",
    ) => {
        if (
            !item?.workItem ||
            responsibilityStatus !== "assigned_to_me" ||
            !can(kind)
        )
            return
        const evidenceRefs =
            kind === "ADD_EVIDENCE" || kind === "LINK_COMPENSATION"
                ? item.linkedEvidence
                : undefined
        if (
            (kind === "ADD_EVIDENCE" || kind === "LINK_COMPENSATION") &&
            evidenceRefs?.length === 0
        ) {
            setActionError("请先从受控证据入口关联已有证据")
            return
        }
        const identity = commandIdentity(kind, item.identity.id)
        try {
            const result = await actionMutation.mutateAsync({
                itemType: item.identity.itemType,
                itemId: item.identity.id,
                workItemId: item.workItem.workItemId,
                expectedSubjectVersion: item.workItem.subjectVersion,
                expectedTaskVersion: item.workItem.taskVersion,
                kind,
                operationId: identity.operationId,
                idempotencyKey: identity.idempotencyKey,
                comment: comment || undefined,
                evidenceRefs,
            })
            if (result.status === "succeeded") {
                commandIdentities.current.delete(identity.key)
            }
            afterResult(result)
        } catch (e) {
            setActionError(getErrorMessage(e, "动作失败"))
        }
    }

    const formalPending =
        actionMutation.isPending ||
        resolveMutation.isPending ||
        directMutation.isPending ||
        responsibilityMutation.isPending

    const reconReason =
        item?.reconciliationReasonRegistry?.registeredReasons.find(
            (r) => r.registeredReasonId === reconReasonId,
        )
    const reasonMismatches = (
        conclusion: "CONFIRM_NO_ERROR" | "CONFIRM_VALID_DIFFERENCE",
    ) => !reconReason || reconReason.conclusion !== conclusion

    const returnRefresh = () => {
        void queueQuery.refetch()
        if (focusMode) void detailItemQuery.refetch()
        setLastResult(null)
    }

    const hasQueueFilters = Boolean(
        urlState.mode !== "all" ||
        urlState.environment !== "production" ||
        urlState.errorClass ||
        urlState.owner !== "me" ||
        urlState.q,
    )

    const clearQueueFilters = React.useCallback(() => {
        setSearchDraft("")
        replaceUrl({
            mode: "all",
            environment: "production",
            errorClass: null,
            owner: "me",
            q: null,
            taskId: null,
            differenceId: null,
        })
    }, [replaceUrl])

    const focusLoading =
        focusMode && detailItemQuery.isPending && !detailItemQuery.data
    const focusError =
        focusMode &&
        (detailItemQuery.isError ||
            (!detailItemQuery.isPending && detailItemQuery.data === null))

    if (queueQuery.isPending && !focusMode) {
        return (
            <PageScaffold>
                <div className="h-10 w-56 animate-pulse rounded-lg bg-muted" />
                <div className="h-16 animate-pulse rounded-lg bg-muted" />
                <div className="grid gap-4 xl:grid-cols-[minmax(0,38fr)_minmax(0,62fr)]">
                    <div className="h-80 animate-pulse rounded-lg bg-muted" />
                    <div className="h-80 animate-pulse rounded-lg bg-muted" />
                </div>
            </PageScaffold>
        )
    }

    if (focusLoading) {
        return (
            <PageScaffold>
                <div className="h-10 w-56 animate-pulse rounded-lg bg-muted" />
                <div className="h-16 animate-pulse rounded-lg bg-muted" />
                <div className="h-80 animate-pulse rounded-lg bg-muted" />
            </PageScaffold>
        )
    }

    if (queueQuery.isError && !focusMode) {
        return (
            <PageScaffold>
                <PageHeader title="接口错误与对账中心" description="加载失败" />
                <BusinessFailureState
                    error={queueQuery.error}
                    onRetry={() => void queueQuery.refetch()}
                />
            </PageScaffold>
        )
    }

    if (focusMode && detailItemQuery.isError) {
        return (
            <PageScaffold>
                <PageHeader
                    title="接口错误与对账中心"
                    description="任务加载失败"
                />
                <BusinessFailureState
                    error={detailItemQuery.error}
                    onRetry={() => void detailItemQuery.refetch()}
                />
            </PageScaffold>
        )
    }

    if (focusError) {
        return (
            <PageScaffold>
                <PageHeader
                    title="接口错误与对账中心"
                    description="未找到该任务"
                />
                <BusinessEmptyState
                    kind="no-data"
                    title="未找到该任务或差异"
                    description="任务可能已结束或链接失效；可返回队列重新选择。"
                    className="rounded-lg border-0 bg-transparent shadow-none ring-0"
                    action={
                        <div className="flex flex-wrap gap-2">
                            <Button
                                type="button"
                                variant="secondary"
                                className="rounded-lg shadow-none"
                                onClick={() => void detailItemQuery.refetch()}
                            >
                                重试
                            </Button>
                            <Button
                                type="button"
                                variant="secondary"
                                className="rounded-lg shadow-none"
                                render={
                                    <Link
                                        href={`/governance/integration-errors?view=${urlState.view}&queueContextId=${encodeURIComponent(urlState.queueContextId)}`}
                                    />
                                }
                            >
                                返回队列
                            </Button>
                        </div>
                    }
                />
            </PageScaffold>
        )
    }

    const panelErrorClass =
        item && isPanelErrorClass(item.classification.errorClass)
            ? item.classification.errorClass
            : null

    async function handleClose(kind: "CLOSE_DUPLICATE" | "CLOSE_MISROUTED") {
        if (!item?.workItem) return
        if (kind === "CLOSE_DUPLICATE" && !replacementTaskId) {
            setActionError("请先选择替代任务")
            throw new Error("请先选择替代任务")
        }
        const identity = commandIdentity(kind, item.workItem.workItemId)
        try {
            await responsibilityMutation.mutateAsync({
                kind: "CLOSE",
                workItemId: item.workItem.workItemId,
                expectedTaskVersion: item.workItem.taskVersion,
                reasonCode:
                    kind === "CLOSE_DUPLICATE" ? "DUPLICATE" : "MISROUTED",
                replacementWorkItemId:
                    kind === "CLOSE_DUPLICATE" ? replacementTaskId : undefined,
                comment: comment || undefined,
                idempotencyKey: identity.idempotencyKey,
            })
            commandIdentities.current.delete(identity.key)
            afterResult({
                status: "succeeded",
                title:
                    kind === "CLOSE_DUPLICATE"
                        ? "已关闭重复任务"
                        : "已关闭误派",
                description: "仅关闭当前处理任务；未写入业务解决结论。",
                workItemStatus: "CLOSED",
                stayOnItem: false,
                terminal: true,
                replacementWorkItemId:
                    kind === "CLOSE_DUPLICATE" ? replacementTaskId : undefined,
            })
        } catch (e) {
            setActionError(getErrorMessage(e, "关闭失败"))
            throw e
        }
    }

    async function handleResolve() {
        if (
            !item?.workItem ||
            !item.resolutionEvidencePolicy ||
            responsibilityStatus !== "assigned_to_me" ||
            !can("RESOLVE")
        )
            return
        const evidence = item.linkedEvidence
        const kinds = new Set(evidence.map((entry) => entry.kind))
        if (
            item.resolutionEvidencePolicy.requiredEvidenceKinds.some(
                (kind) => !kinds.has(kind),
            )
        ) {
            setActionError("完成凭证尚未齐备，请先从证据入口完成关联")
            return
        }
        const identity = commandIdentity("resolve", item.identity.id)
        try {
            const result = await resolveMutation.mutateAsync({
                itemType: item.identity.itemType,
                itemId: item.identity.id,
                workItemId: item.workItem.workItemId,
                expectedSubjectVersion: item.workItem.subjectVersion,
                expectedTaskVersion: item.workItem.taskVersion,
                operationId: identity.operationId,
                idempotencyKey: identity.idempotencyKey,
                reasonCode: "TERMINAL_EVIDENCE_VERIFIED",
                evidencePolicyId:
                    item.resolutionEvidencePolicy.evidencePolicyId,
                evidencePolicyVersion:
                    item.resolutionEvidencePolicy.evidencePolicyVersion,
                policyKey: item.resolutionEvidencePolicy.key,
                evidenceRefs: evidence,
                comment: comment || undefined,
            })
            commandIdentities.current.delete(identity.key)
            afterResult(result)
        } catch (e) {
            setActionError(getErrorMessage(e, "解决失败"))
            throw e
        }
    }

    async function handleDirectTerminal(
        conclusion: "CONFIRM_NO_ERROR" | "CONFIRM_VALID_DIFFERENCE",
    ) {
        if (
            !item ||
            item.hasWorkItem ||
            item.identity.itemType !== "RECONCILIATION_DIFFERENCE" ||
            !can(conclusion)
        )
            return
        const reg = item.reconciliationReasonRegistry
        if (!reg) return
        const reason = reg.registeredReasons.find(
            (r) => r.registeredReasonId === reconReasonId,
        )
        if (!reason || reason.conclusion !== conclusion) {
            setActionError("请选择与结论匹配的注册原因")
            return
        }
        const evidence = item.linkedEvidence
        const evidenceKinds = new Set(evidence.map((entry) => entry.kind))
        if (
            reason.requiredEvidenceKinds.some(
                (kind) => !evidenceKinds.has(kind),
            )
        ) {
            setActionError("结论所需证据尚未齐备")
            return
        }
        const identity = commandIdentity(conclusion, item.identity.id)
        try {
            const result = await directMutation.mutateAsync({
                differenceId: item.identity.id,
                expectedDifferenceVersion: item.objectVersion,
                operationId: identity.operationId,
                idempotencyKey: identity.idempotencyKey,
                decision: {
                    kind: "TERMINAL_CONCLUSION",
                    reasonCode: reason.registeredReasonId,
                    reasonRegistryId: reg.reasonRegistryId,
                    reasonRegistryVersion: reg.reasonRegistryVersion,
                    registeredReasonId: reason.registeredReasonId,
                    conclusion,
                    evidenceRefs: evidence,
                    comment: comment || undefined,
                },
            })
            commandIdentities.current.delete(identity.key)
            afterResult(result)
        } catch (e) {
            setActionError(getErrorMessage(e, "对账确认失败"))
            throw e
        }
    }

    async function handleDirectAction(
        kind:
            | "QUERY_ORIGINAL_RESULT"
            | "REPLAY_ORIGINAL"
            | "REATTRIBUTE"
            | "LINK_COMPENSATION"
            | "ADD_EVIDENCE",
    ) {
        const needsEvidence =
            kind === "ADD_EVIDENCE" || kind === "LINK_COMPENSATION"
        if (
            !item ||
            item.hasWorkItem ||
            item.identity.itemType !== "RECONCILIATION_DIFFERENCE" ||
            !can(kind) ||
            (needsEvidence && item.linkedEvidence.length === 0)
        ) {
            if (needsEvidence && item?.linkedEvidence.length === 0) {
                setActionError("请先从受控证据入口关联已有证据")
            }
            return
        }
        const identity = commandIdentity(`direct-${kind}`, item.identity.id)
        try {
            const result = await directMutation.mutateAsync({
                differenceId: item.identity.id,
                expectedDifferenceVersion: item.objectVersion,
                operationId: identity.operationId,
                idempotencyKey: identity.idempotencyKey,
                decision: {
                    kind: "NON_TERMINAL_ACTION",
                    action: kind,
                    evidenceRefs: needsEvidence
                        ? item.linkedEvidence
                        : undefined,
                    comment: comment || undefined,
                },
            })
            if (result.status === "succeeded") {
                commandIdentities.current.delete(identity.key)
            }
            afterResult(result)
        } catch (error) {
            setActionError(
                getErrorMessage(
                    error,
                    `${INTEGRATION_ACTION_LABEL[kind] ?? kind}失败`,
                ),
            )
        }
    }

    return (
        <PageScaffold>
            <PageHeader
                title={
                    focusMode
                        ? (item?.identity.number ?? "接口错误与对账中心")
                        : "接口错误与对账中心"
                }
                breadcrumbs={[
                    {
                        id: "gov",
                        label: "治理",
                        href: "/governance/integration-errors",
                    },
                    {
                        id: "ie",
                        label: focusMode
                            ? (item?.identity.number ?? "详情")
                            : "接口错误与对账",
                        current: true,
                    },
                ]}
                metadata={
                    <DataFreshness
                        state="fresh"
                        label={freshnessText.dataUpdatedAt}
                        updatedAt={formatDateTime(
                            view?.context.updatedAt,
                            "default",
                        )}
                        dateTime={view?.context.updatedAt}
                    />
                }
            />

            {metrics ? (
                <MetricStrip>
                    <MetricFilterItem
                        label="结果未知"
                        value={metrics.resultUnknown}
                        active={urlState.view === "result_unknown"}
                        onClick={
                            focusMode
                                ? undefined
                                : () =>
                                      replaceUrl({
                                          view: "result_unknown",
                                          taskId: null,
                                          differenceId: null,
                                      })
                        }
                    />
                    <MetricItem label="待人工" value={metrics.manualRequired} />
                    <MetricFilterItem
                        label="安全故障"
                        value={metrics.securityFaults}
                        active={urlState.view === "security"}
                        onClick={
                            focusMode
                                ? undefined
                                : () =>
                                      replaceUrl({
                                          view: "security",
                                          taskId: null,
                                          differenceId: null,
                                      })
                        }
                    />
                    <MetricFilterItem
                        label="未解决差异"
                        value={metrics.openDifferences}
                        active={urlState.view === "reconciliation"}
                        onClick={
                            focusMode
                                ? undefined
                                : () =>
                                      replaceUrl({
                                          view: "reconciliation",
                                          taskId: null,
                                          differenceId: null,
                                      })
                        }
                    />
                    <MetricItem
                        label="最长滞留"
                        value={metrics.longestAgeLabel}
                    />
                </MetricStrip>
            ) : null}

            {!focusMode ? (
                <div
                    className={cn(
                        surfacePanelClassName,
                        "sticky top-0 z-10 space-y-2.5 px-3 py-2.5",
                    )}
                >
                    <div className="flex flex-wrap items-center gap-2">
                        <OptionCombobox
                            value={urlState.view}
                            onValueChange={(v) =>
                                replaceUrl({
                                    view:
                                        (v as IntegrationView | null) ?? "mine",
                                    taskId: null,
                                    differenceId: null,
                                })
                            }
                            options={(
                                Object.keys(VIEW_LABEL) as IntegrationView[]
                            ).map((v) => ({ value: v, label: VIEW_LABEL[v] }))}
                            allowClear={false}
                            size="sm"
                            aria-label="队列视图"
                            inputClassName="w-[9.5rem]"
                        />
                    </div>
                    <ListToolbar
                        aria-label="队列筛选"
                        search={
                            <form
                                onSubmit={(e) => {
                                    e.preventDefault()
                                    replaceUrl({
                                        q: searchDraft.trim() || null,
                                        taskId: null,
                                        differenceId: null,
                                    })
                                }}
                            >
                                <InputGroup>
                                    <InputGroupAddon>
                                        <SearchIcon aria-hidden="true" />
                                    </InputGroupAddon>
                                    <InputGroupInput
                                        ref={searchInputRef}
                                        value={searchDraft}
                                        onChange={(e) =>
                                            setSearchDraft(e.target.value)
                                        }
                                        placeholder="任务号 / 业务单号 / 事件摘要"
                                        aria-label="搜索"
                                    />
                                </InputGroup>
                            </form>
                        }
                        filters={
                            <>
                                <OptionCombobox
                                    value={urlState.mode}
                                    onValueChange={(v) =>
                                        replaceUrl({
                                            mode: v ?? "all",
                                            taskId: null,
                                            differenceId: null,
                                        })
                                    }
                                    options={(
                                        Object.keys(
                                            MODE_LABEL,
                                        ) as (keyof typeof MODE_LABEL)[]
                                    ).map((m) => ({
                                        value: m,
                                        label: MODE_LABEL[m],
                                    }))}
                                    inputClassName="w-[8rem]"
                                    size="sm"
                                    aria-label="模式"
                                    allowClear={false}
                                />
                                <OptionCombobox
                                    value={urlState.environment}
                                    onValueChange={(v) =>
                                        replaceUrl({
                                            environment: v ?? "production",
                                            taskId: null,
                                            differenceId: null,
                                        })
                                    }
                                    options={(
                                        Object.keys(
                                            ENV_LABEL,
                                        ) as (keyof typeof ENV_LABEL)[]
                                    ).map((e) => ({
                                        value: e,
                                        label: ENV_LABEL[e],
                                    }))}
                                    inputClassName="w-[7rem]"
                                    size="sm"
                                    aria-label="环境"
                                    allowClear={false}
                                />
                                <OptionCombobox
                                    value={urlState.errorClass ?? "all"}
                                    onValueChange={(v) =>
                                        replaceUrl({
                                            errorClass:
                                                !v || v === "all" ? null : v,
                                            taskId: null,
                                            differenceId: null,
                                        })
                                    }
                                    options={[
                                        { value: "all", label: "全部类别" },
                                        ...Object.entries(
                                            ERROR_CLASS_LABEL,
                                        ).map(([k, label]) => ({
                                            value: k,
                                            label,
                                        })),
                                    ]}
                                    inputClassName="w-[10rem]"
                                    size="sm"
                                    aria-label="错误类别"
                                    placeholder="错误类别"
                                    allowClear={false}
                                />
                            </>
                        }
                        secondary={
                            <OptionCombobox
                                value={urlState.owner}
                                onValueChange={(v) =>
                                    replaceUrl({
                                        owner: v ?? "me",
                                        taskId: null,
                                        differenceId: null,
                                    })
                                }
                                options={(
                                    Object.keys(
                                        OWNER_LABEL,
                                    ) as (keyof typeof OWNER_LABEL)[]
                                ).map((o) => ({
                                    value: o,
                                    label: OWNER_LABEL[o],
                                }))}
                                inputClassName="w-[8rem]"
                                size="sm"
                                aria-label="责任人"
                                allowClear={false}
                            />
                        }
                        actions={
                            <>
                                {hasQueueFilters ? (
                                    <Button
                                        type="button"
                                        size="sm"
                                        variant="ghost"
                                        onClick={clearQueueFilters}
                                    >
                                        清除筛选
                                    </Button>
                                ) : null}
                                <div className="flex items-center gap-2">
                                    <Label
                                        htmlFor="w29-auto-next"
                                        className="text-xs text-muted-foreground"
                                    >
                                        自动下一项
                                    </Label>
                                    <Switch
                                        id="w29-auto-next"
                                        checked={autoNext}
                                        onCheckedChange={(on) => {
                                            replaceUrl({
                                                autoNext: on ? "1" : "0",
                                            })
                                        }}
                                    />
                                </div>
                            </>
                        }
                    />
                </div>
            ) : (
                <div className="flex flex-wrap gap-2">
                    <Button
                        type="button"
                        size="sm"
                        variant="ghost"
                        render={
                            <Link
                                href={`/governance/integration-errors?view=${urlState.view}&queueContextId=${encodeURIComponent(urlState.queueContextId)}`}
                            />
                        }
                    >
                        返回队列
                    </Button>
                    <Button
                        type="button"
                        size="sm"
                        variant="ghost"
                        className="text-muted-foreground"
                        onClick={returnRefresh}
                    >
                        <RefreshCwIcon data-icon="inline-start" aria-hidden />
                        刷新当前任务
                    </Button>
                </div>
            )}

            {!focusMode ? (
                <p className="text-xs text-muted-foreground">
                    筛选：{view?.context.filterSummary}
                </p>
            ) : null}

            {lastResult ? (
                <div ref={resultRef} tabIndex={-1} className="outline-none">
                    <FormalActionResult
                        status={formalStatus(lastResult.status)}
                        title={lastResult.title}
                        description={lastResult.description}
                        reference={lastResult.reference}
                        referenceLabel="本次处理编号"
                        facts={lastResult.facts}
                        actions={
                            lastResult.terminal && !autoNext ? (
                                <Button
                                    type="button"
                                    size="sm"
                                    onClick={() => {
                                        const next = neighbor(1)
                                        if (next) goToItem(next)
                                    }}
                                >
                                    下一项
                                </Button>
                            ) : null
                        }
                    />
                </div>
            ) : null}

            {actionError ? (
                <Alert variant="destructive">
                    <AlertTitle>操作失败</AlertTitle>
                    <AlertDescription>{actionError}</AlertDescription>
                </Alert>
            ) : null}

            {items.length === 0 ? (
                <BusinessEmptyState
                    kind="filter"
                    title="当前筛选项已处理完"
                    description="可切换视图、清除筛选，或返回工作台。"
                    className="rounded-lg border-0 bg-transparent shadow-none ring-0"
                    action={
                        <Button
                            type="button"
                            size="sm"
                            variant="secondary"
                            className="rounded-lg shadow-none"
                            onClick={clearQueueFilters}
                        >
                            清除筛选
                        </Button>
                    }
                />
            ) : (
                <div
                    className={cn(
                        "grid gap-4",
                        focusMode
                            ? "grid-cols-1"
                            : "xl:grid-cols-[minmax(0,38fr)_minmax(0,62fr)]",
                    )}
                >
                    {!focusMode ? (
                        <IntegrationQueuePanel
                            items={items}
                            selectedId={item?.identity.id}
                            onSelect={goToItem}
                        />
                    ) : null}

                    <div className="flex min-w-0 flex-col gap-3">
                        {item ? (
                            <>
                                <SequentialProcessBar
                                    current={positionIndex}
                                    total={positionTotal}
                                    responsibilityStatus={responsibilityStatus}
                                    responsibilityStatusLabel={
                                        !item.hasWorkItem
                                            ? item.identity.itemType ===
                                              "RECONCILIATION_DIFFERENCE"
                                                ? "直接对账"
                                                : "责任未配置"
                                            : item.workItem?.ownerUser
                                              ? `当前处理人：${item.workItem.ownerUser.displayName}`
                                              : undefined
                                    }
                                    processLabel="处理当前"
                                    processNextLabel="下一项"
                                    pending={formalPending}
                                    processDisabled={
                                        responsibilityStatus !==
                                        "assigned_to_me"
                                    }
                                    processNextDisabled={false}
                                    showProcessNext={!focusMode}
                                    onBack={() => {
                                        if (focusMode) {
                                            router.push(
                                                `/governance/integration-errors?view=${urlState.view}&queueContextId=${encodeURIComponent(urlState.queueContextId)}`,
                                            )
                                        } else {
                                            replaceUrl({
                                                taskId: null,
                                                differenceId: null,
                                            })
                                        }
                                    }}
                                    onProcess={() => {
                                        focusFirstAction()
                                    }}
                                    onProcessNext={() => {
                                        const next = neighbor(1)
                                        if (next) goToItem(next)
                                    }}
                                    onStartProcessing={
                                        item.workItem?.allowedActions.includes(
                                            "START_PROCESSING",
                                        )
                                            ? () => void handleStartProcessing()
                                            : undefined
                                    }
                                />

                                {item.workItem?.allowedActions.includes(
                                    "RELEASE_TO_TEAM",
                                ) ? (
                                    <div className="flex flex-wrap gap-2">
                                        <Button
                                            type="button"
                                            size="sm"
                                            variant="secondary"
                                            disabled={
                                                responsibilityStatus !==
                                                    "assigned_to_me" ||
                                                formalPending ||
                                                !comment.trim()
                                            }
                                            onClick={() =>
                                                void handleReleaseToTeam()
                                            }
                                        >
                                            <PauseIcon
                                                data-icon="inline-start"
                                                aria-hidden
                                            />
                                            退回团队
                                        </Button>
                                        <span className="self-center text-xs text-muted-foreground">
                                            使用下方处理说明作为退回原因
                                        </span>
                                    </div>
                                ) : null}

                                <IntegrationItemSummary
                                    item={item}
                                    headingRef={headingRef}
                                    onRefresh={returnRefresh}
                                />
                                <IntegrationEvidencePanel item={item} />
                                {panelErrorClass ? (
                                    <InterfaceErrorResolutionPanel
                                        errorClass={panelErrorClass}
                                        status={mapPanelStatus(item)}
                                        businessImpact={
                                            <span>
                                                {item.businessObject.title} ·{" "}
                                                {item.fundsImpactLabel}
                                                {item.compensationOpen
                                                    ? " · 补偿未完成"
                                                    : ""}
                                            </span>
                                        }
                                        latestAttempt={{
                                            attemptNumber:
                                                item.attempts[0]
                                                    ?.attemptNumber ?? 0,
                                            attemptedAt: {
                                                dateTime:
                                                    item.attempts[0]
                                                        ?.attemptedAt ??
                                                    item.createdAt,
                                                label: formatDateTime(
                                                    item.attempts[0]
                                                        ?.attemptedAt ??
                                                        item.createdAt,
                                                    "default",
                                                ),
                                            },
                                            result:
                                                item.attempts[0]?.result ??
                                                "尚无尝试",
                                            requestSummary:
                                                item.attempts[0]
                                                    ?.requestSummary,
                                            responseSummary:
                                                item.attempts[0]
                                                    ?.responseSummary,
                                            nextRetryAt: item.attempts[0]
                                                ?.nextRetryAt
                                                ? {
                                                      dateTime:
                                                          item.attempts[0]
                                                              .nextRetryAt,
                                                      label: formatDateTime(
                                                          item.attempts[0]
                                                              .nextRetryAt,
                                                          "default",
                                                      ),
                                                  }
                                                : undefined,
                                        }}
                                        errorCode={item.classification.label}
                                    />
                                ) : null}

                                {/* Action zone */}
                                <Card
                                    size="sm"
                                    className={surfacePanelClassName}
                                    ref={actionZoneRef}
                                >
                                    <CardHeader className="border-b border-border/30">
                                        <CardTitle>处理动作</CardTitle>
                                        <CardDescription>
                                            仅展示可操作范围；阻断原因见下方说明
                                        </CardDescription>
                                    </CardHeader>
                                    <CardContent className="space-y-3 pt-4">
                                        {item.actionBlockers.length > 0 ? (
                                            <ul className="space-y-1 text-xs text-muted-foreground">
                                                {item.actionBlockers.map(
                                                    (b) => (
                                                        <li
                                                            key={`${b.action}-${b.code}`}
                                                        >
                                                            <span className="font-medium text-foreground">
                                                                {INTEGRATION_ACTION_LABEL[
                                                                    b.action
                                                                ] ?? b.action}
                                                            </span>
                                                            ：{b.message}
                                                        </li>
                                                    ),
                                                )}
                                            </ul>
                                        ) : null}

                                        <div className="space-y-1">
                                            <Label htmlFor="w29-comment">
                                                处理说明
                                            </Label>
                                            <Textarea
                                                id="w29-comment"
                                                rows={2}
                                                value={comment}
                                                onChange={(e) =>
                                                    setComment(e.target.value)
                                                }
                                                placeholder="可选说明（不覆盖业务证据）"
                                            />
                                        </div>

                                        <div className="flex flex-wrap gap-2">
                                            {can("QUERY_ORIGINAL_RESULT") &&
                                            item.workItem ? (
                                                <Button
                                                    type="button"
                                                    disabled={
                                                        responsibilityStatus !==
                                                            "assigned_to_me" ||
                                                        formalPending
                                                    }
                                                    onClick={() =>
                                                        void runTaskAction(
                                                            "QUERY_ORIGINAL_RESULT",
                                                        )
                                                    }
                                                >
                                                    查询原结果
                                                </Button>
                                            ) : null}
                                            {can("REPLAY_ORIGINAL") &&
                                            item.workItem ? (
                                                <Button
                                                    type="button"
                                                    variant="secondary"
                                                    disabled={
                                                        responsibilityStatus !==
                                                            "assigned_to_me" ||
                                                        formalPending
                                                    }
                                                    onClick={() =>
                                                        void runTaskAction(
                                                            "REPLAY_ORIGINAL",
                                                        )
                                                    }
                                                >
                                                    重新提交
                                                </Button>
                                            ) : null}
                                            {can("ADD_EVIDENCE") &&
                                            item.workItem ? (
                                                <Button
                                                    type="button"
                                                    variant="outline"
                                                    disabled={
                                                        responsibilityStatus !==
                                                            "assigned_to_me" ||
                                                        formalPending
                                                    }
                                                    onClick={() =>
                                                        void runTaskAction(
                                                            "ADD_EVIDENCE",
                                                        )
                                                    }
                                                >
                                                    补充证据
                                                </Button>
                                            ) : null}
                                            {can("LINK_COMPENSATION") &&
                                            item.workItem ? (
                                                <Button
                                                    type="button"
                                                    variant="outline"
                                                    disabled={
                                                        responsibilityStatus !==
                                                            "assigned_to_me" ||
                                                        formalPending
                                                    }
                                                    onClick={() =>
                                                        void runTaskAction(
                                                            "LINK_COMPENSATION",
                                                        )
                                                    }
                                                >
                                                    关联补偿
                                                </Button>
                                            ) : null}
                                            {can("REATTRIBUTE") &&
                                            item.workItem ? (
                                                <Button
                                                    type="button"
                                                    variant="outline"
                                                    disabled={
                                                        responsibilityStatus !==
                                                            "assigned_to_me" ||
                                                        formalPending
                                                    }
                                                    onClick={() =>
                                                        void runTaskAction(
                                                            "REATTRIBUTE",
                                                        )
                                                    }
                                                >
                                                    重新归集
                                                </Button>
                                            ) : null}
                                            {can("RESOLVE") &&
                                            item.workItem &&
                                            item.resolutionEvidencePolicy ? (
                                                <Button
                                                    type="button"
                                                    disabled={
                                                        responsibilityStatus !==
                                                            "assigned_to_me" ||
                                                        formalPending
                                                    }
                                                    onClick={() =>
                                                        setTerminalConfirm({
                                                            kind: "RESOLVE",
                                                        })
                                                    }
                                                >
                                                    标记已解决
                                                </Button>
                                            ) : null}
                                            {item.workItem?.allowedActions.includes(
                                                "CLOSE",
                                            ) ? (
                                                <div className="flex w-full flex-wrap items-end gap-2 rounded-lg border p-2">
                                                    <div className="space-y-1">
                                                        <Label className="text-xs">
                                                            替代任务
                                                        </Label>
                                                        <ReplacementWorkItemSearchCombobox
                                                            value={
                                                                replacementTaskId ||
                                                                null
                                                            }
                                                            onValueChange={(
                                                                v,
                                                            ) =>
                                                                setReplacementTaskId(
                                                                    v ?? "",
                                                                )
                                                            }
                                                            excludeItemId={
                                                                item.identity.id
                                                            }
                                                            className="w-72"
                                                            size="sm"
                                                            aria-label="选择替代任务"
                                                            placeholder="选择替代任务（任务号 · 业务单）"
                                                            allowClear={false}
                                                        />
                                                    </div>
                                                    <Button
                                                        type="button"
                                                        size="sm"
                                                        disabled={
                                                            formalPending ||
                                                            !replacementTaskId
                                                        }
                                                        onClick={() =>
                                                            setTerminalConfirm({
                                                                kind: "CLOSE_DUPLICATE",
                                                            })
                                                        }
                                                    >
                                                        关闭重复
                                                    </Button>
                                                    <Button
                                                        type="button"
                                                        size="sm"
                                                        variant="outline"
                                                        disabled={formalPending}
                                                        onClick={() =>
                                                            setTerminalConfirm({
                                                                kind: "CLOSE_MISROUTED",
                                                            })
                                                        }
                                                    >
                                                        关闭误派
                                                    </Button>
                                                </div>
                                            ) : null}
                                        </div>

                                        {/* Direct reconciliation */}
                                        {item.identity.itemType ===
                                            "RECONCILIATION_DIFFERENCE" &&
                                        !item.hasWorkItem ? (
                                            <div className="space-y-3 rounded-xl border border-dashed p-3">
                                                <p className="text-sm font-medium">
                                                    直接对账（无关联任务）
                                                </p>
                                                <p className="text-xs text-muted-foreground">
                                                    处理完成只能「确认无误 /
                                                    确认有效差异」，引用原因注册表与受控证据；不得虚构任务关闭。
                                                </p>
                                                {item.reconciliationReasonRegistry ? (
                                                    <>
                                                        <OptionCombobox
                                                            value={
                                                                reconReasonId ||
                                                                null
                                                            }
                                                            onValueChange={(
                                                                v,
                                                            ) =>
                                                                setReconReasonId(
                                                                    v ?? "",
                                                                )
                                                            }
                                                            options={item.reconciliationReasonRegistry.registeredReasons.map(
                                                                (r) => ({
                                                                    value: r.registeredReasonId,
                                                                    label: r.label,
                                                                }),
                                                            )}
                                                            className="w-full max-w-md"
                                                            size="sm"
                                                            aria-label="注册原因"
                                                            placeholder="选择注册原因"
                                                            allowClear={false}
                                                        />
                                                        <div className="flex flex-wrap gap-2">
                                                            {can(
                                                                "CONFIRM_NO_ERROR",
                                                            ) ? (
                                                                <Button
                                                                    type="button"
                                                                    size="sm"
                                                                    disabled={
                                                                        formalPending ||
                                                                        reasonMismatches(
                                                                            "CONFIRM_NO_ERROR",
                                                                        )
                                                                    }
                                                                    onClick={() =>
                                                                        setTerminalConfirm(
                                                                            {
                                                                                kind: "CONFIRM_NO_ERROR",
                                                                            },
                                                                        )
                                                                    }
                                                                >
                                                                    确认无误
                                                                </Button>
                                                            ) : null}
                                                            {can(
                                                                "CONFIRM_VALID_DIFFERENCE",
                                                            ) ? (
                                                                <Button
                                                                    type="button"
                                                                    size="sm"
                                                                    variant="secondary"
                                                                    disabled={
                                                                        formalPending ||
                                                                        reasonMismatches(
                                                                            "CONFIRM_VALID_DIFFERENCE",
                                                                        )
                                                                    }
                                                                    onClick={() =>
                                                                        setTerminalConfirm(
                                                                            {
                                                                                kind: "CONFIRM_VALID_DIFFERENCE",
                                                                            },
                                                                        )
                                                                    }
                                                                >
                                                                    确认有效差异
                                                                </Button>
                                                            ) : null}
                                                        </div>
                                                    </>
                                                ) : (
                                                    <Alert variant="warning">
                                                        <AlertTitle>
                                                            原因注册表未配置
                                                        </AlertTitle>
                                                        <AlertDescription>
                                                            确认无误/有效差异均禁用；仅展示服务端当前开放的非终结动作。
                                                        </AlertDescription>
                                                    </Alert>
                                                )}
                                                <div className="flex flex-wrap gap-2">
                                                    {can(
                                                        "QUERY_ORIGINAL_RESULT",
                                                    ) ? (
                                                        <Button
                                                            type="button"
                                                            size="sm"
                                                            variant="outline"
                                                            disabled={
                                                                formalPending
                                                            }
                                                            onClick={() =>
                                                                void handleDirectAction(
                                                                    "QUERY_ORIGINAL_RESULT",
                                                                )
                                                            }
                                                        >
                                                            查询原结果
                                                        </Button>
                                                    ) : null}
                                                    {can("REPLAY_ORIGINAL") ? (
                                                        <Button
                                                            type="button"
                                                            size="sm"
                                                            variant="outline"
                                                            disabled={
                                                                formalPending
                                                            }
                                                            onClick={() =>
                                                                void handleDirectAction(
                                                                    "REPLAY_ORIGINAL",
                                                                )
                                                            }
                                                        >
                                                            重新提交
                                                        </Button>
                                                    ) : null}
                                                    {can("REATTRIBUTE") ? (
                                                        <Button
                                                            type="button"
                                                            size="sm"
                                                            variant="outline"
                                                            disabled={
                                                                formalPending
                                                            }
                                                            onClick={() =>
                                                                void handleDirectAction(
                                                                    "REATTRIBUTE",
                                                                )
                                                            }
                                                        >
                                                            重新归集
                                                        </Button>
                                                    ) : null}
                                                    {can(
                                                        "LINK_COMPENSATION",
                                                    ) ? (
                                                        <Button
                                                            type="button"
                                                            size="sm"
                                                            variant="outline"
                                                            disabled={
                                                                formalPending ||
                                                                item
                                                                    .linkedEvidence
                                                                    .length ===
                                                                    0
                                                            }
                                                            onClick={() =>
                                                                void handleDirectAction(
                                                                    "LINK_COMPENSATION",
                                                                )
                                                            }
                                                        >
                                                            关联补偿
                                                        </Button>
                                                    ) : null}
                                                    {can("ADD_EVIDENCE") ? (
                                                        <Button
                                                            type="button"
                                                            size="sm"
                                                            variant="outline"
                                                            disabled={
                                                                formalPending ||
                                                                item
                                                                    .linkedEvidence
                                                                    .length ===
                                                                    0
                                                            }
                                                            onClick={() =>
                                                                void handleDirectAction(
                                                                    "ADD_EVIDENCE",
                                                                )
                                                            }
                                                        >
                                                            补充证据（暂不完成对账）
                                                        </Button>
                                                    ) : null}
                                                </div>
                                            </div>
                                        ) : null}
                                    </CardContent>
                                </Card>

                                {terminalConfirm ? (
                                    <TerminalActionDialog
                                        confirm={terminalConfirm}
                                        item={item}
                                        pending={formalPending}
                                        onConfirm={async () => {
                                            const kind = terminalConfirm.kind
                                            if (kind === "CLOSE_DUPLICATE") {
                                                await handleClose(
                                                    "CLOSE_DUPLICATE",
                                                )
                                            } else if (
                                                kind === "CLOSE_MISROUTED"
                                            ) {
                                                await handleClose(
                                                    "CLOSE_MISROUTED",
                                                )
                                            } else if (kind === "RESOLVE") {
                                                await handleResolve()
                                            } else {
                                                await handleDirectTerminal(kind)
                                            }
                                            setTerminalConfirm(null)
                                        }}
                                        onCancel={() =>
                                            setTerminalConfirm(null)
                                        }
                                    />
                                ) : null}
                            </>
                        ) : (
                            <BusinessEmptyState
                                kind="filter"
                                title="未选择处理项"
                                description="从左侧队列选择任务或差异。"
                                className="rounded-lg border-0 bg-transparent shadow-none ring-0"
                            />
                        )}
                    </div>
                </div>
            )}
        </PageScaffold>
    )
}
