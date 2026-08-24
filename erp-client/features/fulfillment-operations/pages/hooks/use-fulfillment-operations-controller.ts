"use client"

import * as React from "react"
import { usePathname, useRouter, useSearchParams } from "next/navigation"

import type { ResultState as SharedResultState } from "@/components/business/feedback"
import type { FulfillmentQueueFilters } from "@/features/fulfillment-operations/api"
import { FIRST_INPUT_ID } from "@/features/fulfillment-operations/components/forms/fulfillment-draft-form"
import {
    useFulfillmentQueueQuery,
    usePostFulfillmentMutation,
    useResolveUnknownFulfillmentMutation,
    useSaveFulfillmentMutation,
} from "@/features/fulfillment-operations/hooks/queries"
import { FULFILLMENT_ROLES } from "@/features/fulfillment-operations/lib/fulfillment-roles"
import {
    canExecuteFulfillmentOperation,
    canListFulfillmentOperation,
} from "@/features/fulfillment-operations/lib/fulfillment-permissions"
import type { FulfillmentLane } from "@/features/fulfillment-operations/lib/lanes"
import {
    cloneDraft,
    clientValidation,
} from "@/features/fulfillment-operations/lib/validation"
import {
    SLUG_TO_TYPE,
    TYPE_SLUG,
    type FulfillmentDraft,
    type FulfillmentFormalOutcome,
    type FulfillmentOperationType,
} from "@/features/fulfillment-operations/types"
import { useFulfillmentActions } from "./use-fulfillment-actions"

type ResultState = SharedResultState<FulfillmentFormalOutcome>

export type FulfillmentOperationsControllerContext = {
    roleValue: FulfillmentQueueFilters["role"]
    filters: FulfillmentQueueFilters
    /** 解析后的岗位通道；无岗位深链为 null */
    lane: FulfillmentLane | null
    /** URL 里的 autoNext：1 / 0 / 未设置 */
    autoNextExplicit: string | null | undefined
    /** 销售单详情使用本地状态，切换单据和自动连续处理时不改页面地址。 */
    stateMode?: "url" | "local"
    grantedPermissions?: readonly string[]
    permissionsReady?: boolean
    onPosted?: (salesOrderId: string) => void
}

/**
 * 履约处理面的状态与动作。页面只负责布局，全部业务流转收敛在这里。
 */
