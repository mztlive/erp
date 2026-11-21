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
import { getErrorMessage, isApiError } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    useSaveSupplierExecutionSetupMutation,
    useSupplierExecutionSetupQuery,
    useQuerySupplierExecutionResultMutation,
} from "../hooks/use-execution-setup"
import { SupplierExecutionSetupForm } from "./supplier-order-execution-setup-form"
import { SupplierOrderFourLevelAddressForm } from "./supplier-order-four-level-address-form"
import { SUPPLIER_EXECUTION_STAGE_LABEL } from "../lib/execution-stage"

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
    const resultQuery = useQuerySupplierExecutionResultMutation(orderId)
    const [open, setOpen] = React.useState(false)
    const [saved, setSaved] = React.useState(false)
    const setup = query.data
    const saveOutcomeUnknown =
        Boolean(mutation.error) &&
        (!isApiError(mutation.error) ||
            ["Network", "Parse", "Unknown"].includes(mutation.error.kind) ||
            (mutation.error.status != null && mutation.error.status >= 500) ||
            Boolean(mutation.error.code?.includes("UNKNOWN")))
    const prefix = `supplier-order-execution-${toAutomationIdSegment(orderId)}`
    const editable =
        setup?.stage === "AWAITING_SETUP" &&
        (setup.geocode_editable ||
            setup.choice_editable ||
            setup.supplier_address_editable)
    const fourLevelAddress = setup?.address_requirement === "FOUR_LEVEL"
    const unknown =
        setup &&
        [
            "CREATION_UNKNOWN",
            "PAYMENT_UNKNOWN",
            "CONFIRMATION_UNKNOWN",
        ].includes(setup.stage)
    if (query.isPending || (!query.isError && !setup)) return null
    const refresh = async () => {
        const result = await query.refetch()
        if (
            result.isSuccess &&
            (!saveOutcomeUnknown ||
                !mutation.variables ||
                result.data?.execution_version !==
                    mutation.variables.expected_version ||
                result.data?.stage !== "AWAITING_SETUP")
        )
            mutation.reset()
    }
    return (
        <>
            <Alert
                variant={setup?.blocker || query.isError ? "warning" : "info"}
            >
                <AlertTitle>
                    {query.isError
                        ? "接口配送资料暂不可查"
                        : (SUPPLIER_EXECUTION_STAGE_LABEL[setup!.stage] ??
                          "接口执行处理中")}
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
                                    if (!saveOutcomeUnknown) mutation.reset()
                                    setOpen(true)
                                }}
                            >
                                完善接口配送资料
                            </Button>
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
                        {setup?.result_query_available && canEdit ? (
                            <Button
                                id={`${prefix}-query-original`}
                                type="button"
                                variant="outline"
                                size="sm"
                                disabled={
                                    !canEdit ||
                                    resultQuery.isPending ||
                                    query.isFetching
                                }
                                onClick={() =>
                                    void resultQuery
                                        .mutateAsync(setup.execution_version)
                                        .catch(() => undefined)
                                }
                            >
                                {resultQuery.isPending
                                    ? "正在提交查询…"
                                    : "查询原结果"}
                            </Button>
                        ) : null}
                    </div>

                    {unknown ? (
                        <p className="mt-2 text-xs">
                            原请求可能已送达，暂未查到或尚未确认不代表可以重新发送。
                        </p>
                    ) : null}
                    {resultQuery.error ? (
                        <p
                            role="alert"
                            className="mt-2 text-body-compact text-destructive"
                        >
                            {getErrorMessage(resultQuery.error)}
                        </p>
                    ) : null}
                    {resultQuery.isSuccess ? (
                        <p className="mt-2 text-xs">
                            已安排查询原请求，实际结果以随后核实的订单进度为准。
                        </p>
                    ) : null}
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
                            {fourLevelAddress
                                ? "核对当前采购的完整收货地址，从供应商候选中确认四级区域或配送方案。"
                                : "核对当前采购的收件地址，再补充真实地区坐标或选择供应商配送方案。"}
                        </DialogDescription>
                    </DialogHeader>
                    {setup &&
                    editable &&
                    setup.supplier_address_editable &&
                    fourLevelAddress ? (
                        <SupplierOrderFourLevelAddressForm
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
                                    /* 原保存结果先查证 */
                                }
                            }}
                        />
                    ) : setup && editable ? (
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
                        <p className="text-xs text-muted-foreground">
                            订单执行步骤已变化，请关闭窗口并刷新状态。
                        </p>
                    )}
                    {mutation.error ? (
                        <div className="space-y-3">
                            {saveOutcomeUnknown && mutation.variables ? (
                                <>
                                    <p className="text-body-compact text-warning-soft-foreground">
                                        上次保存结果确认中，已保留您填写的配送资料，请点击「重试保存」确认结果。
                                    </p>
                                    <Button
                                        id={`${prefix}-retry-original-save`}
                                        type="button"
                                        variant="outline"
                                        disabled={
                                            mutation.isPending ||
                                            query.isFetching
                                        }
                                        onClick={() => {
                                            if (!mutation.variables) return
                                            void mutation
                                                .mutateAsync(mutation.variables)
                                                .then(() => {
                                                    setSaved(true)
                                                    setOpen(false)
                                                })
                                                .catch(() => undefined)
                                        }}
                                    >
                                        重试保存
                                    </Button>
                                </>
                            ) : null}
                            <Button
                                id={`${prefix}-retry-read`}
                                type="button"
                                variant="outline"
                                disabled={query.isFetching}
                                onClick={() => void refresh()}
                            >
                                重新读取配送资料
                            </Button>
                        </div>
                    ) : null}
                </DialogContent>
            </Dialog>
        </>
    )
}
