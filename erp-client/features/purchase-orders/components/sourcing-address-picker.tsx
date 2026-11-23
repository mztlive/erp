"use client"

import { useState } from "react"
import { PlusIcon, SearchIcon, XIcon } from "lucide-react"
import { QuantityValue } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Input } from "@/components/ui/input"
import {
    Popover,
    PopoverContent,
    PopoverTitle,
    PopoverTrigger,
} from "@/components/ui/popover"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { fullDeliveryAddress } from "@/lib/delivery-address"
import { compareDecimal } from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"
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
    const [query, setQuery] = useState("")
    const assigned = new Set(rows.map(deliveryKey))
    const candidates = available.filter(
        (row) => !assigned.has(deliveryKey(row)),
    )
    const reason = (row: PurchaseDeliveryAllocation) =>
        !addressDeliverable(option, row.region_code)
            ? "该来源不配送"
            : compareDecimal(row.quantity, "0", 6) <= 0
              ? "已分配完"
              : undefined
    const deliverableCandidates = candidates.filter((row) => !reason(row))
    const keyword = query.trim().toLocaleLowerCase()
    const visibleCandidates = keyword
        ? candidates.filter((row) =>
              `${row.recipient} ${row.phone} ${fullDeliveryAddress(row)}`
                  .toLocaleLowerCase()
                  .includes(keyword),
          )
        : candidates
    const visibleDeliverableKeys = visibleCandidates
        .filter((row) => !reason(row))
        .map(deliveryKey)
    const allVisibleSelected =
        visibleDeliverableKeys.length > 0 &&
        visibleDeliverableKeys.every((key) => picked.includes(key))
    const selected = deliverableCandidates.filter((row) =>
        picked.includes(deliveryKey(row)),
    )

    const toggleAllVisible = () => {
        setPicked((previous) => {
            if (allVisibleSelected) {
                const visibleSet = new Set(visibleDeliverableKeys)
                return previous.filter((key) => !visibleSet.has(key))
            }
            return [...new Set([...previous, ...visibleDeliverableKeys])]
        })
    }

    return (
        <Popover
            open={open}
            onOpenChange={(next) => {
                setOpen(next)
                setPicked([])
                setQuery("")
            }}
        >
            <PopoverTrigger
                render={
                    <Button
                        id={`${id}-add`}
                        type="button"
                        variant={
                            deliverableCandidates.length > 0
                                ? "outline"
                                : "ghost"
                        }
                        size="sm"
                        className={cn(
                            deliverableCandidates.length === 0 &&
                                "text-muted-foreground",
                        )}
                    />
                }
            >
                <PlusIcon />
                添加交付地址
                {deliverableCandidates.length > 0 ? (
                    <span className="num rounded-sm bg-muted px-1.5 py-0 text-tiny font-medium text-muted-foreground">
                        {deliverableCandidates.length}
                    </span>
                ) : null}
            </PopoverTrigger>
            <PopoverContent
                align="end"
                className="w-[min(32rem,calc(100vw-2rem))] gap-0 overflow-hidden rounded-lg p-0"
            >
                <div className="border-b border-border/60 px-4 py-3">
                    <PopoverTitle>选择交付地址</PopoverTitle>
                    <p className="mt-0.5 text-xs text-muted-foreground">
                        可多选；可分配数量已扣除其他来源承担的数量。
                    </p>
                </div>

                {candidates.length > 0 &&
                (candidates.length >= 6 || deliverableCandidates.length > 1) ? (
                    <div className="flex flex-wrap items-center justify-between gap-2 border-b border-border/60 bg-surface-sunken px-4 py-2">
                        {candidates.length >= 6 ? (
                            <div className="relative min-w-36 flex-1">
                                <SearchIcon
                                    aria-hidden="true"
                                    className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground"
                                />
                                <Input
                                    id={`${id}-picker-search`}
                                    aria-label="搜索待添加的收件人或地址"
                                    placeholder="搜索收件人、电话或地址"
                                    className="h-control-sm bg-card pr-7 pl-8 text-xs"
                                    value={query}
                                    onChange={(event) =>
                                        setQuery(event.target.value)
                                    }
                                />
                                {query ? (
                                    <Button
                                        id={`${id}-picker-search-clear`}
                                        type="button"
                                        variant="ghost"
                                        size="icon-xs"
                                        aria-label="清除搜索"
                                        className="absolute top-1/2 right-0.5 -translate-y-1/2 text-muted-foreground"
                                        onClick={() => setQuery("")}
                                    >
                                        <XIcon />
                                    </Button>
                                ) : null}
                            </div>
                        ) : (
                            <span className="text-xs text-muted-foreground">
                                可添加 {deliverableCandidates.length} 个地址
                            </span>
                        )}
                        {visibleDeliverableKeys.length > 1 ? (
                            <Button
                                id={`${id}-picker-select-all`}
                                type="button"
                                variant="ghost"
                                size="xs"
                                className="text-primary hover:bg-sidebar-accent/70 hover:text-primary"
                                onClick={toggleAllVisible}
                            >
                                {allVisibleSelected
                                    ? "取消全选"
                                    : `全选可分配 (${visibleDeliverableKeys.length})`}
                            </Button>
                        ) : null}
                    </div>
                ) : null}

                <div className="max-h-72 overflow-y-auto overscroll-contain">
                    {candidates.length === 0 ? (
                        <p className="px-4 py-6 text-center text-xs text-muted-foreground">
                            本来源已添加全部地址。
                        </p>
                    ) : visibleCandidates.length === 0 ? (
                        <p className="px-4 py-6 text-center text-xs text-muted-foreground">
                            没有匹配的待添加地址。
                        </p>
                    ) : (
                        <div className="divide-y divide-grid">
                            {visibleCandidates.map((row) => {
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
                                        className={cn(
                                            "flex items-start gap-3 px-4 py-2.5 transition-colors",
                                            disabledReason
                                                ? "cursor-not-allowed opacity-60"
                                                : "cursor-pointer hover:bg-row-hover",
                                        )}
                                    >
                                        <Checkbox
                                            id={checkboxId}
                                            className="mt-0.5"
                                            disabled={Boolean(disabledReason)}
                                            checked={picked.includes(key)}
                                            onCheckedChange={toggle}
                                        />
                                        <span className="min-w-0 flex-1">
                                            <span className="flex flex-wrap items-center justify-between gap-x-2 gap-y-0.5">
                                                <span className="text-body-compact font-medium text-foreground">
                                                    {row.recipient}
                                                    <span className="num ml-1.5 text-xs font-normal text-muted-foreground">
                                                        {row.phone}
                                                    </span>
                                                </span>
                                                {disabledReason ? (
                                                    <Badge
                                                        variant={
                                                            disabledReason ===
                                                            "该来源不配送"
                                                                ? "destructive"
                                                                : "neutral"
                                                        }
                                                    >
                                                        {disabledReason}
                                                    </Badge>
                                                ) : (
                                                    <span className="text-xs text-muted-foreground">
                                                        可分配{" "}
                                                        <QuantityValue
                                                            className="font-medium text-foreground"
                                                            value={row.quantity}
                                                            unit={unit}
                                                        />
                                                    </span>
                                                )}
                                            </span>
                                            <span className="mt-0.5 block break-words text-xs text-muted-foreground">
                                                {fullDeliveryAddress(row)}
                                            </span>
                                        </span>
                                    </label>
                                )
                            })}
                        </div>
                    )}
                </div>

                <div className="flex items-center justify-between gap-2 border-t border-border/60 bg-surface-sunken px-4 py-2.5">
                    <span className="num text-xs text-muted-foreground">
                        {selected.length > 0
                            ? `已选 ${selected.length} 个地址`
                            : "请选择需添加的地址"}
                    </span>
                    <div className="flex items-center gap-2">
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
                                setQuery("")
                            }}
                        >
                            添加
                            {selected.length
                                ? ` ${selected.length} 个地址`
                                : ""}
                        </Button>
                    </div>
                </div>
            </PopoverContent>
        </Popover>
    )
}
