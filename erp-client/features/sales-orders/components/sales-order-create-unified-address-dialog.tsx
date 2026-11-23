"use client"

import * as React from "react"
import { MapPinIcon } from "lucide-react"
import { z } from "zod"

import { QuantityValue, detailWorkspaceStyles } from "@/components/business"
import { toFieldErrors, useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogBody,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { RegionName } from "@/features/regions"
import type { UnifiedDeliveryAddress } from "@/features/sales-orders/lib/sales-order-create-model"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { DeliveryAddressGroup } from "../lib/sales-delivery-pool"
import { DeliveryRegionField } from "./delivery-region-field"
import { SalesDeliverySmartPaste } from "./sales-delivery-smart-paste"

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
    /** all：应用到全部商品（默认，保留原有自动化 ID）；selected：应用到已勾选商品。 */
    mode?: "all" | "selected"
    lineCount: number
    /** 已有地址的商品数，用于提示覆盖范围。 */
    allocatedLineCount: number
    /** 打开时带入的地址，通常取第一个已填写的地址。 */
    initialAddress?: UnifiedDeliveryAddress
    /** 本单已有地址池，支持一键复用免重填。 */
    existingAddresses?: readonly DeliveryAddressGroup[]
    triggerLabel?: string
    /** 受控打开状态（用于从批量勾选栏直接打开）。 */
    open?: boolean
    onOpenChange?: (open: boolean) => void
    hideTrigger?: boolean
    onApply: (address: UnifiedDeliveryAddress) => void
}

