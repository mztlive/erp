"use client"

import * as React from "react"
import { MapPinIcon } from "lucide-react"
import { z } from "zod"

import { QuantityValue } from "@/components/business"
import { toFieldErrors, useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import type { UnifiedDeliveryAddress } from "@/features/sales-orders/lib/sales-order-create-model"
import { DeliveryRegionField } from "./delivery-region-field"

const unifiedAddressSchema = z.object({
    recipient: z
        .string()
        .trim()
        .min(1, "请填写收货人")
        .max(64, "收货人最多 64 个字"),
    phone: z
        .string()
        .trim()
        .min(1, "请填写联系电话")
        .max(32, "联系电话最多 32 个字符"),
    region_code: z.string().min(1, "请选择收货地区（到区县）"),
    address: z
        .string()
        .trim()
        .min(1, "请填写详细地址")
        .max(512, "详细地址最多 512 个字"),
})

const EMPTY_ADDRESS: UnifiedDeliveryAddress = {
    recipient: "",
    phone: "",
    region_code: "",
    address: "",
}

export type SalesOrderCreateUnifiedAddressDialogProps = {
    lineCount: number
    /** 已有地址的商品数，用于提示覆盖范围。 */
    allocatedLineCount: number
    /** 打开时带入的地址，通常取第一个已填写的地址。 */
    initialAddress?: UnifiedDeliveryAddress
    triggerLabel?: string
    onApply: (address: UnifiedDeliveryAddress) => void
}

/** 一次填写收货资料，应用到全部商品；之后仍可逐个商品改为分地址。 */
export function SalesOrderCreateUnifiedAddressDialog({
    lineCount,
    allocatedLineCount,
    initialAddress,
    triggerLabel = "设置统一地址",
    onApply,
}: SalesOrderCreateUnifiedAddressDialogProps) {
    const [open, setOpen] = React.useState(false)
    const form = useAppForm({
        defaultValues: EMPTY_ADDRESS,
        validators: { onChange: unifiedAddressSchema },
        onSubmit: ({ value }) => {
            onApply(value)
            setOpen(false)
        },
    })

    return (
        <>
            <Button
                id="sales-orders-create-unified-address-open"
                type="button"
                variant="outline"
                size="sm"
                disabled={lineCount === 0}
                onClick={() => {
                    form.reset({ ...EMPTY_ADDRESS, ...initialAddress })
                    setOpen(true)
                }}
            >
                <MapPinIcon aria-hidden="true" />
                {triggerLabel}
            </Button>
            <Dialog open={open} onOpenChange={setOpen}>
                <DialogContent
                    className="sm:max-w-xl"
                    closeButtonId="sales-orders-create-unified-address-close"
                >
                    <DialogHeader>
                        <DialogTitle>设置统一收货地址</DialogTitle>
                        <DialogDescription>
                            全部{" "}
                            <QuantityValue unit="" value={String(lineCount)} />{" "}
                            个商品按各自数量送到这个地址。
                            {allocatedLineCount > 0 ? (
                                <>
                                    将覆盖{" "}
                                    <QuantityValue
                                        unit=""
                                        value={String(allocatedLineCount)}
                                    />{" "}
                                    个商品已填写的地址。
                                </>
                            ) : null}
                            应用后仍可逐个商品添加地址、改为分地址交付。
                        </DialogDescription>
                    </DialogHeader>
                    <form
                        className="flex flex-col gap-4"
                        noValidate
                        onSubmit={(event) => {
                            // 弹窗渲染在开单表单之外的节点，但 React 事件仍会冒泡到开单表单。
                            event.preventDefault()
                            event.stopPropagation()
                            void form.handleSubmit()
                        }}
                    >
                        <div className="grid gap-4 sm:grid-cols-2">
                            <form.AppField name="recipient">
                                {(field) => (
                                    <field.TextField
                                        id="sales-orders-create-unified-address-recipient"
                                        label="收货人"
                                        required
                                    />
                                )}
                            </form.AppField>
                            <form.AppField name="phone">
                                {(field) => (
                                    <field.TextField
                                        id="sales-orders-create-unified-address-phone"
                                        label="联系电话"
                                        type="tel"
                                        required
                                    />
                                )}
                            </form.AppField>
                        </div>
                        <form.AppField name="region_code">
                            {(field) => (
                                <DeliveryRegionField
                                    id="sales-orders-create-unified-address-region"
                                    required
                                    value={field.state.value}
                                    onValueChange={(code) =>
                                        field.handleChange(code)
                                    }
                                    onBlur={field.handleBlur}
                                    invalid={
                                        field.state.meta.isTouched &&
                                        !field.state.meta.isValid
                                    }
                                    errors={toFieldErrors(
                                        field.state.meta.errors,
                                    )}
                                />
                            )}
                        </form.AppField>
                        <form.AppField name="address">
                            {(field) => (
                                <field.TextField
                                    id="sales-orders-create-unified-address-detail"
                                    label="详细地址"
                                    required
                                    placeholder="街道、门牌号"
                                />
                            )}
                        </form.AppField>
                        <DialogFooter>
                            <Button
                                id="sales-orders-create-unified-address-cancel"
                                type="button"
                                variant="ghost"
                                onClick={() => setOpen(false)}
                            >
                                取消
                            </Button>
                            <form.AppForm>
                                <form.SubmitButton
                                    id="sales-orders-create-unified-address-apply"
                                    label="应用到全部商品"
                                />
                            </form.AppForm>
                        </DialogFooter>
                    </form>
                </DialogContent>
            </Dialog>
        </>
    )
}
