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
                <h3 className="font-medium">供应商预存款执行</h3>
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
                        {unknown ? "查询原余额执行结果" : "执行本申请余额占用"}
                    </Button>
                )}
            </div>
            <p className="text-sm text-muted-foreground">
                使用本申请批准的同主体、同供应商预存款账户。系统按原采购执行组及批准预算办理专属占用；占用不计入已付，真实扣款核实后才更新核销和任务进度。
            </p>
            <p className="text-sm text-muted-foreground">
                本次人工执行仅覆盖本申请及其原执行组，不授予账户后续自动执行权限。
            </p>
            {mutation.isSuccess && (
                <Alert>
                    <AlertDescription>
                        已办理 {mutation.data.length}{" "}
                        组资金占用，等待供应商执行及真实扣款核实。付款任务将在实际核销后更新。
                    </AlertDescription>
                </Alert>
            )}
            {mutation.isError && (
                <Alert variant="destructive">
                    <AlertDescription>
                        {unknown
                            ? "本次执行结果尚未查明，请查询原结果。原金额、付款申请和任务资料保持锁定，不新建执行。"
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
                title="执行供应商预存款"
                actionLabel="办理余额占用"
                confirmLabel={unknown ? "查询原执行结果" : "确认按批准范围执行"}
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
                    "服务器逐组重验采购修订、地址及批准预算，同一批准额度只占用一次。",
                    "已存在的原占用沿原记录恢复，不生成新的银行付款。",
                    "资金占用不替代真实扣款核销；已发送或未知订单不会因超时释放。",
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
