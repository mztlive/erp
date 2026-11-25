"use client"

import { useState } from "react"
import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import {
    Dialog,
    DialogBody,
    DialogContent,
    DialogHeader,
    DialogTitle,
    DialogDescription,
    DialogFooter,
} from "@/components/ui/dialog"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { packingQuantity, splitPackage } from "../../lib/packing-allocation"
import type {
    DeliveryPacking,
    DeliveryPackage,
    PackingItem,
} from "../../lib/delivery-packing"

/** 拆分作为独立事务式编辑；确认前不修改原包裹，也不创建空包裹。 */
export function FulfillmentPackageSplit({
    packing,
    parcel,
    label,
    itemName,
    unit,
    disabled,
    onApply,
    onClose,
}: {
    packing: DeliveryPacking
    parcel: DeliveryPackage
    label: string
    itemName: (id: string) => string
    unit: (id: string) => string
    disabled?: boolean
    onApply: (packing: DeliveryPacking) => void
    onClose: () => void
}) {
    const [search, setSearch] = useState("")
    const matches = (id: string) =>
        itemName(id)
            .toLocaleLowerCase()
            .includes(search.trim().toLocaleLowerCase())
    const error = (items: PackingItem[]) => {
        try {
            splitPackage(packing, parcel.id, items, "preview")
            return undefined
        } catch (reason) {
            return reason instanceof Error &&
                !reason.message.includes("DECIMAL")
                ? reason.message
                : "请填写有效数量，最多6位小数"
        }
    }
    const form = useAppForm({
        defaultValues: {
            items: parcel.items.map((item) => ({ ...item, quantity: "0" })),
        },
        validators: {
            onChange: ({ value }) => error(value.items),
            onSubmit: ({ value }) => error(value.items),
        },
        onSubmit: ({ value }) => {
            if (disabled) return
            onApply(
                splitPackage(
                    packing,
                    parcel.id,
                    value.items,
                    crypto.randomUUID(),
                ),
            )
            onClose()
        },
    })
    const prefix = `fulfillment-packing-split-${toAutomationIdSegment(parcel.id)}`
    return (
        <Dialog
            open
            onOpenChange={(open) => {
                if (!open) onClose()
            }}
        >
            <DialogContent
                size="md"
                scroll="body"
                closeButtonId={`${prefix}-close`}
            >
                <DialogHeader>
                    <DialogTitle>拆分{label}</DialogTitle>
                    <DialogDescription>
                        填写移到新包裹的数量。原包裹自动扣减，原物流号保留；新包裹需另外登记物流。
                    </DialogDescription>
                </DialogHeader>
                <form
                    id={`${prefix}-form`}
                    onSubmit={(event) => {
                        event.preventDefault()
                        event.stopPropagation()
                        void form.handleSubmit()
                    }}
                    className="flex min-h-0 flex-1 flex-col"
                >
                    <form.Subscribe selector={(state) => state.values}>
                        {(values) => (
                            <>
                                <DialogBody className="space-y-4">
                                    {parcel.items.length > 5 && (
                                        <Input
                                            id={`${prefix}-search`}
                                            aria-label="搜索要拆分的商品"
                                            placeholder="搜索要拆分的商品"
                                            value={search}
                                            onChange={(event) =>
                                                setSearch(event.target.value)
                                            }
                                        />
                                    )}
                                    <div className="space-y-3">
                                        {values.items.map((item, index) => {
                                            const id = `${prefix}-${toAutomationIdSegment(item.delivery_line_id)}`
                                            const original =
                                                parcel.items.find(
                                                    (row) =>
                                                        row.delivery_line_id ===
                                                        item.delivery_line_id,
                                                )?.quantity ?? "0"
                                            return (
                                                <div
                                                    key={item.delivery_line_id}
                                                    hidden={
                                                        !matches(
                                                            item.delivery_line_id,
                                                        )
                                                    }
                                                    className="space-y-1.5"
                                                >
                                                    <Label htmlFor={id}>
                                                        {itemName(
                                                            item.delivery_line_id,
                                                        )}
                                                    </Label>
                                                    <div className="flex flex-wrap items-center gap-3">
                                                        <span className="text-xs text-muted-foreground">
                                                            原包裹{" "}
                                                            {packingQuantity(
                                                                original,
                                                            )}{" "}
                                                            {unit(
                                                                item.delivery_line_id,
                                                            )}
                                                        </span>
                                                        <form.Field
                                                            name={`items[${index}].quantity`}
                                                        >
                                                            {(field) => (
                                                                <Input
                                                                    id={id}
                                                                    aria-label={`${itemName(item.delivery_line_id)}拆出数量`}
                                                                    inputMode="decimal"
                                                                    className="w-24"
                                                                    value={
                                                                        field
                                                                            .state
                                                                            .value
                                                                    }
                                                                    onChange={(
                                                                        event,
                                                                    ) =>
                                                                        field.handleChange(
                                                                            event
                                                                                .target
                                                                                .value,
                                                                        )
                                                                    }
                                                                    disabled={
                                                                        disabled
                                                                    }
                                                                />
                                                            )}
                                                        </form.Field>
                                                        <span className="text-xs text-muted-foreground">
                                                            移到新包裹
                                                        </span>
                                                    </div>
                                                </div>
                                            )
                                        })}
                                    </div>
                                    {!values.items.some((item) =>
                                        matches(item.delivery_line_id),
                                    ) && (
                                        <p className="text-xs text-muted-foreground">
                                            没有匹配的商品，请更换搜索词。
                                        </p>
                                    )}
                                    {parcel.items.length > 5 && (
                                        <p className="text-xs text-muted-foreground">
                                            共 {parcel.items.length}{" "}
                                            种商品；搜索不清空已填数量，确认时一并拆出。
                                        </p>
                                    )}
                                    <p
                                        role="status"
                                        className="text-xs text-muted-foreground"
                                    >
                                        {error(values.items) ||
                                            "确认后同步更新两个包裹的商品数量。"}
                                    </p>
                                </DialogBody>
                                <DialogFooter>
                                    <Button
                                        id={`${prefix}-cancel`}
                                        type="button"
                                        variant="outline"
                                        onClick={onClose}
                                    >
                                        取消
                                    </Button>
                                    <Button
                                        id={`${prefix}-confirm`}
                                        type="submit"
                                        disabled={
                                            disabled || !!error(values.items)
                                        }
                                    >
                                        确认拆分
                                    </Button>
                                </DialogFooter>
                            </>
                        )}
                    </form.Subscribe>
                </form>
            </DialogContent>
        </Dialog>
    )
}
