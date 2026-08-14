"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import {
    ArrowRightIcon,
    CircleCheckIcon,
    EyeIcon,
    SkipForwardIcon,
    SaveIcon,
    Undo2Icon,
} from "lucide-react"

import {
    BusinessEmptyState,
    BusinessFailureState,
    BusinessStatusBadge,
    DataFreshness,
    FormalActionConfirmDialog,
    FormalActionResult,
    PageHeader,
    PageScaffold,
    PrepaymentGate,
    SequentialProcessBar,
    surfaceInsetClassName,
    surfacePanelClassName,
    ValidationSummary,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import { Separator } from "@/components/ui/separator"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import type {
    FulfillmentDraft,
    FulfillmentFormalOutcome,
    FulfillmentOperationType,
} from "@/features/fulfillment-operations/types"
import { type ResultState as SharedResultState } from "@/components/business/feedback"
import {
    CORRECTION_NOTICE,
    NOT_ACCEPTANCE_NOTICE,
    OPERATION_ACTION_LABEL,
    OPERATION_CLEARED_LABEL,
    OPERATION_CONFIRM_TITLE,
    OPERATION_DONE_LABEL,
    OPERATION_TYPE_LABEL,
    OPERATION_TYPE_SHORT,
    SLUG_TO_TYPE,
    TYPE_SLUG,
} from "@/features/fulfillment-operations/types"
import {
    useFulfillmentQueueQuery,
    usePostFulfillmentMutation,
    useResolveUnknownFulfillmentMutation,
    useSaveFulfillmentMutation,
} from "@/features/fulfillment-operations/hooks/queries"
import {
    DEFAULT_FULFILLMENT_ROLE,
    FULFILLMENT_ROLES,
} from "@/features/fulfillment-operations/lib/fulfillment-roles"
import {
    laneHeader,
    resolveLane,
} from "@/features/fulfillment-operations/lib/lanes"
import {
    parseDueParam,
    parseGateParam,
    parseTypeParam,
    typeParamValue,
} from "@/features/fulfillment-operations/lib/filters"
import { cn } from "@/lib/utils"
import { getErrorMessage } from "@/lib/api/errors"
import {
    buildPostedFacts,
    clientValidation,
    cloneDraft,
    impactPreview,
} from "@/features/fulfillment-operations/lib/validation"
import {
    FIRST_INPUT_ID,
    FulfillmentDraftForm,
} from "@/features/fulfillment-operations/components/forms/fulfillment-draft-form"
import { FulfillmentQueueList } from "@/features/fulfillment-operations/components/queue/fulfillment-queue-list"
import { FulfillmentQueueToolbar } from "@/features/fulfillment-operations/components/queue/fulfillment-queue-toolbar"
import { freshnessText, resultText, workspaceLabel } from "@/lib/ui-text"
import type { WorkspaceId } from "@/lib/workspace-registry"

type ResultState = SharedResultState<FulfillmentFormalOutcome>

function createIdempotencyKey(
    operationId: string,
    documentVersion: number,
    action: "save" | "post",
): string {
    return `w09:${operationId}:${documentVersion}:${action}:${crypto.randomUUID()}`
}

export function FulfillmentOperationsPage() {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()

    const lane = resolveLane(searchParams.get("lane"))
    const header = laneHeader(lane)
    // 队列按岗位通道取角色（仓储/采购经办）；无岗位深链回落默认角色
    const roleValue = lane ?? DEFAULT_FULFILLMENT_ROLE
    const operationTypes = parseTypeParam(searchParams.get("type"))
    const warehouseId = searchParams.get("warehouseId") ?? undefined
    const q = searchParams.get("q") ?? undefined
    const due = parseDueParam(searchParams.get("due"))
    const gate = parseGateParam(searchParams.get("gate"))
    const salesOrderId = searchParams.get("salesOrderId") ?? undefined
    const purchaseOrderId = searchParams.get("purchaseOrderId") ?? undefined
    const currentOperationId =
        searchParams.get("currentOperationId") ?? undefined
    const returnTo = searchParams.get("returnTo") ?? undefined
    const fromWorkspace = searchParams.get("from") ?? undefined

    const autoNextExplicit = searchParams.get("autoNext")
    const [sessionAutoNext, setSessionAutoNext] = React.useState(true)
    const autoNext =
        autoNextExplicit === "0"
            ? false
            : autoNextExplicit === "1"
              ? true
              : sessionAutoNext

    const filters = React.useMemo(
        (): import("@/features/fulfillment-operations/api").FulfillmentQueueFilters => ({
            role: roleValue,
            operationTypes,
            warehouseId,
            q,
            due,
            gate,
            salesOrderId,
            purchaseOrderId,
            currentOperationId,
        }),
        [
            roleValue,
            operationTypes,
            warehouseId,
            q,
            due,
            gate,
            salesOrderId,
            purchaseOrderId,
            currentOperationId,
        ],
    )

    const queueQuery = useFulfillmentQueueQuery(filters)
    const saveMutation = useSaveFulfillmentMutation()
    const postMutation = usePostFulfillmentMutation()
    const resolveUnknownMutation = useResolveUnknownFulfillmentMutation()

    const view = queueQuery.data
    const operations = React.useMemo(
        () => view?.operations ?? [],
        [view?.operations],
    )
    const context = view?.context
    const canExecute = context?.canExecute ?? true
    const visibleTypes =
        context?.visibleTypes ?? FULFILLMENT_ROLES[roleValue].types
    const operation =
        operations.find((t) => t.operationId === currentOperationId) ??
        view?.current ??
        operations[0]
    const currentIndex = operation
        ? Math.max(
              0,
              operations.findIndex(
                  (t) => t.operationId === operation.operationId,
              ),
          )
        : 0
    const completed = Boolean(view) && operations.length === 0

    const [draft, setDraft] = React.useState<FulfillmentDraft | null>(null)
    const [dirty, setDirty] = React.useState(false)
    const [confirmOpen, setConfirmOpen] = React.useState(false)
    const [lastResult, setLastResult] = React.useState<ResultState>(null)
    const [shortcutsOpen, setShortcutsOpen] = React.useState(false)
    const [actionError, setActionError] = React.useState<string | null>(null)
    const [saveMessage, setSaveMessage] = React.useState<string | null>(null)
    const headingRef = React.useRef<HTMLHeadingElement>(null)
    const resultRef = React.useRef<HTMLDivElement>(null)

    React.useEffect(() => {
        if (!operation) {
            setDraft(null)
            setDirty(false)
            return
        }
        setDraft(cloneDraft(operation.draft))
        setDirty(false)
        setActionError(null)
        setSaveMessage(null)
    }, [operation])

    React.useEffect(() => {
        if (queueQuery.isPending || !view) return
        const hasLane = searchParams.has("lane")
        const hasItem = searchParams.has("currentOperationId")
        // 没有确定岗位（只读角色 / 未声明岗位的深链）就不写 lane，
        // 否则侧栏会高亮到用户没有选择的岗位入口。
        const laneSettled = hasLane || lane === null
        if (laneSettled && (hasItem || operations.length === 0)) {
            return
        }
        const params = new URLSearchParams(searchParams.toString())
        if (!hasLane && lane) params.set("lane", lane)
        if (!hasItem && operation) {
            params.set("currentOperationId", operation.operationId)
        }
        const qs = params.toString()
        router.replace(qs ? `${pathname}?${qs}` : pathname, { scroll: false })
    }, [
        queueQuery.isPending,
        view,
        searchParams,
        lane,
        operation,
        operations.length,
        pathname,
        router,
    ])

    React.useEffect(() => {
        if (lastResult) {
            resultRef.current?.focus()
            return
        }
        if (!operation) return
        // 可执行角色直接落到第一个要填的框并全选，省一次鼠标；
        // 标题挂了 aria-live，换条时仍会播报，不靠抢焦点来通知。
        if (canExecute) {
            const el = document.getElementById(
                FIRST_INPUT_ID[operation.operationType],
            ) as HTMLInputElement | HTMLTextAreaElement | null
            if (el) {
                el.focus()
                el.select?.()
                return
            }
        }
        headingRef.current?.focus()
    }, [operation, lastResult, canExecute])

    const replaceUrl = React.useCallback(
        (patch: Record<string, string | null | undefined>) => {
            const params = new URLSearchParams(searchParams.toString())
            for (const [key, value] of Object.entries(patch)) {
                if (value == null || value === "") params.delete(key)
                else params.set(key, value)
            }
            const qs = params.toString()
            router.replace(qs ? `${pathname}?${qs}` : pathname, {
                scroll: false,
            })
        },
        [pathname, router, searchParams],
    )

    const goToOperation = React.useCallback(
        (operationId: string | undefined | null, keepResult?: boolean) => {
            if (!keepResult) setLastResult(null)
            setActionError(null)
            replaceUrl({
                currentOperationId: operationId ?? null,
            })
        },
        [replaceUrl],
    )

    const neighborId = React.useCallback(
        (delta: number) => {
            const idx = currentIndex + delta
            return operations[idx]?.operationId
        },
        [currentIndex, operations],
    )

    const validationIssues =
        operation && draft ? clientValidation(operation, draft) : []
    const canPost =
        canExecute &&
        Boolean(operation && draft) &&
        validationIssues.length === 0 &&
        !(
            operation?.gate.state === "BLOCKED" &&
            operation.operationType !== "WAREHOUSE_SHIP"
        ) &&
        !operation?.actionBlockers.some((b) => b.action === "POST")

    const formalPending =
        postMutation.isPending ||
        saveMutation.isPending ||
        resolveUnknownMutation.isPending
    const supportsSave =
        draft?.type === "RECEIPT" ||
        draft?.type === "WAREHOUSE_SHIP" ||
        draft?.type === "SUPPLIER_DIRECT"

    const updateDraft = React.useCallback((next: FulfillmentDraft) => {
        setDraft(next)
        setDirty(true)
    }, [])

    /** 回到最近一次保存的草稿；多处「请先保存或放弃」提示都指向这里 */
    const handleDiscard = React.useCallback(() => {
        if (!operation) return
        setDraft(cloneDraft(operation.draft))
        setDirty(false)
        setActionError(null)
        setSaveMessage(null)
    }, [operation])

    const handleSave = React.useCallback(async (): Promise<boolean> => {
        if (!operation || !draft) return false
        if (!supportsSave) {
            setActionError("这类履约单据没有草稿保存命令，请直接确认")
            return false
        }
        try {
            await saveMutation.mutateAsync({
                operationId: operation.operationId,
                expectedDocumentVersion: operation.editVersion,
                expectedSourceVersion: operation.sourceVersion,
                idempotencyKey: createIdempotencyKey(
                    operation.operationId,
                    operation.editVersion,
                    "save",
                ),
                draft,
            })
            setDirty(false)
            setSaveMessage("草稿已保存")
            setActionError(null)
            return true
        } catch (error) {
            setActionError(getErrorMessage(error, "保存失败"))
            return false
        }
    }, [draft, saveMutation, supportsSave, operation])

    const advanceIfNeeded = React.useCallback(
        (
            shouldAdvance: boolean,
            preferredNext?: string,
            keepResult?: boolean,
        ) => {
            if (!shouldAdvance) return
            const nextId =
                preferredNext ??
                neighborId(1) ??
                operations.find((t) => t.operationId !== operation?.operationId)
                    ?.operationId
            if (nextId) goToOperation(nextId, keepResult)
            else replaceUrl({ currentOperationId: null })
        },
        [
            goToOperation,
            neighborId,
            replaceUrl,
            operation?.operationId,
            operations,
        ],
    )

    const handlePost = React.useCallback(async () => {
        if (!operation || !draft) return
        setActionError(null)
        try {
            const nextId = neighborId(1)
            const response = await postMutation.mutateAsync({
                operationId: operation.operationId,
                expectedSourceVersion: operation.sourceVersion,
                expectedDocumentVersion: operation.editVersion,
                idempotencyKey: createIdempotencyKey(
                    operation.operationId,
                    operation.editVersion,
                    "post",
                ),
                draft,
            })
            setConfirmOpen(false)

            if (response.status === "unknown") {
                setLastResult({
                    status: "unknown",
                    title: resultText.unknown,
                    description: response.message,
                    pendingIdempotencyKey: response.idempotencyKey,
                    stayOnItem: true,
                })
                return
            }
            if (response.status === "failed") {
                setActionError(response.message)
                return
            }
            setLastResult({
                status: "succeeded",
                title: OPERATION_DONE_LABEL[response.outcome.operationType],
                description: autoNext
                    ? "已记下来了，马上打开下一条。"
                    : "已记下来了。可以先核对一下库存变化再继续。",
                reference: response.outcome.factNo,
                outcome: response.outcome,
                stayOnItem: !autoNext,
            })
            if (autoNext) {
                advanceIfNeeded(true, nextId, true)
            }
        } catch (error) {
            setActionError(getErrorMessage(error, "没能提交成功"))
        }
    }, [advanceIfNeeded, autoNext, draft, neighborId, postMutation, operation])

    const handleSkip = React.useCallback(() => {
        if (dirty) {
            setActionError("有未保存修改，请先保存或放弃后再切换")
            return
        }
        const nextId = neighborId(1)
        if (!nextId) {
            setActionError("当前已是最后一条单据")
            return
        }
        goToOperation(nextId)
    }, [dirty, goToOperation, neighborId])

    const handleResolveUnknown = React.useCallback(async () => {
        if (!operation || !draft) return
        const response = await resolveUnknownMutation.mutateAsync({
            operationId: operation.operationId,
            idempotencyKey:
                lastResult?.pendingIdempotencyKey ??
                createIdempotencyKey(
                    operation.operationId,
                    operation.editVersion,
                    "post",
                ),
        })
        if (response.status === "unknown") {
            setLastResult({
                status: "unknown",
                title: "还是没查到结果",
                description: response.message,
                pendingIdempotencyKey: response.idempotencyKey,
                stayOnItem: true,
            })
            return
        }
        if (response.status === "failed") {
            setActionError(response.message)
            return
        }
        if (response.outcome.kind === "POSTED") {
            setLastResult({
                status: "succeeded",
                title: "查到了：这一条已经做完",
                description: "查到的是同一条记录，库存和留货没有被重复改动。",
                reference: response.outcome.factNo,
                outcome: response.outcome,
                stayOnItem: !autoNext,
            })
            if (autoNext) advanceIfNeeded(true)
        }
    }, [
        advanceIfNeeded,
        autoNext,
        draft,
        lastResult?.pendingIdempotencyKey,
        resolveUnknownMutation,
        operation,
    ])

    React.useEffect(() => {
        const onKey = (event: KeyboardEvent) => {
            const target = event.target as HTMLElement | null
            const tag = target?.tagName
            const inField =
                tag === "INPUT" ||
                tag === "TEXTAREA" ||
                tag === "SELECT" ||
                target?.isContentEditable

            if (
                (event.metaKey || event.ctrlKey) &&
                event.key.toLowerCase() === "s"
            ) {
                event.preventDefault()
                if (canExecute && supportsSave) void handleSave()
                return
            }
            if (
                (event.metaKey || event.ctrlKey) &&
                event.key === "Enter" &&
                !inField
            ) {
                event.preventDefault()
                if (canPost && !formalPending) setConfirmOpen(true)
                return
            }
            if (inField) return
            if (event.key === "?") {
                event.preventDefault()
                setShortcutsOpen((v) => !v)
                return
            }
            if (
                (event.key === "ArrowDown" || event.key === "ArrowUp") &&
                target instanceof HTMLButtonElement
            ) {
                // 焦点在队列列表按钮上时保留原生滚动，不劫持方向键
                return
            }
            if (event.key === "j" || event.key === "ArrowDown") {
                event.preventDefault()
                if (dirty) {
                    setActionError("有没保存的修改，先保存或放弃再切换")
                    return
                }
                const next = neighborId(1)
                if (next) goToOperation(next)
            }
            if (event.key === "k" || event.key === "ArrowUp") {
                event.preventDefault()
                if (dirty) {
                    setActionError("有未保存修改，请先保存或放弃后再切换")
                    return
                }
                const prev = neighborId(-1)
                if (prev) goToOperation(prev)
            }
        }
        window.addEventListener("keydown", onKey)
        return () => window.removeEventListener("keydown", onKey)
    }, [
        canPost,
        dirty,
        formalPending,
        goToOperation,
        handleSave,
        neighborId,
        canExecute,
        supportsSave,
    ])

    const setTypeFilter = React.useCallback(
        (next: FulfillmentOperationType | "all") => {
            if (dirty) {
                setActionError("有没保存的修改，先保存或放弃再切换类型")
                return
            }
            setLastResult(null)
            replaceUrl({
                type: next === "all" ? null : TYPE_SLUG[next],
                currentOperationId: null,
            })
        },
        [dirty, replaceUrl],
    )

    /** 空态出口：类型、单号、仓库、到期、门禁和来源对象筛选一次清干净 */
    const clearAllFilters = React.useCallback(() => {
        if (dirty) {
            setActionError("有没保存的修改，先保存或放弃再清除筛选")
            return
        }
        setLastResult(null)
        replaceUrl({
            type: null,
            q: null,
            warehouseId: null,
            due: null,
            gate: null,
            salesOrderId: null,
            purchaseOrderId: null,
            currentOperationId: null,
        })
    }, [dirty, replaceUrl])

    const handlePatch = React.useCallback(
        (patch: Record<string, string | null | undefined>) => {
            if (dirty) {
                setActionError("有没保存的修改，先保存或放弃再改筛选")
                return
            }
            setLastResult(null)
            replaceUrl(patch)
        },
        [dirty, replaceUrl],
    )

    const responsibilityStatus =
        operation?.gate.state === "BLOCKED"
            ? "blocked"
            : canExecute
              ? "assigned_to_me"
              : "assigned_to_other"
    const responsibilityStatusLabel = !canExecute
        ? "只能查看"
        : operation?.gate.state === "BLOCKED"
          ? "业务条件未满足"
          : "当前岗位可处理"

    /** 只读角色看到的一句话：谁在处理、什么时候要交 */
    const readOnlyNote = operation
        ? `你只能查看。这条由 ${operation.responsibleLabel} 处理，${
              operation.overdue
                  ? `原定 ${operation.dueLabel}，已超期`
                  : `预计 ${operation.dueLabel} 前完成`
          }。`
        : "你只能查看这些单据的进度。"

    const activeTypeSlug = typeParamValue(operationTypes)
    const sourceReturnHref =
        returnTo ??
        (fromWorkspace === "W05" && operation
            ? `/sales/orders/${operation.source.salesOrderId}`
            : fromWorkspace === "W08" && operation?.source.purchaseOrderId
              ? `/procurement/orders`
              : fromWorkspace === "W10"
                ? `/inventory`
                : undefined)

    if (queueQuery.isPending) {
        return (
            <PageScaffold>
                <PageHeader title={header.label} description="正在加载队列…" />
                <div className="h-20 animate-pulse rounded-lg bg-muted" />
                <div className="grid gap-4 xl:grid-cols-[minmax(16rem,1fr)_minmax(0,2fr)]">
                    <div className="h-80 animate-pulse rounded-lg bg-muted" />
                    <div className="h-96 animate-pulse rounded-lg bg-muted" />
                </div>
            </PageScaffold>
        )
    }

    // 查询失败必须与「没有符合条件的单据」区分：系统故障 ≠ 没活干
    if (queueQuery.isError) {
        return (
            <PageScaffold>
                <PageHeader title={header.label} description="队列加载失败" />
                <BusinessFailureState
                    error={queueQuery.error}
                    action={
                        <Button
                            type="button"
                            onClick={() => void queueQuery.refetch()}
                        >
                            重新加载
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }

    return (
        <PageScaffold>
            <PageHeader
                title={header.label}
                description={header.description}
                breadcrumbs={[
                    ...(header.group
                        ? [
                              {
                                  id: "group",
                                  label: header.group.label,
                                  href: header.group.href,
                              },
                          ]
                        : []),
                    { id: "fulfillment", label: header.label, current: true },
                ]}
                metadata={
                    <div className="flex flex-wrap items-center gap-3">
                        <DataFreshness
                            updatedAt="刚刚"
                            dateTime={context?.snapshotUpdatedAt}
                            state="fresh"
                            label={freshnessText.dataUpdatedAt}
                        />
                        <span
                            className="text-xs text-muted-foreground"
                            aria-live="polite"
                        >
                            {context?.filterSummary ?? "全部类型"} · 待处理{" "}
                            {context?.total ?? 0}
                        </span>
                    </div>
                }
            />

            {sourceReturnHref ? (
                <div
                    className={`${surfaceInsetClassName} flex flex-wrap items-center justify-between gap-2 px-3 py-2.5 text-sm`}
                >
                    <span className="text-muted-foreground">
                        从
                        {fromWorkspace
                            ? workspaceLabel(fromWorkspace as WorkspaceId)
                            : "关联页面"}
                        进来的
                        {operation
                            ? ` · 已经定位到 ${operation.source.salesOrderNo}${
                                  operation.source.purchaseNo
                                      ? ` / ${operation.source.purchaseNo}`
                                      : ""
                              }`
                            : ""}
                        。返回时会回到原来的位置。
                    </span>
                    <Button
                        type="button"
                        size="sm"
                        variant="ghost"
                        render={<Link href={sourceReturnHref} />}
                    >
                        返回来源
                    </Button>
                </div>
            ) : null}

            {/* M3 sticky 处理面：第 0 层范围/类型 + 第 1/2 层 ListToolbar（ui-filter-design §2.3） */}
            <div
                className={cn(
                    surfacePanelClassName,
                    "sticky top-0 z-10 space-y-2.5 px-3 py-2.5",
                )}
            >
                <div className="flex flex-wrap items-center gap-2">
                    <ToggleGroup
                        value={[
                            activeTypeSlug === "all" ? "all" : activeTypeSlug,
                        ]}
                        onValueChange={(values) => {
                            const next = values[0]
                            if (!next) return
                            if (next === "all") setTypeFilter("all")
                            else {
                                const t = SLUG_TO_TYPE[next]
                                if (t) setTypeFilter(t)
                            }
                        }}
                        variant="outline"
                        size="sm"
                        spacing={0}
                        className="w-fit flex-wrap"
                        aria-label="作业类型"
                    >
                        <ToggleGroupItem value="all">全部</ToggleGroupItem>
                        {visibleTypes.map((t) => (
                            <ToggleGroupItem key={t} value={TYPE_SLUG[t]}>
                                {OPERATION_TYPE_SHORT[t]}
                            </ToggleGroupItem>
                        ))}
                    </ToggleGroup>
                </div>

                <FulfillmentQueueToolbar
                    q={q}
                    warehouseId={warehouseId}
                    due={due}
                    gate={gate}
                    salesOrderId={salesOrderId}
                    purchaseOrderId={purchaseOrderId}
                    salesOrderNo={
                        operations.find(
                            (t) => t.source.salesOrderId === salesOrderId,
                        )?.source.salesOrderNo
                    }
                    purchaseNo={
                        operations.find(
                            (t) => t.source.purchaseOrderId === purchaseOrderId,
                        )?.source.purchaseNo
                    }
                    autoNext={autoNext}
                    total={context?.total ?? operations.length}
                    showAutoNext={canExecute}
                    type={activeTypeSlug}
                    onPatch={handlePatch}
                    onAutoNextChange={(next) => {
                        setSessionAutoNext(next)
                        replaceUrl({ autoNext: next ? "1" : "0" })
                    }}
                />
            </div>

            {lastResult ? (
                <div ref={resultRef} tabIndex={-1} className="outline-none">
                    <FormalActionResult
                        status={
                            lastResult.status === "failed"
                                ? "blocked"
                                : lastResult.status
                        }
                        title={lastResult.title}
                        description={
                            lastResult.outcome?.kind === "POSTED" &&
                            lastResult.outcome.acceptanceRequired ? (
                                <span className="block space-y-1">
                                    <span className="block">
                                        {lastResult.description}
                                    </span>
                                    <span className="block text-muted-foreground">
                                        {NOT_ACCEPTANCE_NOTICE}
                                    </span>
                                </span>
                            ) : (
                                lastResult.description
                            )
                        }
                        reference={lastResult.reference}
                        facts={
                            lastResult.outcome
                                ? buildPostedFacts(lastResult.outcome)
                                : undefined
                        }
                        actions={
                            <div className="flex flex-wrap gap-2">
                                {lastResult.status === "unknown" ? (
                                    <Button
                                        type="button"
                                        size="sm"
                                        onClick={() =>
                                            void handleResolveUnknown()
                                        }
                                    >
                                        查询最终结果
                                    </Button>
                                ) : null}
                                {lastResult.outcome?.kind === "POSTED" &&
                                lastResult.outcome.acceptanceRequired ? (
                                    <Button
                                        type="button"
                                        size="sm"
                                        variant="outline"
                                        render={
                                            <Link
                                                href={`/sales/orders/${lastResult.outcome.salesOrderId}?section=acceptance&from=W09&returnTo=${encodeURIComponent(
                                                    `${pathname}?${searchParams.toString()}`,
                                                )}`}
                                            />
                                        }
                                    >
                                        去登记客户验收
                                        <ArrowRightIcon data-icon="inline-end" />
                                    </Button>
                                ) : null}
                                {lastResult.stayOnItem === false ||
                                lastResult.status === "blocked" ? null : (
                                    <Button
                                        type="button"
                                        size="sm"
                                        onClick={() => {
                                            const next =
                                                neighborId(1) ||
                                                operations[0]?.operationId
                                            goToOperation(next)
                                        }}
                                    >
                                        下一条
                                    </Button>
                                )}
                            </div>
                        }
                    />
                </div>
            ) : null}

            {actionError ? (
                <Alert variant="destructive">
                    <AlertTitle>没有生效</AlertTitle>
                    <AlertDescription>{actionError}</AlertDescription>
                </Alert>
            ) : null}

            {completed ? (
                <BusinessEmptyState
                    kind="no-tasks"
                    className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                    title={
                        operationTypes?.length === 1
                            ? OPERATION_CLEARED_LABEL[operationTypes[0]]
                            : "这批活都干完了"
                    }
                    description="可以换个类型看看，或者清掉筛选、回工作台。"
                    action={
                        <div className="flex flex-wrap gap-2">
                            <Button
                                type="button"
                                variant="secondary"
                                className="rounded-lg shadow-none"
                                onClick={clearAllFilters}
                            >
                                清除全部筛选
                            </Button>
                            <Button render={<Link href="/workspace" />}>
                                回今日工作台
                            </Button>
                        </div>
                    }
                />
            ) : operation && draft ? (
                <div className="grid min-h-[28rem] min-w-0 gap-4 xl:grid-cols-[minmax(15rem,0.9fr)_minmax(0,2.1fr)]">
                    <FulfillmentQueueList
                        operations={operations}
                        currentIndex={currentIndex}
                        position={context?.position ?? currentIndex + 1}
                        total={context?.total ?? operations.length}
                        onSelect={(operationId) => {
                            if (
                                dirty &&
                                operationId !== operation.operationId
                            ) {
                                setActionError(
                                    "有未保存修改，请先保存或放弃后再切换",
                                )
                                return
                            }
                            goToOperation(operationId)
                        }}
                    />

                    <div className="min-w-0 space-y-3">
                        <SequentialProcessBar
                            current={context?.position ?? currentIndex + 1}
                            total={context?.total ?? operations.length}
                            responsibilityStatus={responsibilityStatus}
                            responsibilityStatusLabel={
                                responsibilityStatusLabel
                            }
                            showProcess={canExecute}
                            processLabel={
                                OPERATION_ACTION_LABEL[operation.operationType]
                            }
                            // 没有独立的「并下一项」路径：两个 handler 同义，第二个按钮名不副实
                            showProcessNext={false}
                            processDisabled={formalPending || !canPost}
                            statusExtras={
                                operation.gate.state !== "NOT_APPLICABLE" ? (
                                    <PrepaymentGate
                                        id="prepayment-gate"
                                        presentation="badge"
                                        copy={
                                            operation.operationType ===
                                            "WAREHOUSE_SHIP"
                                                ? {
                                                      title: "发货条件",
                                                      description:
                                                          "只认已经到账并核销过的货款，付款申请和附件不算。",
                                                      allowedBadge: "可以发货",
                                                      blockedBadge:
                                                          "暂时不能发货",
                                                      amountTerm: "至少要付",
                                                      ratioTerm: "至少要付比例",
                                                      allocatedTerm: "已经付了",
                                                      gapTerm: "还差",
                                                      updatedTerm:
                                                          "算到什么时候",
                                                      allowedTitle:
                                                          "货款已到，可以发货",
                                                      blockedTitle:
                                                          "先款未到，暂时不能发货",
                                                      allowedBody:
                                                          "货款已经够了，这一单可以继续。",
                                                      blockedBody:
                                                          "差额补齐之前，仓发单据暂时不能确认发货。",
                                                  }
                                                : {
                                                      title: "先款条件",
                                                      description:
                                                          "只认已经到账并核销过的货款，付款申请和附件不算。",
                                                      allowedBadge: "可以收货",
                                                      blockedBadge:
                                                          "暂时不能收货",
                                                      amountTerm: "至少要付",
                                                      ratioTerm: "至少要付比例",
                                                      allocatedTerm: "已经付了",
                                                      gapTerm: "还差",
                                                      updatedTerm:
                                                          "算到什么时候",
                                                      allowedTitle:
                                                          "货款已到，可以收货",
                                                      blockedTitle:
                                                          "先款未到，暂时不能收货",
                                                      allowedBody:
                                                          "货款已经够了，这一单可以继续。",
                                                      blockedBody:
                                                          "差额补齐之前，入库、直发、电子交付和服务都确认不了。",
                                                  }
                                        }
                                        condition={{
                                            kind: "amount",
                                            required:
                                                operation.gate.requiredAmount ??
                                                "—",
                                            description: operation.gate.message,
                                        }}
                                        allocated={
                                            operation.gate
                                                .effectivePaidAmount ?? "—"
                                        }
                                        gap={
                                            operation.gate.state ===
                                                "BLOCKED" &&
                                            operation.gate.requiredAmount &&
                                            operation.gate.effectivePaidAmount
                                                ? String(
                                                      Math.max(
                                                          0,
                                                          Number(
                                                              operation.gate
                                                                  .requiredAmount,
                                                          ) -
                                                              Number(
                                                                  operation.gate
                                                                      .effectivePaidAmount,
                                                              ),
                                                      ),
                                                  )
                                                : "0"
                                        }
                                        updatedAt={{
                                            label: "刚刚",
                                            dateTime:
                                                context?.snapshotUpdatedAt ??
                                                "",
                                        }}
                                        allowed={
                                            operation.gate.state === "SATISFIED"
                                        }
                                        paymentAction={
                                            operation.gate.state ===
                                            "BLOCKED" ? (
                                                <Button
                                                    type="button"
                                                    size="sm"
                                                    variant="outline"
                                                    render={
                                                        <Link
                                                            href={`/finance/supplier-accounts?from=W09&purchaseOrderId=${operation.source.purchaseOrderId ?? ""}&returnTo=${encodeURIComponent(
                                                                `${pathname}?${searchParams.toString()}`,
                                                            )}`}
                                                        />
                                                    }
                                                >
                                                    去登记付款
                                                </Button>
                                            ) : undefined
                                        }
                                    />
                                ) : (
                                    <BusinessStatusBadge
                                        context="list"
                                        id="prepayment-gate"
                                        tone="neutral"
                                        label={
                                            operation.operationType ===
                                            "WAREHOUSE_SHIP"
                                                ? "发货条件：无先款要求"
                                                : "无先款要求"
                                        }
                                        description={operation.gate.message}
                                    />
                                )
                            }
                            onBack={() => {
                                if (sourceReturnHref)
                                    router.push(sourceReturnHref)
                                else router.push("/workspace")
                            }}
                            backLabel="返回"
                            onProcess={() => setConfirmOpen(true)}
                            onProcessNext={() => setConfirmOpen(true)}
                        />

                        <button
                            type="button"
                            onClick={() => setShortcutsOpen((v) => !v)}
                            aria-expanded={shortcutsOpen}
                            className="self-start text-xs text-muted-foreground hover:text-foreground"
                        >
                            {shortcutsOpen
                                ? `快捷键：J / K 上下条${
                                      canExecute
                                          ? " · Ctrl+S 保存 · Ctrl+Enter 确认"
                                          : ""
                                  } · 再按 ? 收起`
                                : "按 ? 看快捷键"}
                        </button>

                        <Card size="sm" className={surfacePanelClassName}>
                            <CardHeader className="border-b border-border/30">
                                <div className="flex flex-wrap items-start justify-between gap-2">
                                    <div>
                                        <CardTitle
                                            ref={headingRef}
                                            tabIndex={-1}
                                            aria-live="polite"
                                            className="outline-none"
                                        >
                                            {
                                                OPERATION_TYPE_LABEL[
                                                    operation.operationType
                                                ]
                                            }{" "}
                                            · {operation.source.salesOrderNo}
                                        </CardTitle>
                                        <CardDescription>
                                            {operation.source.customerLabel}
                                            {operation.source.purchaseNo
                                                ? ` · 采购 ${operation.source.purchaseNo}`
                                                : ""}
                                            {operation.source.supplierLabel
                                                ? ` · ${operation.source.supplierLabel}`
                                                : ""}
                                        </CardDescription>
                                    </div>
                                    <BusinessStatusBadge
                                        context="list"
                                        label={operation.statusLabel}
                                        tone={operation.statusTone}
                                    />
                                </div>
                            </CardHeader>
                            <CardContent className="space-y-4">
                                <section aria-label="来源上下文">
                                    <dl className="grid gap-px overflow-hidden rounded-lg border border-grid bg-grid sm:grid-cols-2 lg:grid-cols-3">
                                        {[
                                            {
                                                label: "销售单",
                                                value: operation.source
                                                    .salesOrderNo,
                                                href: `/sales/orders/${operation.source.salesOrderId}?from=W09&returnTo=${encodeURIComponent(
                                                    `${pathname}?${searchParams.toString()}`,
                                                )}`,
                                            },
                                            {
                                                label: "采购单",
                                                value:
                                                    operation.source
                                                        .purchaseNo ?? "—",
                                            },
                                            {
                                                label: "仓库",
                                                value:
                                                    operation.source
                                                        .warehouseLabel ??
                                                    "不涉及仓库",
                                            },
                                            {
                                                label: "还剩多少",
                                                value: operation.lines
                                                    .map(
                                                        (l) =>
                                                            `${l.itemName} ${l.remainingQuantity}${l.unitCode}`,
                                                    )
                                                    .join("；"),
                                                numeric: true,
                                            },
                                            {
                                                label: "供应商",
                                                value:
                                                    operation.source
                                                        .supplierLabel ?? "—",
                                            },
                                            {
                                                label: "客户",
                                                value: operation.source
                                                    .customerLabel,
                                            },
                                        ].map((field) => (
                                            <div
                                                key={field.label}
                                                className="bg-card p-3"
                                            >
                                                <dt className="text-xs text-muted-foreground">
                                                    {field.label}
                                                </dt>
                                                <dd
                                                    className={cn(
                                                        "mt-1 font-medium",
                                                        field.numeric && "num",
                                                    )}
                                                >
                                                    {field.href &&
                                                    field.value !== "—" ? (
                                                        <Link
                                                            href={field.href}
                                                            className="text-primary underline-offset-4 hover:underline"
                                                        >
                                                            {field.value}
                                                        </Link>
                                                    ) : (
                                                        field.value
                                                    )}
                                                </dd>
                                            </div>
                                        ))}
                                    </dl>
                                </section>

                                <Separator />

                                <FulfillmentDraftForm
                                    operation={operation}
                                    draft={draft}
                                    onChange={updateDraft}
                                    disabled={
                                        formalPending ||
                                        !canExecute ||
                                        lastResult?.status === "unknown"
                                    }
                                />

                                {validationIssues.length > 0 ? (
                                    <ValidationSummary
                                        title="还差这些没填好"
                                        issues={validationIssues}
                                    />
                                ) : null}

                                {saveMessage ? (
                                    <p className="text-xs text-muted-foreground">
                                        {saveMessage}
                                    </p>
                                ) : null}

                                {canExecute ? (
                                    <div className="sticky bottom-0 flex flex-wrap justify-end gap-2 border-t border-border/30 bg-card/95 py-3 backdrop-blur">
                                        <Button
                                            type="button"
                                            variant="ghost"
                                            disabled={formalPending}
                                            onClick={handleSkip}
                                        >
                                            <SkipForwardIcon data-icon="inline-start" />
                                            先跳过
                                        </Button>
                                        <Button
                                            type="button"
                                            variant="ghost"
                                            disabled={formalPending || !dirty}
                                            onClick={handleDiscard}
                                        >
                                            <Undo2Icon data-icon="inline-start" />
                                            放弃修改
                                        </Button>
                                        <Button
                                            type="button"
                                            variant="secondary"
                                            className="rounded-lg shadow-none"
                                            disabled={
                                                formalPending ||
                                                !dirty ||
                                                !supportsSave
                                            }
                                            onClick={() => void handleSave()}
                                        >
                                            <SaveIcon data-icon="inline-start" />
                                            保存草稿
                                        </Button>
                                        <Button
                                            type="button"
                                            disabled={formalPending || !canPost}
                                            onClick={() => setConfirmOpen(true)}
                                        >
                                            <CircleCheckIcon data-icon="inline-start" />
                                            {autoNext
                                                ? `${OPERATION_ACTION_LABEL[operation.operationType]}并下一条`
                                                : OPERATION_ACTION_LABEL[
                                                      operation.operationType
                                                  ]}
                                        </Button>
                                    </div>
                                ) : (
                                    /* 只读角色：与其摆一排点不动的按钮，不如说清楚谁在处理 */
                                    <div className="sticky bottom-0 flex flex-wrap items-center justify-between gap-2 border-t border-border/30 bg-card/95 py-3 backdrop-blur">
                                        <p className="flex items-center gap-2 text-sm text-muted-foreground">
                                            <EyeIcon
                                                className="size-4 shrink-0"
                                                aria-hidden="true"
                                            />
                                            {readOnlyNote}
                                        </p>
                                        <Button
                                            type="button"
                                            size="sm"
                                            variant="secondary"
                                            className="rounded-lg shadow-none"
                                            render={
                                                <Link
                                                    href={`/sales/orders/${operation.source.salesOrderId}?from=W09&returnTo=${encodeURIComponent(
                                                        `${pathname}?${searchParams.toString()}`,
                                                    )}`}
                                                />
                                            }
                                        >
                                            打开销售单
                                            <ArrowRightIcon data-icon="inline-end" />
                                        </Button>
                                    </div>
                                )}
                            </CardContent>
                        </Card>
                    </div>
                </div>
            ) : view?.emptyReason === "NO_PERMISSION" ? (
                <BusinessEmptyState
                    kind="no-scope"
                    className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                    title="你没有这类单据的权限"
                    description={`${context?.roleLabel ?? FULFILLMENT_ROLES[roleValue].label}能处理的是：${visibleTypes
                        .map((t) => OPERATION_TYPE_SHORT[t])
                        .join("、")}。`}
                    action={
                        <Button
                            type="button"
                            variant="secondary"
                            className="rounded-lg shadow-none"
                            onClick={clearAllFilters}
                        >
                            回到我能处理的
                        </Button>
                    }
                />
            ) : (
                <BusinessEmptyState
                    kind="filter"
                    className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                    title="没有符合条件的单据"
                    description={context?.filterSummary ?? "换个类型或单号试试"}
                    action={
                        <Button
                            type="button"
                            variant="secondary"
                            className="rounded-lg shadow-none"
                            onClick={clearAllFilters}
                        >
                            清除全部筛选
                        </Button>
                    }
                />
            )}

            <FormalActionConfirmDialog
                open={confirmOpen}
                onOpenChange={setConfirmOpen}
                title={
                    operation
                        ? OPERATION_CONFIRM_TITLE[operation.operationType]
                        : "确认？"
                }
                description="没确认成功之前，库存和留货都不会动。"
                actionLabel={
                    operation
                        ? OPERATION_ACTION_LABEL[operation.operationType]
                        : "确认"
                }
                confirmLabel={
                    operation
                        ? OPERATION_ACTION_LABEL[operation.operationType]
                        : "确认"
                }
                fromStatus={{ label: "待确认", tone: "warning" }}
                toStatus={{
                    label: operation
                        ? OPERATION_DONE_LABEL[operation.operationType]
                        : "已完成",
                    tone: "success",
                }}
                lockedFields={["来源单据、版本和留货", "单据类型"]}
                effects={
                    operation && draft ? impactPreview(operation, draft) : []
                }
                irreversibleEffects={[CORRECTION_NOTICE]}
                nextDepartment="做完之后由销售登记客户验收"
                pending={postMutation.isPending}
                onConfirm={async () => {
                    await handlePost()
                }}
            />
        </PageScaffold>
    )
}
