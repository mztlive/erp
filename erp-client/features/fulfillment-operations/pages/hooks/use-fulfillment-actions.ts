"use client"

import * as React from "react"

import {
    OPERATION_DONE_LABEL,
    type FulfillmentDraft,
    type FulfillmentOperation,
} from "@/features/fulfillment-operations/types"
import { getErrorMessage } from "@/lib/api/errors"
import {
    FormalCommandKeyLedger,
    classifyFormalCommandError,
} from "@/lib/formal-command"
import type {
    PostFulfillmentOperationCommand,
    SaveFulfillmentOperationCommand,
} from "../../types"
import { resultText } from "@/lib/ui-text"
import { createIdempotencyKey } from "../lib/idempotency"

type ResultState = import("@/components/business/feedback").ResultState<
    import("@/features/fulfillment-operations/types").FulfillmentFormalOutcome
>

type SaveMutation = ReturnType<
    typeof import("@/features/fulfillment-operations/hooks/queries").useSaveFulfillmentMutation
>
type PostMutation = ReturnType<
    typeof import("@/features/fulfillment-operations/hooks/queries").usePostFulfillmentMutation
>
type ResolveUnknownMutation = ReturnType<
    typeof import("@/features/fulfillment-operations/hooks/queries").useResolveUnknownFulfillmentMutation
>

export type FulfillmentActionsOptions = {
    operation: FulfillmentOperation | undefined
    draft: FulfillmentDraft | null
    dirty: boolean
    autoNext: boolean
    canExecute?: boolean
    /** 上一次结果未确认时保留的请求标识，用于补查 */
    pendingIdempotencyKey: string | undefined
    saveMutation: SaveMutation
    postMutation: PostMutation
    resolveUnknownMutation: ResolveUnknownMutation
    /** 前进 delta 位返回相邻 operationId；无相邻返回 undefined */
    neighborId: (delta: number) => string | undefined
    goToOperation: (
        operationId: string | null | undefined,
        keepResult?: boolean,
    ) => void
    advanceIfNeeded: (
        shouldAdvance: boolean,
        preferredNext?: string,
        keepResult?: boolean,
    ) => void
    markDraftPristine: () => void
    setActionError: React.Dispatch<React.SetStateAction<string | null>>
    setSaveMessage: React.Dispatch<React.SetStateAction<string | null>>
    setConfirmOpen: React.Dispatch<React.SetStateAction<boolean>>
    setLastResult: React.Dispatch<React.SetStateAction<ResultState>>
    onPosted?: (salesOrderId: string) => void
    onOperationCompleted?: (operationId: string) => void
}

/**
 * 单据命令的提交编排：保存、确认、跳过与结果补查。
 * 只编排 mutation 与结果状态，不做页面级筛选/导航决策。
 */
