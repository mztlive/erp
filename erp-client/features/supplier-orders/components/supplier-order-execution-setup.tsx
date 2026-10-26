"use client"

import * as React from "react"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { getErrorMessage } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    useSaveSupplierExecutionSetupMutation,
    useSupplierExecutionSetupQuery,
} from "../hooks/use-execution-setup"
import { SupplierExecutionSetupForm } from "./supplier-order-execution-setup-form"

const STAGE_LABEL: Record<string, string> = {
    QUEUED: "等待接口处理",
    AWAITING_SETUP: "等待补充配送资料",
    PREPARE_ADDRESS: "正在核验配送地址",
    ADDRESS_UNKNOWN: "地址提交结果待查询",
    PLAN_DELIVERY: "正在查询配送方案",
    CREATE: "正在向供应商下单",
    CREATION_UNKNOWN: "下单结果待查询",
    CONFIRM_PAYMENT: "正在确认供应商履约接口",
    PAYMENT_UNKNOWN: "供应商履约确认结果待查询",
    OBSERVE: "订单已提交，持续跟踪中",
    STOPPED: "接口执行已暂停",
}

export function SupplierOrderExecutionSetup({
    orderId,
    canEdit,
    addressSummary,
}: {
    orderId: string
    canEdit: boolean
    addressSummary: string
}) {
    const query = useSupplierExecutionSetupQuery(orderId)
    const mutation = useSaveSupplierExecutionSetupMutation(orderId)
    const [open, setOpen] = React.useState(false)
    const [saved, setSaved] = React.useState(false)
    const setup = query.data
    const prefix = `supplier-order-execution-${toAutomationIdSegment(orderId)}`
    const editable =
        setup?.stage === "AWAITING_SETUP" &&
        (setup.geocode_editable || setup.choice_editable)
    if (query.isPending || (!query.isError && !setup)) return null
    const refresh = async () => {
        const result = await query.refetch()
        if (result.isSuccess) mutation.reset()
    }
    return (
        <>
            <Alert
                variant={setup?.blocker || query.isError ? "warning" : "info"}
            >
                <AlertTitle>
                    {query.isError
                        ? "接口配送资料暂不可查"
                        : (STAGE_LABEL[setup!.stage] ?? "接口执行处理中")}
                </AlertTitle>
                <AlertDescription>
                    {query.isError ? (
                        <p>{getErrorMessage(query.error)}</p>
                    ) : setup?.blocker ? (
                        <p>{setup.blocker}</p>
                    ) : (
                        <p>
                            系统按已批准的采购和配送条件执行，实际结果以订单和物流状态为准。
                        </p>
                    )}
                    {saved ? (
                        <p className="mt-1">
                            配送资料已保存，等待后台继续核验；尚不代表供应商下单成功。
                        </p>
                    ) : null}
                    <div className="mt-2 flex flex-wrap gap-2">
                        {editable && canEdit ? (
                            <Button
                                id={`${prefix}-open`}
                                type="button"
                                variant="outline"
                                size="sm"
                                onClick={() => {
                                    mutation.reset()
                                    setOpen(true)
                                }}
                            >
                                完善接口配送资料
                            </Button>
                        ) : null}
                        {editable && !canEdit ? (
                            <p className="text-xs">
                                补充资料需要供应商订单提交权限。
                            </p>
                        ) : null}
                        <Button
                            id={`${prefix}-refresh`}
                            type="button"
                            variant="ghost"
                            size="sm"
                            disabled={query.isFetching || mutation.isPending}
                            onClick={() => void refresh()}
                        >
                            刷新执行状态
                        </Button>
                    </div>
                </AlertDescription>
            </Alert>
            <Dialog
                open={open}
                onOpenChange={(next) => {
                    if (!mutation.isPending) setOpen(next)
                }}
            >
                <DialogContent
                    closeButtonId={`${prefix}-close`}
                    className="max-h-[90dvh] overflow-y-auto sm:max-w-2xl"
                >
                    <DialogHeader>
                        <DialogTitle>完善接口配送资料</DialogTitle>
                        <DialogDescription>
                            核对当前采购的收件地址，再补充真实地区坐标或选择供应商配送方案。
                        </DialogDescription>
                    </DialogHeader>
                    {setup && editable ? (
                        <SupplierExecutionSetupForm
                            key={setup.execution_version}
                            orderId={orderId}
                            setup={setup}
                            addressSummary={addressSummary}
                            pending={
                                mutation.isPending ||
                                query.isFetching ||
                                Boolean(mutation.error)
                            }
                            error={mutation.error}
                            onCancel={() => setOpen(false)}
                            onSubmit={async (request) => {
                                try {
                                    await mutation.mutateAsync(request)
                                    setSaved(true)
                                    setOpen(false)
                                } catch {
                                    /* 保留输入并展示服务端原因 */
                                }
                            }}
                        />
                    ) : (
                        <p className="text-sm text-muted-foreground">
                            订单执行步骤已变化，请关闭窗口并刷新状态。
                        </p>
                    )}
                    {mutation.error ? (
                        <Button
                            id={`${prefix}-retry-read`}
                            type="button"
                            variant="outline"
                            disabled={query.isFetching}
                            onClick={() => void refresh()}
                        >
                            重新读取配送资料
                        </Button>
                    ) : null}
                </DialogContent>
            </Dialog>
        </>
    )
}