export function useFulfillmentOperationsController({
    roleValue,
    filters,
    lane,
    autoNextExplicit,
    stateMode = "url",
    grantedPermissions,
    permissionsReady = true,
    onPosted,
}: FulfillmentOperationsControllerContext) {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()
    const localState = stateMode === "local"
    const [localFilters, setLocalFilters] =
        React.useState<FulfillmentQueueFilters>(filters)
    const [localAutoNextExplicit, setLocalAutoNextExplicit] =
        React.useState(autoNextExplicit)

    React.useEffect(() => {
        if (!localState) return
        setLocalFilters(filters)
        setLocalAutoNextExplicit(autoNextExplicit)
    }, [autoNextExplicit, filters, localState])

    const effectiveFilters = localState ? localFilters : filters
    const effectiveAutoNextExplicit = localState
        ? localAutoNextExplicit
        : autoNextExplicit

    const queueQuery = useFulfillmentQueueQuery(effectiveFilters)
    const saveMutation = useSaveFulfillmentMutation()
    const postMutation = usePostFulfillmentMutation()
    const resolveUnknownMutation = useResolveUnknownFulfillmentMutation()

    const view = queueQuery.data
    const context = view?.context
    const roleVisibleTypes =
        context?.visibleTypes ?? FULFILLMENT_ROLES[roleValue].types
    const permissionChecksEnabled = grantedPermissions !== undefined
    const visibleTypes = React.useMemo(
        () =>
            permissionChecksEnabled && permissionsReady
                ? roleVisibleTypes.filter((type) =>
                      canListFulfillmentOperation(grantedPermissions, type),
                  )
                : roleVisibleTypes,
        [
            grantedPermissions,
            permissionChecksEnabled,
            permissionsReady,
            roleVisibleTypes,
        ],
    )
    const operations = React.useMemo(
        () =>
            (view?.operations ?? []).filter((candidate) =>
                visibleTypes.includes(candidate.operationType),
            ),
        [view?.operations, visibleTypes],
    )
    const currentOperationId = effectiveFilters.currentOperationId
    const operation =
        operations.find((t) => t.operationId === currentOperationId) ??
        operations.find(
            (candidate) => candidate.operationId === view?.current?.operationId,
        ) ??
        operations[0]
    const canExecute =
        Boolean(context?.canExecute ?? true) &&
        Boolean(operation) &&
        (!permissionChecksEnabled ||
            (permissionsReady &&
                canExecuteFulfillmentOperation(
                    grantedPermissions,
                    operation!.operationType,
                )))
    const executeBlockedReason =
        permissionChecksEnabled && !permissionsReady
            ? "正在核对履约操作权限，请稍候。"
            : permissionChecksEnabled && operation && !canExecute
              ? "当前账号可以查看这类单据，但没有保存或确认权限。"
              : undefined
    const currentIndex = operation
        ? Math.max(
              0,
              operations.findIndex(
                  (t) => t.operationId === operation.operationId,
              ),
          )
        : 0
    const permissionDenied =
        permissionChecksEnabled && permissionsReady && visibleTypes.length === 0
    const emptyReason = permissionDenied ? "NO_PERMISSION" : view?.emptyReason
    const completed =
        Boolean(view) &&
        operations.length === 0 &&
        emptyReason !== "NO_PERMISSION" &&
        emptyReason !== "FILTER_NO_RESULT"

    const [sessionAutoNext, setSessionAutoNext] = React.useState(
        () => roleValue !== "warehouse",
    )
    const autoNext =
        effectiveAutoNextExplicit === "0"
            ? false
            : effectiveAutoNextExplicit === "1"
              ? true
              : sessionAutoNext

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
        if (localState || queueQuery.isPending || !view) return
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
        localState,
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
            if (localState) {
                setLocalFilters((current) => {
                    const next = { ...current }
                    if (Object.hasOwn(patch, "currentOperationId")) {
                        next.currentOperationId =
                            patch.currentOperationId || undefined
                    }
                    if (Object.hasOwn(patch, "type")) {
                        const type = patch.type
                            ? SLUG_TO_TYPE[patch.type]
                            : undefined
                        next.operationTypes = type ? [type] : undefined
                    }
                    if (Object.hasOwn(patch, "purchaseOrderId")) {
                        next.purchaseOrderId =
                            patch.purchaseOrderId || undefined
                    }
                    if (Object.hasOwn(patch, "warehouseId")) {
                        next.warehouseId = patch.warehouseId || undefined
                    }
                    if (Object.hasOwn(patch, "q")) {
                        next.q = patch.q || undefined
                    }
                    if (Object.hasOwn(patch, "due")) {
                        next.due =
                            patch.due === "today" || patch.due === "overdue"
                                ? patch.due
                                : undefined
                    }
                    if (Object.hasOwn(patch, "gate")) {
                        next.gate =
                            patch.gate === "blocked" ||
                            patch.gate === "satisfied"
                                ? patch.gate
                                : undefined
                    }
                    return {
                        ...next,
                        role: filters.role,
                        salesOrderId: filters.salesOrderId,
                    }
                })
                if (Object.hasOwn(patch, "autoNext")) {
                    setLocalAutoNextExplicit(patch.autoNext ?? undefined)
                }
                return
            }

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
        [
            filters.role,
            filters.salesOrderId,
            localState,
            pathname,
            router,
            searchParams,
        ],
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

    const actions = useFulfillmentActions({
        operation,
        draft,
        dirty,
        autoNext,
        canExecute,
        pendingIdempotencyKey: lastResult?.pendingIdempotencyKey,
        saveMutation,
        postMutation,
        resolveUnknownMutation,
        neighborId,
        goToOperation,
        advanceIfNeeded,
        setDirty,
        setActionError,
        setSaveMessage,
        setConfirmOpen,
        setLastResult,
        onPosted,
    })

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

    const handleNavigate = React.useCallback(
        (delta: 1 | -1) => {
            if (dirty) {
                setActionError("有未保存修改，请先保存或放弃后再切换")
                return
            }
            const next = neighborId(delta)
            if (next) goToOperation(next)
        },
        [dirty, goToOperation, neighborId],
    )

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

    const setAutoNext = React.useCallback(
        (next: boolean) => {
            setSessionAutoNext(next)
            replaceUrl({ autoNext: next ? "1" : "0" })
        },
        [replaceUrl],
    )

    const goToWarehouseShip = React.useCallback(
        (salesOrderId: string) => {
            setDirty(false)
            replaceUrl({
                purchaseOrderId: null,
                salesOrderId,
                currentOperationId: null,
                type: "warehouse_ship",
                autoNext: "0",
            })
        },
        [replaceUrl],
    )

    const currentUrl = `${pathname}?${searchParams.toString()}`

    return {
        queueQuery,
        view,
        context,
        operations,
        operation,
        currentIndex,
        completed,
        emptyReason,
        canExecute,
        executeBlockedReason,
        visibleTypes,
        draft,
        dirty,
        confirmOpen,
        setConfirmOpen,
        lastResult,
        shortcutsOpen,
        setShortcutsOpen,
        actionError,
        saveMessage,
        headingRef,
        resultRef,
        validationIssues,
        canPost,
        formalPending,
        supportsSave: actions.supportsSave,
        autoNext,
        currentUrl,
        updateDraft,
        handleDiscard,
        handleSave: actions.handleSave,
        handlePost: actions.handlePost,
        handleSkip: actions.handleSkip,
        handleResolveUnknown: actions.handleResolveUnknown,
        handleNavigate,
        goToOperation,
        setActionError,
        setTypeFilter,
        clearAllFilters,
        handlePatch,
        setAutoNext,
        goToWarehouseShip,
    }
}

export type FulfillmentOperationsController = ReturnType<
    typeof useFulfillmentOperationsController
>
