"use client"

import * as React from "react"
import { useRouter } from "@/lib/navigation"

import { Button } from "@/components/ui/button"
import { documentIsEditableDraft } from "@/features/approval-workflow/api/document-cancel"
import { getErrorPresentation } from "@/lib/api/errors"
import { ApprovalWithdrawDialog } from "./approval-withdraw-dialog"

type WithdrawGate = {
    enabled: boolean
    reason?: string
}

type WithdrawResult = {
    status: "succeeded" | "unknown" | "blocked"
    title: string
    description: string
    reference?: string
}

/**
 * 销售单与采购单页头「撤回审批」。
 *
 * 可见性、权限闸门、命令内容和稳定 id 由调用方提供。
 * 底层统一复用 `ApprovalWithdrawDialog`（强制 destructive + BPM 512B UTF-8 校验 + 幂等恢复）。
 */
export function DocumentWithdrawApprovalButton<TCommand>({
    visible,
    permitted,
    pending,
    gate,
    reviseRejected,
    documentNoun,
    ids,
    createIdempotencyKey,
    buildCommand,
    documentType,
    documentIdOf,
    submit,
    successHref,
    reference,
    onResult,
    onTriggerClick,
    triggerVariant = "outline",
    triggerClassName,
}: {
    visible: boolean
    permitted: boolean
    pending: boolean
    gate: WithdrawGate
    reviseRejected: boolean
    documentNoun: string
    ids: {
        trigger: string
        reason: string
        cancel: string
        confirm: string
    }
    createIdempotencyKey: () => string
    buildCommand: (input: {
        reason: string
        idempotencyKey: string
    }) => TCommand
    documentType: string
    documentIdOf: (command: TCommand) => string
    submit: (command: TCommand) => Promise<unknown>
    successHref?: string
    reference?: string
    onResult?: (result: WithdrawResult) => void
    onTriggerClick?: () => void
    triggerVariant?: React.ComponentProps<typeof Button>["variant"]
    triggerClassName?: string
}) {
    const router = useRouter()
    const [open, setOpen] = React.useState(false)
    const pendingCommand = React.useRef<TCommand | null>(null)

    if (!visible || !permitted) return null

    const idPrefix = ids.cancel.endsWith("-cancel")
        ? ids.cancel.slice(0, -"-cancel".length)
        : ids.trigger

    const notifySucceeded = () => {
        if (successHref) router.push(successHref)
        onResult?.({
            status: "succeeded",
            title: "审批已撤回",
            description: "已撤回当前审批，单据回到可编辑草稿。",
            reference,
        })
    }

    const summary = reference
        ? [
              <span key="ref">
                  {documentNoun}编号：
                  <span className="num font-medium text-foreground">
                      {reference}
                  </span>
              </span>,
          ]
        : []

    const effects = [
        `撤回后，${documentNoun}将回到草稿。`,
        ...(reviseRejected
            ? [`原${documentNoun}编号和审批记录保留，修改后重新提交审批。`]
            : []),
    ]

    return (
        <>
            <Button
                id={ids.trigger}
                type="button"
                size="sm"
                variant={triggerVariant}
                className={triggerClassName}
                disabled={!gate.enabled}
                title={gate.reason}
                onClick={() => {
                    onTriggerClick?.()
                    pendingCommand.current = null
                    setOpen(true)
                }}
            >
                {reviseRejected ? "修改原单" : "撤回审批"}
            </Button>
            <ApprovalWithdrawDialog
                open={open}
                onOpenChange={(next) => {
                    if (!next) pendingCommand.current = null
                    setOpen(next)
                }}
                idPrefix={idPrefix}
                reasonId={ids.reason}
                title={reviseRejected ? "修改原单" : "撤回审批"}
                description={`请确认撤回影响并填写撤回原因，撤回后${documentNoun}将回到可编辑草稿。`}
                actionLabel="撤回审批"
                confirmLabel={reviseRejected ? "撤回并修改原单" : "确认撤回"}
                unknownConfirmLabel="核对撤回结果"
                cancelLabel="取消"
                fromStatus={{
                    label: reviseRejected ? "已驳回" : "审批中",
                    tone: reviseRejected ? "destructive" : "info",
                }}
                toStatus={{ label: "草稿", tone: "neutral" }}
                summary={summary}
                effects={effects}
                reasonLabel="撤回原因"
                reasonPlaceholder="请输入撤回原因"
                reviseRejected={reviseRejected}
                pending={pending}
                createIdempotencyKey={createIdempotencyKey}
                checkUnknownResolved={async () => {
                    const command = pendingCommand.current
                    if (!command) return false
                    const resolved = await documentIsEditableDraft(
                        documentType,
                        documentIdOf(command),
                    )
                    if (resolved) {
                        pendingCommand.current = null
                        notifySucceeded()
                    }
                    return resolved
                }}
                onSubmitWithdraw={async ({ reason, idempotencyKey }) => {
                    const command =
                        pendingCommand.current ??
                        buildCommand({ reason, idempotencyKey })
                    pendingCommand.current = command
                    await submit(command)
                    pendingCommand.current = null
                    notifySucceeded()
                }}
                onSubmitError={(error, isUnknown) => {
                    if (!isUnknown) {
                        pendingCommand.current = null
                    }
                    const failure = getErrorPresentation(
                        error,
                        "撤回审批未完成，请刷新后重试。",
                    )
                    onResult?.({
                        status: isUnknown ? "unknown" : "blocked",
                        title: isUnknown ? "处理结果待确认" : failure.title,
                        description: isUnknown
                            ? "请使用本次操作重试；确认前不要修改输入或重复撤回。"
                            : failure.description,
                        reference,
                    })
                }}
                formatError={(error, isUnknown) => {
                    if (isUnknown) {
                        return "处理结果待确认，请点击「核对撤回结果」重试；确认前不要修改输入或重复撤回。"
                    }
                    return getErrorPresentation(
                        error,
                        "撤回审批未完成，请刷新后重试。",
                    ).description
                }}
            />
        </>
    )
}
