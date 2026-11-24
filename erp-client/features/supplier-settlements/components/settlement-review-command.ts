import { useEffect, useRef, useState } from "react"

import type { ResultState } from "@/components/business/feedback"
import { useReviewDecisionMutation } from "@/features/supplier-settlements/hooks/queries"
import { outcomeToResult } from "@/features/supplier-settlements/lib/operations"
import type {
    ReviewDecisionInput,
    SettlementDetailView,
} from "@/features/supplier-settlements/types"
import { getErrorMessage, isApiError } from "@/lib/api/errors"
import {
    FormalCommandKeyLedger,
    isCommandOutcomeUnknown,
} from "@/lib/formal-command"

export function useSettlementReviewCommand({
    detail,
    onCompleted,
    onCloseForm,
    onConflict,
}: {
    detail: SettlementDetailView
    onCompleted: (
        action: ReviewDecisionInput["action"],
        workItemId: string,
    ) => void
    onCloseForm: () => void
    onConflict: () => void
}) {
    const mutation = useReviewDecisionMutation()
    const [ledger] = useState(() => new FormalCommandKeyLedger())
    const submitting = useRef(false)
    const [result, setResult] = useState<ResultState>(null)
    const [awaitingResult, setAwaitingResult] = useState(false)

    type Payload = Omit<ReviewDecisionInput, "idempotencyKey">
    const slot = detail.statement.id
    const onCompletedRef = useRef(onCompleted)
    onCompletedRef.current = onCompleted

    // 若刷新后服务端已推进状态（例如原未知提交实际已生效），自动结束待核对态并释放幂等槽。
    useEffect(() => {
        if (!awaitingResult) return
        const settled =
            detail.statement.status !== "PENDING_REVIEW" ||
            (detail.workItem != null && detail.workItem.status !== "OPEN")
        if (!settled) return
        const pendingIdentity = ledger.peek<Payload>(slot)
        ledger.settle(slot, "succeeded")
        setAwaitingResult(false)
        setResult({
            status: "succeeded",
            title:
                detail.statement.status === "CONFIRMED"
                    ? "结算已确认生效"
                    : "结算单状态已更新",
            description: `已刷新核对最新状态：${detail.statement.statusLabel}。`,
        })
        if (pendingIdentity) {
            onCompletedRef.current(
                pendingIdentity.payload.action,
                pendingIdentity.payload.workItemId,
            )
        }
    }, [
        awaitingResult,
        detail.statement.status,
        detail.statement.statusLabel,
        detail.workItem,
        ledger,
        slot,
    ])

    async function execute(identity: {
        payload: Payload
        idempotencyKey: string
    }) {
        if (submitting.current) return
        submitting.current = true
        try {
            const outcome = await mutation.mutateAsync({
                ...identity.payload,
                idempotencyKey: identity.idempotencyKey,
            })
            const unknown = outcome.status === "unknown"
            ledger.settle(slot, unknown ? "unknown" : "succeeded")
            setAwaitingResult(unknown)
            setResult(outcomeToResult(outcome))
            onCloseForm()
            if (
                outcome.status === "succeeded" ||
                outcome.status === "rejected"
            ) {
                onCompleted(
                    identity.payload.action,
                    identity.payload.workItemId,
                )
            }
        } catch (error) {
            const unknown = isCommandOutcomeUnknown(error)
            ledger.settle(slot, unknown ? "unknown" : "failed")
            setAwaitingResult(unknown)
            if (isApiError(error) && error.status === 409 && !unknown) {
                onConflict()
            }
            setResult({
                status: unknown ? "unknown" : "rejected",
                title: unknown ? "复核结果待确认" : "复核未完成",
                description: unknown
                    ? "原请求可能已生效。刷新核对当前状态后，可按原请求重试。"
                    : getErrorMessage(error, "请核对结算单状态后重试。"),
            })
            onCloseForm()
        } finally {
            submitting.current = false
        }
    }

    async function submit(
        action: ReviewDecisionInput["action"],
        reasonCode?: string,
        comment?: string,
    ) {
        const workItem = detail.workItem
        if (!workItem || submitting.current || ledger.peek(slot)) return
        const identity = ledger.acquire<Payload>(slot, "w27:review", {
            action,
            statementId: detail.statement.id,
            workItemId: workItem.workItemId,
            expectedTaskVersion: workItem.taskVersion,
            expectedSubjectVersion: workItem.subjectVersion,
            expectedLockVersion: detail.statement.lockVersion,
            operationId: `w27:review:${crypto.randomUUID()}`,
            reasonCode,
            comment: comment?.trim() || undefined,
        })
        await execute(identity)
    }

    async function retry() {
        const identity = ledger.peek<Payload>(slot)
        if (identity) await execute(identity)
    }

    return {
        result,
        awaitingResult,
        pending: mutation.isPending,
        clearResult: () => setResult(null),
        submit,
        retry,
    }
}
