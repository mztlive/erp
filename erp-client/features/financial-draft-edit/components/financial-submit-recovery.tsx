"use client"

import { useState } from "react"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import { getErrorMessage } from "@/lib/api/errors"
import {
    useFinancialDraftMutations,
    useFinancialDraftQuery,
} from "../hooks/queries"
import type { FinancialDraft, FinancialDraftKind } from "../types"

const statusLabels: Record<string, string> = {
    draft: "草稿",
    rejected: "已驳回",
    in_approval: "审批中",
    posted: "已过账",
    reversed: "已冲正",
}

/** 仅凭新读取的更高版本允许用户结束本地核对，不认定原提交结果。 */
export function FinancialSubmitRecovery({
    kind,
    documentId,
    expectedVersion,
    idPrefix,
    pending,
    onEnd,
    onBusyChange,
}: {
    kind: Exclude<FinancialDraftKind, "customer_receipt">
    documentId: string
    expectedVersion: number
    idPrefix: string
    pending: boolean
    onEnd: () => void
    onBusyChange: (busy: boolean) => void
}) {
    const query = useFinancialDraftQuery(kind, documentId, false)
    const commands = useFinancialDraftMutations(kind, documentId)
    const [checked, setChecked] = useState<FinancialDraft | null>(null)
    const [error, setError] = useState<unknown>(null)
    const [finishing, setFinishing] = useState(false)
    const latest = checked?.id === documentId ? checked : null
    const advanced = Boolean(latest && latest.version > expectedVersion)
    async function readLatest() {
        if (pending) return
        setChecked(null)
        setError(null)
        onBusyChange(true)
        try {
            const result = await query.refetch()
            if (result.isError) {
                setError(result.error)
                return
            }
            const value = result.data
            if (
                !value ||
                value.id !== documentId ||
                !Number.isSafeInteger(value.version) ||
                value.version < 1 ||
                typeof value.status !== "string" ||
                !Object.hasOwn(statusLabels, value.status.toLowerCase())
            ) {
                setError(new Error("未取得完整的单据版本和状态，请重新读取"))
                return
            }
            setChecked(value)
        } finally {
            onBusyChange(false)
        }
    }
    async function endRecovery() {
        if (pending || !latest || !advanced) return
        setFinishing(true)
        onBusyChange(true)
        try {
            await commands.confirm(latest)
            onEnd()
        } catch (cause) {
            setError(cause)
        } finally {
            setFinishing(false)
            onBusyChange(false)
        }
    }
    return (
        <div className="space-y-2 rounded-lg border p-3 text-sm">
            <p>
                如果原单已有后续修改，可读取最新单据后结束本次核对。版本变化不能证明原提交成功或失败。
            </p>
            <LoadingButton
                id={`${idPrefix}-read-latest`}
                type="button"
                variant="outline"
                loading={query.isFetching}
                disabled={pending || query.isFetching || finishing}
                onClick={() => void readLatest()}
            >
                读取最新单据
            </LoadingButton>
            {error ? (
                <p role="alert" className="text-destructive">
                    {getErrorMessage(error, "读取最新单据失败，请重试")}
                </p>
            ) : null}
            {latest ? (
                <p>
                    当前版本 {latest.version} ·{" "}
                    {statusLabels[latest.status.toLowerCase()]}。
                    {advanced
                        ? "原版本已不能再次提交。结束核对后请按最新单据状态继续办理。"
                        : "版本未前进，请继续使用原请求核对。"}
                </p>
            ) : null}
            {advanced ? (
                <Button
                    id={`${idPrefix}-end-recovery`}
                    type="button"
                    variant="outline"
                    disabled={pending || query.isFetching || finishing}
                    onClick={() => void endRecovery()}
                >
                    结束本次核对，查看最新单据
                </Button>
            ) : null}
        </div>
    )
}
