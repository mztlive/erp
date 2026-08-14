"use client"

import * as React from "react"

import {
    OPERATION_DONE_LABEL,
    type FulfillmentDraft,
    type FulfillmentOperation,
} from "@/features/fulfillment-operations/types"
import { getErrorMessage } from "@/lib/api/errors"
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
    setDirty: React.Dispatch<React.SetStateAction<boolean>>
    setActionError: React.Dispatch<React.SetStateAction<string | null>>
    setSaveMessage: React.Dispatch<React.SetStateAction<string | null>>
    setConfirmOpen: React.Dispatch<React.SetStateAction<boolean>>
    setLastResult: React.Dispatch<React.SetStateAction<ResultState>>
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
    pendingIdempotencyKey,
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
}: FulfillmentActionsOptions) {
    const supportsSave =
        draft?.type === "RECEIPT" ||
        draft?.type === "WAREHOUSE_SHIP" ||
        draft?.type === "SUPPLIER_DIRECT"

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
    }, [
        draft,
        saveMutation,
        supportsSave,
        operation,
        setDirty,
        setActionError,
        setSaveMessage,
    ])

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
    }, [
        advanceIfNeeded,
        autoNext,
        draft,
        neighborId,
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
        if (!operation || !draft) return
        const response = await resolveUnknownMutation.mutateAsync({
            operationId: operation.operationId,
            idempotencyKey:
                pendingIdempotencyKey ??
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
        pendingIdempotencyKey,
        resolveUnknownMutation,
        operation,
        setActionError,
        setLastResult,
    ])

    return {
        supportsSave,
        handleSave,
        handlePost,
        handleSkip,
        handleResolveUnknown,
    }
}