/** 一次填写收货资料，应用到全部或已勾选商品；内置智能粘贴识别与本单已有地址快捷复用。 */
export function SalesOrderCreateUnifiedAddressDialog({
    mode = "all",
    lineCount,
    allocatedLineCount,
    initialAddress,
    existingAddresses = [],
    triggerLabel = "设置统一地址",
    open: controlledOpen,
    onOpenChange,
    hideTrigger = false,
    onApply,
}: SalesOrderCreateUnifiedAddressDialogProps) {
    const [uncontrolledOpen, setUncontrolledOpen] = React.useState(false)
    const open = controlledOpen ?? uncontrolledOpen
    const setOpen = onOpenChange ?? setUncontrolledOpen
    const idPrefix =
        mode === "selected"
            ? "sales-orders-create-batch-address"
            : "sales-orders-create-unified-address"

    const form = useAppForm({
        defaultValues: EMPTY_ADDRESS,
        validators: { onChange: unifiedAddressSchema },
        onSubmit: ({ value }) => {
            onApply(value)
            setOpen(false)
        },
    })

    // 受控模式打开时同步带入初始地址。
    const prevOpenRef = React.useRef(false)
    React.useEffect(() => {
        if (open && !prevOpenRef.current) {
            form.reset({
                ...EMPTY_ADDRESS,
                ...(initialAddress
                    ? {
                          recipient: initialAddress.recipient,
                          phone: initialAddress.phone,
                          region_code: initialAddress.region_code,
                          address: initialAddress.address,
                      }
                    : {}),
            })
        }
        prevOpenRef.current = open
    }, [form, initialAddress, open])

    const fillFromAddress = (address: Partial<UnifiedDeliveryAddress>) => {
        if (address.recipient !== undefined) {
            form.setFieldValue("recipient", address.recipient)
        }
        if (address.phone !== undefined) {
            form.setFieldValue("phone", address.phone)
        }
        if (address.region_code !== undefined) {
            form.setFieldValue("region_code", address.region_code)
        }
        if (address.address !== undefined) {
            form.setFieldValue("address", address.address)
        }
    }

    const title = mode === "selected" ? "批量设置收货地址" : "设置统一收货地址"
    const submitLabel =
        mode === "selected"
            ? `应用到已选 ${lineCount} 项商品`
            : "应用到全部商品"

    return (
        <>
            {!hideTrigger ? (
                <Button
                    id={`${idPrefix}-open`}
                    type="button"
                    variant="ghost"
                    size="sm"
                    className={detailWorkspaceStyles.subtleButton}
                    disabled={lineCount === 0}
                    onClick={() => setOpen(true)}
                >
                    <MapPinIcon
                        data-icon="inline-start"
                        className="size-3.5"
                        aria-hidden="true"
                    />
                    {triggerLabel}
                </Button>
            ) : null}
            <Dialog open={open} onOpenChange={setOpen}>
                <DialogContent
                    size="lg"
                    scroll="body"
                    closeButtonId={`${idPrefix}-close`}
                >
                    <DialogHeader>
                        <DialogTitle>{title}</DialogTitle>
                        <DialogDescription>
                            {mode === "selected" ? "已选 " : "全部 "}
                            <QuantityValue
                                unit=""
                                value={String(lineCount)}
                            />{" "}
                            项商品将按各自销售数量送到这个地址。
                            {allocatedLineCount > 0 ? (
                                <>
                                    将覆盖其中{" "}
                                    <QuantityValue
                                        unit=""
                                        value={String(allocatedLineCount)}
                                    />{" "}
                                    项商品已填写的地址。
                                </>
                            ) : null}
                            应用后仍可针对单个商品拆分为多个地址。
                        </DialogDescription>
                    </DialogHeader>
                    <form
                        className="flex min-h-0 flex-1 flex-col overflow-hidden"
                        noValidate
                        onSubmit={(event) => {
                            // 弹窗渲染在开单表单之外的节点，但 React 事件仍会冒泡到开单表单。
                            event.preventDefault()
                            event.stopPropagation()
                            void form.handleSubmit()
                        }}
                    >
                        <DialogBody className="space-y-4">
                            {existingAddresses.length > 0 ? (
                                <div className="space-y-1.5">
                                    <p className="text-xs font-medium text-foreground/75">
                                        复用本单已有地址
                                    </p>
                                    <div className="flex flex-wrap gap-1.5">
                                        {existingAddresses
                                            .slice(0, 6)
                                            .map((group) => (
                                                <Button
                                                    key={group.key}
                                                    id={`${idPrefix}-reuse-${toAutomationIdSegment(group.key).slice(0, 32)}`}
                                                    type="button"
                                                    variant="outline"
                                                    size="xs"
                                                    className="h-auto max-w-full py-1 text-left font-normal"
                                                    onClick={() =>
                                                        fillFromAddress({
                                                            recipient:
                                                                group.recipient,
                                                            phone: group.phone,
                                                            region_code:
                                                                group.region_code,
                                                            address:
                                                                group.address,
                                                        })
                                                    }
                                                >
                                                    <MapPinIcon
                                                        aria-hidden="true"
                                                        className="size-3 shrink-0 text-muted-foreground"
                                                    />
                                                    <span className="truncate">
                                                        {group.recipient} ·{" "}
                                                        <RegionName
                                                            code={
                                                                group.region_code
                                                            }
                                                        />
                                                        {group.address}
                                                    </span>
                                                </Button>
                                            ))}
                                    </div>
                                </div>
                            ) : null}

                            <SalesDeliverySmartPaste
                                idPrefix={idPrefix}
                                onParsed={(parsed) => {
                                    fillFromAddress({
                                        ...(parsed.recipient
                                            ? { recipient: parsed.recipient }
                                            : {}),
                                        ...(parsed.phone
                                            ? { phone: parsed.phone }
                                            : {}),
                                        ...(parsed.region_code
                                            ? {
                                                  region_code:
                                                      parsed.region_code,
                                              }
                                            : {}),
                                        ...(parsed.address
                                            ? { address: parsed.address }
                                            : {}),
                                    })
                                }}
                            />

                            <div className="grid gap-4 sm:grid-cols-2">
                                <form.AppField name="recipient">
                                    {(field) => (
                                        <field.TextField
                                            id={`${idPrefix}-recipient`}
                                            label="收货人"
                                            required
                                        />
                                    )}
                                </form.AppField>
                                <form.AppField name="phone">
                                    {(field) => (
                                        <field.TextField
                                            id={`${idPrefix}-phone`}
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
                                        id={`${idPrefix}-region`}
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
                                        id={`${idPrefix}-detail`}
                                        label="详细地址"
                                        required
                                        placeholder="街道、门牌号"
                                    />
                                )}
                            </form.AppField>
                        </DialogBody>
                        <DialogFooter>
                            <Button
                                id={`${idPrefix}-cancel`}
                                type="button"
                                variant="outline"
                                onClick={() => setOpen(false)}
                            >
                                取消
                            </Button>
                            <form.AppForm>
                                <form.SubmitButton
                                    id={`${idPrefix}-apply`}
                                    label={submitLabel}
                                />
                            </form.AppForm>
                        </DialogFooter>
                    </form>
                </DialogContent>
            </Dialog>
        </>
    )
}
