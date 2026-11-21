"use client"

import { useRef, useState } from "react"
import { MoneyValue, FormalActionConfirmDialog } from "@/components/business"
import { Button } from "@/components/ui/button"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import { classifyFormalCommandError } from "@/lib/formal-command"
import { getErrorMessage } from "@/lib/api"
import type { ExecuteRequestDepositCommand } from "../api"
import { useRequestDepositExecutionMutation } from "../hooks/queries"
import type { PaymentRequest, PaymentTaskContext } from "../types"

export function RequestDepositExecution({
    request,
    task,
}: {
    request: PaymentRequest
    task: PaymentTaskContext
}) {
    const profile = useAccountProfileQuery()
    const mutation = useRequestDepositExecutionMutation()
    const [confirmOpen, setConfirmOpen] = useState(false)
    const [unknown, setUnknown] = useState(false)
    const command = useRef<ExecuteRequestDepositCommand | null>(null)
    const canExecute =
        hasPermission(
            profile.data?.permissions,
            "purchase_payment_request:pay",
        ) && task.allowedActions.includes("PROCESS")
    const executable = ["approved", "partially_paid"].includes(request.status)
    return (
        <section className="space-y-4 rounded-lg bg-muted/40 p-5">
            <div className="flex flex-wrap items-center justify-between gap-3">
                <h3 className="font-medium">使用预存款支付</h3>
                {canExecute && (
                    <Button
                        id="ppr-deposit-execute"
                        disabled={
                            !canExecute ||
                            !executable ||
                            mutation.isPending ||
                            mutation.isSuccess
                        }
                        onClick={() => setConfirmOpen(true)}
                    >
                        {unknown ? "刷新处理结果" : "确认使用预存款"}
                    </Button>
                )}
            </div>
            <p className="text-xs text-muted-foreground">
                将从该供应商预存款账户中冻结本次批准的付款额度，供采购单下单扣款使用；待供应商实际扣款核实后自动完成核销。
            </p>
            {mutation.isSuccess && (
                <Alert>
                    <AlertDescription>
                        已冻结 {mutation.data.length}{" "}
                        组预存款额度，待供应商实际扣款核实后自动完成核销。
                    </AlertDescription>
                </Alert>
            )}
            {mutation.isError && (
                <Alert variant="destructive">
                    <AlertDescription>
                        {unknown
                            ? "本次操作结果确认中，请点击「刷新处理结果」核对，不会重复冻结。"
                            : getErrorMessage(
                                  mutation.error,
                                  "余额执行未完成，请核对账户及原采购执行条件。",
                              )}
                    </AlertDescription>
                </Alert>
            )}
            <FormalActionConfirmDialog
                idPrefix="ppr-deposit-execute-confirm"
                open={confirmOpen}
                onOpenChange={setConfirmOpen}
                pending={mutation.isPending}
                title="确认使用预存款"
                actionLabel="使用预存款支付"
                confirmLabel={unknown ? "刷新处理结果" : "确认使用预存款"}
                fromStatus={{ label: "待付款", tone: "neutral" }}
                toStatus={{ label: "余额已占用，待扣款核实", tone: "info" }}
                showStatusChange={false}
                summary={[
                    <span key="request">付款申请：{request.request_no}</span>,
                    <span key="supplier">供应商：{request.supplier_name}</span>,
                    <span key="payer">
                        我方付款主体：{request.our_company_name}
                    </span>,
                    <span key="amount">
                        本申请剩余批准金额{" "}
                        <MoneyValue value={request.unpaid_amount} />
                    </span>,
                ]}
                effects={[
                    "核对采购单与批准额度，冻结预存款账户对应金额（不会重复冻结）",
                    "使用预存款额度支付，不产生新的银行转账记录",
                    "待供应商实际扣款核实后，自动更新付款与核销进度",
                ]}
                confirmDisabled={
                    !canExecute || !executable || mutation.isSuccess
                }
                onConfirm={async () => {
                    command.current ??= {
                        work_item_id: task.workItemId,
                        task_version: task.taskVersion,
                        expected_version: request.version,
                        idempotency_key: `ppr-deposit:${crypto.randomUUID()}`,
                    }
                    try {
                        await mutation.mutateAsync({
                            id: request.id,
                            command: command.current,
                        })
                        setUnknown(false)
                        setConfirmOpen(false)
                    } catch (error) {
                        const uncertain =
                            classifyFormalCommandError(error) === "unknown"
                        setUnknown(uncertain)
                        if (!uncertain) command.current = null
                        throw error
                    }
                }}
            />
        </section>
    )
}