export function useFulfillmentActions({
    operation,
    draft,
    dirty,
    autoNext,
    canExecute = true,
    pendingIdempotencyKey,
    saveMutation,
    postMutation,
    resolveUnknownMutation,
    neighborId,
    goToOperation,
    advanceIfNeeded,
    markDraftPristine,
    setActionError,
    setSaveMessage,
    setConfirmOpen,
    setLastResult,
    onPosted,
    onOperationCompleted,
}: FulfillmentActionsOptions) {
    const ledger = React.useRef(new FormalCommandKeyLedger())
    const postInFlight = React.useRef(false)
    const postContext = React.useRef<FulfillmentOperation | null>(null)
    const supportsSave =
        draft?.type === "RECEIPT" ||
        draft?.type === "WAREHOUSE_SHIP" ||
        draft?.type === "SUPPLIER_DIRECT"

    const handleSave = React.useCallback(async (): Promise<boolean> => {
        if (!operation || !draft) return false
        if (
            (draft.type === "WAREHOUSE_SHIP" ||
                draft.type === "SUPPLIER_DIRECT") &&
            draft.pendingTrackingLineIds?.length
        ) {
            setActionError("请先点击添加物流号，再保存草稿")
            return false
        }
        if (ledger.current.peek("post")) {
            setActionError("请先确认本次发货结果，确认前不能另存草稿")
            return false
        }
        if (!canExecute) {
            setActionError("当前账号没有保存这类履约单据的权限")
            return false
        }
        if (!supportsSave) {
            setActionError("这类履约单据没有草稿保存命令，请直接确认")
            return false
        }
        try {
            const command =
                ledger.current.acquire<SaveFulfillmentOperationCommand>(
                    "save",
                    `fulfillment:${operation.operationId}:save`,
                    {
                        operationId: operation.operationId,
                        expectedDocumentVersion: operation.editVersion,
                        expectedSourceVersion: operation.sourceVersion,
                        idempotencyKey: createIdempotencyKey(
                            operation.operationId,
                            operation.editVersion,
                            "save",
                        ),
                        draft,
                    },
                )
            await saveMutation.mutateAsync(command.payload)
            ledger.current.settle("save", "succeeded")
            setLastResult(null)
            markDraftPristine()
            setSaveMessage("草稿已保存")
            setActionError(null)
            return true
        } catch (error) {
            const outcome = classifyFormalCommandError(error)
            ledger.current.settle("save", outcome)
            if (outcome === "unknown") {
                setLastResult({
                    status: "unknown",
                    title: resultText.unknown,
                    description:
                        "保存结果暂无法确认，当前输入已保留，请使用本次操作重试。",
                    pendingIdempotencyKey:
                        ledger.current.peek<SaveFulfillmentOperationCommand>(
                            "save",
                        )?.payload.idempotencyKey,
                    stayOnItem: true,
                })
            }
            setActionError(
                getErrorMessage(error, "保存失败，请检查必填项后重试"),
            )
            return false
        }
    }, [
        canExecute,
        draft,
        saveMutation,
        supportsSave,
        operation,
        markDraftPristine,
        setActionError,
        setSaveMessage,
        setLastResult,
    ])

    const handlePost = React.useCallback(async () => {
        const frozen =
            ledger.current.peek<PostFulfillmentOperationCommand>("post")
        const activeOperation = frozen ? postContext.current : operation
        const activeDraft = frozen?.payload.draft ?? draft
        if (!activeOperation || !activeDraft) return
        if (postInFlight.current || ledger.current.peek("save")) return
        if (!canExecute) {
            setActionError("当前账号没有确认这类履约单据的权限")
            setConfirmOpen(false)
            return
        }
        setActionError(null)
        postInFlight.current = true
        if (!frozen) postContext.current = activeOperation
        try {
            const nextId = neighborId(1)
            const command =
                frozen ??
                ledger.current.acquire<PostFulfillmentOperationCommand>(
                    "post",
                    `fulfillment:${activeOperation.operationId}:post`,
                    {
                        operationId: activeOperation.operationId,
                        expectedSourceVersion: activeOperation.sourceVersion,
                        expectedDocumentVersion: activeOperation.editVersion,
                        idempotencyKey: createIdempotencyKey(
                            activeOperation.operationId,
                            activeOperation.editVersion,
                            "post",
                        ),
                        draft: activeDraft,
                    },
                )
            const response = await postMutation.mutateAsync(command.payload)
            const uncertainConflict = Boolean(
                frozen &&
                response.status === "failed" &&
                response.code === "SUBJECT_VERSION_MISMATCH",
            )
            ledger.current.settle(
                "post",
                uncertainConflict ? "unknown" : response.status,
            )
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
                if (!uncertainConflict) setLastResult(null)
                return
            }
            const outcome = {
                ...response.outcome,
                salesOrderId:
                    response.outcome.salesOrderId ||
                    activeOperation.source.salesOrderId,
                salesOrderNo:
                    response.outcome.salesOrderNo ||
                    activeOperation.source.salesOrderNo,
            }
            setLastResult({
                status: "succeeded",
                title: OPERATION_DONE_LABEL[response.outcome.operationType],
                description: autoNext
                    ? "已记下来了，马上打开下一条。"
                    : activeOperation.operationType === "RECEIPT"
                      ? "已记下来了。合格的货已入库并按销售单留好，可以继续本单仓发。"
                      : "已记下来了。可以先核对一下库存变化再继续。",
                reference: response.outcome.factNo,
                outcome,
                stayOnItem: !autoNext,
            })
            onPosted?.(outcome.salesOrderId)
            onOperationCompleted?.(outcome.operationId)
            if (autoNext) {
                advanceIfNeeded(true, nextId, true)
            }
        } catch (error) {
            const outcome = classifyFormalCommandError(error)
            ledger.current.settle("post", outcome)
            if (outcome === "unknown") {
                setLastResult({
                    status: "unknown",
                    title: resultText.unknown,
                    description:
                        "处理结果暂无法确认，当前输入已保留，请查询结果或使用本次操作重试。",
                    pendingIdempotencyKey:
                        ledger.current.peek<PostFulfillmentOperationCommand>(
                            "post",
                        )?.payload.idempotencyKey,
                    stayOnItem: true,
                })
            }
            setActionError(getErrorMessage(error, "提交失败，请稍后重试"))
        } finally {
            postInFlight.current = false
        }
    }, [
        advanceIfNeeded,
        autoNext,
        canExecute,
        draft,
        neighborId,
        onPosted,
        onOperationCompleted,
        postMutation,
        operation,
        setActionError,
        setConfirmOpen,
        setLastResult,
    ])

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
    }, [dirty, goToOperation, neighborId, setActionError])

    const handleResolveUnknown = React.useCallback(async () => {
        if (ledger.current.peek("save")) {
            await handleSave()
            return
        }
        const frozen =
            ledger.current.peek<PostFulfillmentOperationCommand>("post")
        const operationId =
            frozen?.payload.operationId ?? operation?.operationId
        if (!operationId) return
        const response = await resolveUnknownMutation.mutateAsync({
            operationId,
            idempotencyKey:
                frozen?.payload.idempotencyKey ??
                pendingIdempotencyKey ??
                createIdempotencyKey(
                    operationId,
                    operation?.editVersion ??
                        frozen?.payload.expectedDocumentVersion ??
                        0,
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
            ledger.current.settle("post", "succeeded")
            setLastResult({
                status: "succeeded",
                title: "查到了：这一条已经做完",
                description: "该单据当前已完成，请核对本次处理信息。",
                reference: response.outcome.factNo,
                outcome: response.outcome,
                stayOnItem: !autoNext,
            })
            onPosted?.(response.outcome.salesOrderId)
            onOperationCompleted?.(response.outcome.operationId)
            if (autoNext) advanceIfNeeded(true, undefined, true)
        }
    }, [
        advanceIfNeeded,
        autoNext,
        pendingIdempotencyKey,
        resolveUnknownMutation,
        operation,
        onPosted,
        onOperationCompleted,
        setActionError,
        setLastResult,
        handleSave,
    ])

    return {
        supportsSave,
        handleSave,
        handlePost,
        handleSkip,
        handleResolveUnknown,
        handleRetryUnknown: () =>
            ledger.current.peek("save") ? handleSave() : handlePost(),
    }
}
