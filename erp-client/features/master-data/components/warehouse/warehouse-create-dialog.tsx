"use client"

import { useState } from "react"
import { z } from "zod"

import { DiscardConfirmDialog } from "@/components/business"
import { FormError, useAppForm } from "@/components/form"
import { useFieldA11y } from "@/components/form/field-a11y"
import { FieldFrame } from "@/components/form/field-frame"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogBody,
    DialogClose,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { FieldGroup } from "@/components/ui/field"
import { toast } from "@/components/ui/toast"
import type { CreateWarehouseInput } from "@/features/master-data/api/mutations/warehouse"
import { WarehouseHandlerField } from "@/features/master-data/components/warehouse/warehouse-handler-field"
import {
    useCreateWarehouseMutation,
    useWarehouseFulfillmentHandlerOptionsQuery,
} from "@/features/master-data/hooks/queries"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import { RegionCascadeSelect } from "@/features/regions"
import { getErrorMessage } from "@/lib/api/errors"
import { listFilterText } from "@/lib/ui-text"

const PREFIX = "master-data-warehouse-create-dialog"

const warehouseSchema = z.object({
    warehouseCode: z.string().trim().min(1, "请输入仓库代码"),
    name: z.string().trim().min(1, "请输入仓库名称"),
    regionCode: z.string().min(1, "请选择仓库所在地区"),
    address: z.string().trim().min(1, "请输入仓库详细地址"),
    contact: z.string().trim().min(1, "请输入仓库联系人"),
    inboundHandlerUserId: z.string().min(1, "请选择入库经办人"),
    outboundHandlerUserId: z.string().min(1, "请选择仓发经办人"),
    changeReason: z.string().trim().min(1, "请填写创建说明"),
})

const defaultValues: CreateWarehouseInput = {
    warehouseCode: "",
    name: "",
    regionCode: "",
    address: "",
    contact: "",
    inboundHandlerUserId: "",
    outboundHandlerUserId: "",
    changeReason: "",
}

function WarehouseRegionField({ disabled }: { disabled: boolean }) {
    const id = `${PREFIX}-region`
    const { field, isInvalid, errors, errorId, descriptionId, describedBy } =
        useFieldA11y<string>(undefined, id)

    return (
        <FieldFrame
            label="所在地区"
            htmlFor={`${id}-province`}
            required
            isInvalid={isInvalid}
            errors={errors}
            errorId={errorId}
            descriptionId={descriptionId}
        >
            <RegionCascadeSelect
                idPrefix={id}
                leafOnly
                disabled={disabled}
                value={field.state.value}
                onValueChange={field.handleChange}
                onBlur={field.handleBlur}
                invalid={isInvalid}
                ariaLabel="所在地区"
                ariaDescribedBy={describedBy || undefined}
            />
        </FieldFrame>
    )
}

