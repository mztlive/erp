"use client"

import * as React from "react"
import { z } from "zod"

import { useAppForm } from "@/components/form"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogClose,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { FieldGroup } from "@/components/ui/field"
import { DialogScrollBody } from "@/features/master-data/components/shared/action-dialog-shared"
import { FixedResourceDisableDialog } from "@/features/master-data/components/shared/disable-action-dialog"
import {
    useUpdateWarehouseFulfillmentHandlersMutation,
    useWarehouseFulfillmentHandlerOptionsQuery,
} from "@/features/master-data/hooks/queries"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import { WAREHOUSE_WRITE_MESSAGE } from "@/features/master-data/lib/data"
import type { RevisionTarget } from "@/features/master-data/lib/revision-target"
import { revisionTargetIds } from "@/features/master-data/lib/revision-target"
import { getErrorMessage } from "@/lib/api/errors"

const warehouseHandlersSchema = z.object({
    inboundUserId: z.string().min(1, "请选择入库经办人"),
    outboundUserId: z.string().min(1, "请选择仓发经办人"),
})

export function WarehouseDisableDialog({
    open,
    onOpenChange,
    target,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    target: RevisionTarget | null
}) {
    const stockNote =
        target &&
        "warehouseStockSummary" in target &&
        target.warehouseStockSummary?.hasBlockingStock
            ? ` 另：在库 ${target.warehouseStockSummary.onHandQty} / 预占 ${target.warehouseStockSummary.reservedQty} 时也不可停用。`
            : ""
    return (
        <FixedResourceDisableDialog
            resource="warehouses"
            open={open}
            onOpenChange={onOpenChange}
            target={target}
            submitDisabled
            submitLabel={masterDataCopy.createSubmitRejected}
            blockedBanner={
                <Alert variant="destructive">
                    <AlertTitle>
                        {masterDataCopy.warehouseWriteTitle}
                    </AlertTitle>
                    <AlertDescription>
                        {WAREHOUSE_WRITE_MESSAGE}
                        {stockNote}
                    </AlertDescription>
                </Alert>
            }
        />
    )
}

