"use client"

import { useState } from "react"
import { PlusIcon } from "lucide-react"
import { QuantityValue } from "@/components/business"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import {
    Popover,
    PopoverContent,
    PopoverTitle,
    PopoverTrigger,
} from "@/components/ui/popover"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { fullDeliveryAddress } from "@/lib/delivery-address"
import { compareDecimal } from "@/lib/fixed-decimal"
import { deliveryKey } from "../lib/sourcing/delivery"
import { addressDeliverable } from "../lib/sourcing/delivery-terms"
import type { SourcingSupplierOption } from "../lib/sourcing/types"
import type { PurchaseDeliveryAllocation } from "../types"

/** 选择待添加的地址；确认前只保存临时勾选，不修改共享草稿。 */
export function SourcingAddressPicker({
    id,
    unit,
    option,
    available,
    rows,
    onAdd,
}: {
    id: string
    unit: string
    option?: SourcingSupplierOption
    available: PurchaseDeliveryAllocation[]
    rows: PurchaseDeliveryAllocation[]
    onAdd: (rows: PurchaseDeliveryAllocation[]) => void
}) {
    const [open, setOpen] = useState(false)
    const [picked, setPicked] = useState<string[]>([])
    const assigned = new Set(rows.map(deliveryKey))
    const candidates = available.filter(
        (row) => !assigned.has(deliveryKey(row)),
    )
    const reason = (row: PurchaseDeliveryAllocation) =>
        !addressDeliverable(option, row.region_code)
            ? "该来源不配送"
            : compareDecimal(row.quantity, "0", 6) <= 0
              ? "已分配完，请先调整其他来源"
              : undefined
    const selected = candidates.filter(
        (row) => !reason(row) && picked.includes(deliveryKey(row)),
    )
    return (
        <Popover
            open={open}
            onOpenChange={(next) => {
                setOpen(next)
                setPicked([])
            }}
        >
            <PopoverTrigger
                render={
                    <Button
                        id={`${id}-add`}
                        type="button"
                        variant="outline"
                        size="sm"
                    />
                }
            >
                <PlusIcon />
                添加交付地址
            </PopoverTrigger>
            <PopoverContent
                align="end"
                className="w-[min(30rem,calc(100vw-2rem))] gap-3 rounded-lg"
            >
                <PopoverTitle>选择交付地址</PopoverTitle>
                <p className="text-xs text-muted-foreground">
                    可多选；可分配数量已扣除其他来源承担的数量。
                </p>
                <div className="max-h-72 overflow-y-auto overscroll-contain">
                    {candidates.length === 0 ? (
                        <p className="py-3 text-xs text-muted-foreground">
                            本来源已添加全部地址。
                        </p>
                    ) : (
                        candidates.map((row) => {
                            const key = deliveryKey(row)
                            const disabledReason = reason(row)
                            const checkboxId = `${id}-option-${toAutomationIdSegment(encodeURIComponent(key))}`
                            const toggle = (checked: boolean) =>
                                setPicked((previous) =>
                                    checked
                                        ? [...previous, key]
                                        : previous.filter(
                                              (item) => item !== key,
                                          ),
                                )
                            return (
                                <label
                                    key={key}
                                    htmlFor={checkboxId}
                                    className="flex items-start gap-3 border-b border-border/60 py-3 last:border-0"
                                >
                                    <Checkbox
                                        id={checkboxId}
                                        className="mt-0.5"
                                        disabled={Boolean(disabledReason)}
                                        checked={picked.includes(key)}
                                        onCheckedChange={toggle}
                                    />
                                    <span className="min-w-0 space-y-1 text-body-compact">
                                        <span className="block font-medium">
                                            {row.recipient} · {row.phone}
                                        </span>
                                        <span className="block break-words text-xs text-muted-foreground">
                                            {fullDeliveryAddress(row)}
                                        </span>
                                        <span className="block text-xs text-muted-foreground">
                                            {disabledReason ?? (
                                                <>
                                                    可分配{" "}
                                                    <QuantityValue
                                                        value={row.quantity}
                                                        unit={unit}
                                                    />
                                                </>
                                            )}
                                        </span>
                                    </span>
                                </label>
                            )
                        })
                    )}
                </div>
                <div className="flex justify-end gap-2">
                    <Button
                        id={`${id}-picker-cancel`}
                        type="button"
                        variant="outline"
                        size="sm"
                        onClick={() => setOpen(false)}
                    >
                        取消
                    </Button>
                    <Button
                        id={`${id}-picker-confirm`}
                        type="button"
                        size="sm"
                        disabled={selected.length === 0}
                        onClick={() => {
                            onAdd(selected)
                            setOpen(false)
                            setPicked([])
                        }}
                    >
                        添加
                        {selected.length ? ` ${selected.length} 个地址` : ""}
                    </Button>
                </div>
            </PopoverContent>
        </Popover>
    )
}