export function WarehouseCreateDialog({
    open,
    onOpenChange,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
}) {
    const [discardOpen, setDiscardOpen] = useState(false)
    const mutation = useCreateWarehouseMutation()
    const options = useWarehouseFulfillmentHandlerOptionsQuery(open, "create")
    const inboundOptions = options.data?.inbound ?? []
    const outboundOptions = options.data?.outbound ?? []
    const pending = mutation.isPending
    const handlerOptionsReady = options.isSuccess && !options.isFetching
    const refreshOptions = () => options.refetch({ throwOnError: true })

    const form = useAppForm({
        defaultValues,
        validators: { onChange: warehouseSchema },
        onSubmit: async ({ value }) => {
            if (!handlerOptionsReady) return
            if (
                !inboundOptions.some(
                    (option) => option.value === value.inboundHandlerUserId,
                ) ||
                !outboundOptions.some(
                    (option) => option.value === value.outboundHandlerUserId,
                )
            ) {
                throw new Error("请重新选择具备资格的入库与仓发经办人。")
            }
            const warehouse = await mutation.mutateAsync(value)
            toast.add({
                title: masterDataCopy.createSuccessTitle,
                description: `仓库 ${warehouse.warehouse_code} 已建立。`,
                type: "success",
            })
            close()
        },
    })

    const close = () => {
        setDiscardOpen(false)
        form.reset()
        onOpenChange(false)
    }

    const handlerFieldState = {
        onRefresh: refreshOptions,
        loading: options.isFetching,
        disabled: options.isPending || pending,
    }

    return (
        <>
            <Dialog
                open={open}
                onOpenChange={(next) => {
                    if (next || form.state.isSubmitting) return
                    if (!form.state.isDefaultValue) setDiscardOpen(true)
                    else close()
                }}
            >
                <DialogContent
                    size="lg"
                    scroll="body"
                    closeButtonId={`${PREFIX}-close`}
                    showCloseButton={!pending}
                >
                    <DialogHeader>
                        <DialogTitle>
                            {masterDataCopy.createTitle("仓库")}
                        </DialogTitle>
                        <DialogDescription>
                            保存后仓库即启用。
                        </DialogDescription>
                    </DialogHeader>
                    <form
                        className="flex min-h-0 flex-1 flex-col overflow-hidden"
                        onSubmit={(event) => {
                            event.preventDefault()
                            void form.handleSubmit().catch(() => undefined)
                        }}
                    >
                        <DialogBody>
                            <FieldGroup>
                                <form.AppForm>
                                    <FormError />
                                </form.AppForm>
                                <div className="grid gap-4 sm:grid-cols-2">
                                    <form.AppField name="warehouseCode">
                                        {(field) => (
                                            <field.TextField
                                                id={`${PREFIX}-code`}
                                                label="仓库代码"
                                                description="创建后不可修改。"
                                                inputClassName="num"
                                                disabled={pending}
                                                required
                                            />
                                        )}
                                    </form.AppField>
                                    <form.AppField name="name">
                                        {(field) => (
                                            <field.TextField
                                                id={`${PREFIX}-name`}
                                                label="仓库名称"
                                                disabled={pending}
                                                required
                                            />
                                        )}
                                    </form.AppField>
                                </div>
                                <form.AppField name="regionCode">
                                    {() => (
                                        <WarehouseRegionField
                                            disabled={pending}
                                        />
                                    )}
                                </form.AppField>
                                <form.AppField name="address">
                                    {(field) => (
                                        <field.TextField
                                            id={`${PREFIX}-address`}
                                            label="详细地址"
                                            disabled={pending}
                                            required
                                        />
                                    )}
                                </form.AppField>
                                <form.AppField name="contact">
                                    {(field) => (
                                        <field.TextField
                                            id={`${PREFIX}-contact`}
                                            label="仓库联系人"
                                            disabled={pending}
                                            required
                                        />
                                    )}
                                </form.AppField>
                                {options.isError ? (
                                    <Alert variant="destructive">
                                        <AlertTitle>
                                            经办人选项加载失败
                                        </AlertTitle>
                                        <AlertDescription>
                                            {getErrorMessage(
                                                options.error,
                                                "请刷新选项后重试。",
                                            )}
                                        </AlertDescription>
                                    </Alert>
                                ) : null}
                                {handlerOptionsReady &&
                                (!inboundOptions.length ||
                                    !outboundOptions.length) ? (
                                    <Alert>
                                        <AlertTitle>经办人候选不足</AlertTitle>
                                        <AlertDescription>
                                            请联系管理员配置对应的履约权限，再刷新经办人选项。
                                        </AlertDescription>
                                    </Alert>
                                ) : null}
                                <div className="grid gap-4 sm:grid-cols-2">
                                    <form.AppField name="inboundHandlerUserId">
                                        {() => (
                                            <WarehouseHandlerField
                                                {...handlerFieldState}
                                                id={`${PREFIX}-inbound`}
                                                duty="inbound"
                                                options={inboundOptions}
                                            />
                                        )}
                                    </form.AppField>
                                    <form.AppField name="outboundHandlerUserId">
                                        {() => (
                                            <WarehouseHandlerField
                                                {...handlerFieldState}
                                                id={`${PREFIX}-outbound`}
                                                duty="outbound"
                                                options={outboundOptions}
                                            />
                                        )}
                                    </form.AppField>
                                </div>
                                <form.AppField name="changeReason">
                                    {(field) => (
                                        <field.TextareaField
                                            id={`${PREFIX}-reason`}
                                            label="创建说明"
                                            rows={2}
                                            disabled={pending}
                                            required
                                        />
                                    )}
                                </form.AppField>
                            </FieldGroup>
                        </DialogBody>
                        <DialogFooter>
                            <DialogClose
                                render={
                                    <Button
                                        id={`${PREFIX}-cancel`}
                                        type="button"
                                        variant="outline"
                                        disabled={pending}
                                    />
                                }
                            >
                                {listFilterText.cancel}
                            </DialogClose>
                            <form.AppForm>
                                <form.SubmitButton
                                    id={`${PREFIX}-save`}
                                    label={masterDataCopy.createSubmit}
                                    pendingLabel="保存中…"
                                    disabled={
                                        !handlerOptionsReady ||
                                        !inboundOptions.length ||
                                        !outboundOptions.length
                                    }
                                    loading={pending}
                                    showError={false}
                                />
                            </form.AppForm>
                        </DialogFooter>
                    </form>
                </DialogContent>
            </Dialog>
            <DiscardConfirmDialog
                idPrefix={`${PREFIX}-discard`}
                open={discardOpen}
                onOpenChange={setDiscardOpen}
                onConfirm={close}
            />
        </>
    )
}