/** 配置仓库入库与仓发经办人；只影响之后新建的履约任务。 */
export function WarehouseReviseDialog({
    open,
    onOpenChange,
    target,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    target: RevisionTarget | null
}) {
    const identity = revisionTargetIds(target)
    const handlers =
        target && "warehouseFulfillmentHandlers" in target
            ? target.warehouseFulfillmentHandlers
            : undefined
    const [error, setError] = React.useState<string | null>(null)
    const optionsQuery = useWarehouseFulfillmentHandlerOptionsQuery(open)
    const updateMutation = useUpdateWarehouseFulfillmentHandlersMutation()

    const inboundOptions = React.useMemo(
        () =>
            (optionsQuery.data ?? [])
                .filter((option) => option.inbound_eligible)
                .map((option) => ({
                    value: option.user_id,
                    label: `${option.display_name} · ${option.account}`,
                    keywords: option.account,
                })),
        [optionsQuery.data],
    )
    const outboundOptions = React.useMemo(
        () =>
            (optionsQuery.data ?? [])
                .filter((option) => option.outbound_eligible)
                .map((option) => ({
                    value: option.user_id,
                    label: `${option.display_name} · ${option.account}`,
                    keywords: option.account,
                })),
        [optionsQuery.data],
    )
    const form = useAppForm({
        defaultValues: {
            inboundUserId: handlers?.inboundUserId ?? "",
            outboundUserId: handlers?.outboundUserId ?? "",
        },
        validators: {
            onChange: warehouseHandlersSchema,
        },
        onSubmit: async ({ value }) => {
            const inboundEligible = inboundOptions.some(
                (option) => option.value === value.inboundUserId,
            )
            const outboundEligible = outboundOptions.some(
                (option) => option.value === value.outboundUserId,
            )
            if (!target || !inboundEligible || !outboundEligible) {
                setError("请分别选择当前仍具备完整操作权限的入库与仓发经办人")
                return
            }
            try {
                await updateMutation.mutateAsync({
                    warehouseId: identity.stableId,
                    version: identity.lockVersion,
                    inboundHandlerUserId: value.inboundUserId,
                    outboundHandlerUserId: value.outboundUserId,
                })
                onOpenChange(false)
            } catch (cause) {
                setError(getErrorMessage(cause, "收发责任未更新，请稍后重试。"))
            }
        },
    })

    // oxlint-disable-next-line react/set-state-in-effect -- reset form when dialog opens with new target
    React.useEffect(() => {
        if (!open) return
        form.reset({
            inboundUserId: handlers?.inboundUserId ?? "",
            outboundUserId: handlers?.outboundUserId ?? "",
        })
        setError(null)
    }, [
        form,
        handlers?.inboundUserId,
        handlers?.outboundUserId,
        identity.stableId,
        open,
    ])

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent className="flex max-h-[92vh] w-full flex-col gap-4 overflow-hidden sm:max-w-lg">
                <DialogHeader>
                    <DialogTitle>配置收发责任</DialogTitle>
                    <DialogDescription>
                        仓库 <span className="num">{identity.stableNo}</span>
                        ；配置保存后只用于新建的入库与仓发任务。
                    </DialogDescription>
                </DialogHeader>
                <form
                    className="flex min-h-0 flex-1 flex-col"
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    <DialogScrollBody>
                        {optionsQuery.isError ? (
                            <Alert variant="destructive">
                                <AlertTitle>经办人选项加载失败</AlertTitle>
                                <AlertDescription>
                                    {getErrorMessage(
                                        optionsQuery.error,
                                        "请稍后重试",
                                    )}
                                </AlertDescription>
                            </Alert>
                        ) : null}
                        {error ? (
                            <Alert variant="destructive">
                                <AlertTitle>没有保存</AlertTitle>
                                <AlertDescription>{error}</AlertDescription>
                            </Alert>
                        ) : null}
                        <FieldGroup>
                            <form.AppField name="inboundUserId">
                                {(field) => (
                                    <field.SelectField
                                        id="master-data-warehouse-handler-inbound-combobox"
                                        label="入库经办人"
                                        options={inboundOptions}
                                        placeholder="选择具备入库确认权限的账号"
                                        description="负责该仓库的采购收货与入库过账。"
                                        loading={optionsQuery.isPending}
                                        disabled={optionsQuery.isError}
                                        allowClear={false}
                                        required
                                    />
                                )}
                            </form.AppField>
                            <form.AppField name="outboundUserId">
                                {(field) => (
                                    <field.SelectField
                                        id="master-data-warehouse-handler-outbound-combobox"
                                        label="仓发经办人"
                                        options={outboundOptions}
                                        placeholder="选择具备发货确认权限的账号"
                                        description="负责该仓库的现货发货与入库后发货；可与入库经办人为同一人。"
                                        loading={optionsQuery.isPending}
                                        disabled={optionsQuery.isError}
                                        allowClear={false}
                                        required
                                    />
                                )}
                            </form.AppField>
                        </FieldGroup>
                        <DialogFooter>
                            <DialogClose
                                render={
                                    <Button
                                        id="master-data-warehouse-handler-cancel"
                                        type="button"
                                        variant="outline"
                                    />
                                }
                            >
                                取消
                            </DialogClose>
                            <form.Subscribe
                                selector={(state) =>
                                    [
                                        state.values.inboundUserId,
                                        state.values.outboundUserId,
                                    ] as const
                                }
                            >
                                {([inboundUserId, outboundUserId]) => (
                                    <form.AppForm>
                                        <form.SubmitButton
                                            id="master-data-warehouse-handler-save"
                                            label="保存配置"
                                            pendingLabel="保存中…"
                                            disabled={
                                                updateMutation.isPending ||
                                                optionsQuery.isPending ||
                                                optionsQuery.isError ||
                                                !inboundOptions.some(
                                                    (option) =>
                                                        option.value ===
                                                        inboundUserId,
                                                ) ||
                                                !outboundOptions.some(
                                                    (option) =>
                                                        option.value ===
                                                        outboundUserId,
                                                )
                                            }
                                        />
                                    </form.AppForm>
                                )}
                            </form.Subscribe>
                        </DialogFooter>
                    </DialogScrollBody>
                </form>
            </DialogContent>
        </Dialog>
    )
}
