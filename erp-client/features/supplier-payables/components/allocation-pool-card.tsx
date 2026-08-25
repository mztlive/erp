"use client"

import {
    MoneyValue,
    surfaceInsetClassName,
    surfacePanelClassName,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import {
    SOURCE_TYPE_LABEL,
    type AllocationSessionView,
    type AllocationTrack,
} from "@/features/supplier-payables/types"
import { cn } from "@/lib/utils"

export type AllocationPoolCardProps = {
    supplierName: string
    pool: AllocationSessionView["pool"]
    track: AllocationTrack
    selected: ReadonlySet<string>
    amounts: Readonly<Record<string, string>>
    disabled: boolean
    onToggleItem: (
        payableAccountId: string,
        checked: boolean | "indeterminate",
        open: string,
    ) => void
    onAmountChange: (payableAccountId: string, value: string) => void
    onToggleSelectAll: () => void
    onFillAllSelected: () => void
}

/** 同供应商待核销池：勾选目标并填写本次分配金额。 */
export function AllocationPoolCard({
    supplierName,
    pool,
    track,
    selected,
    amounts,
    disabled,
    onToggleItem,
    onAmountChange,
    onToggleSelectAll,
    onFillAllSelected,
}: AllocationPoolCardProps) {
    return (
        <section
            id="alloc-pool"
            className={cn(surfacePanelClassName, "min-w-0 overflow-hidden")}
            aria-label="同供应商待核销池"
        >
            <div className="border-b border-border px-4 py-3">
                <h2 className="text-sm font-semibold">同供应商待核销池</h2>
                <p className="mt-0.5 text-xs text-muted-foreground">
                    仅 {supplierName} ·{" "}
                    {track === "payment" ? "开放应付" : "可收票余额"}
                </p>
            </div>
            <div className="space-y-2 p-4">
                <div className="flex flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground">
                    <span>共 {pool.length} 个开放目标</span>
                    <div className="flex gap-1">
                        <Button
                            type="button"
                            size="xs"
                            variant="ghost"
                            disabled={pool.length === 0 || disabled}
                            onClick={onToggleSelectAll}
                        >
                            全选
                        </Button>
                        <Button
                            type="button"
                            size="xs"
                            variant="ghost"
                            disabled={selected.size === 0 || disabled}
                            onClick={onFillAllSelected}
                        >
                            按开放余额填满
                        </Button>
                    </div>
                </div>
                {pool.length === 0 ? (
                    <p className="py-6 text-sm text-muted-foreground">
                        当前无开放目标
                    </p>
                ) : (
                    pool.map((item) => {
                        const checked = selected.has(item.payableAccountId)
                        const open =
                            track === "payment"
                                ? item.openTotal
                                : item.openInvoiceableTotal
                        return (
                            <div
                                key={item.payableAccountId}
                                className={cn(
                                    "flex flex-col gap-2 rounded-lg border p-3",
                                    checked
                                        ? "border-primary/30 bg-accent"
                                        : "border-border bg-card",
                                )}
                            >
                                <div className="flex items-start gap-2">
                                    <Checkbox
                                        checked={checked}
                                        onCheckedChange={(v) =>
                                            onToggleItem(
                                                item.payableAccountId,
                                                v,
                                                open,
                                            )
                                        }
                                        aria-label={`选择 ${item.sourceDocumentNo}`}
                                    />
                                    <div className="min-w-0 flex-1">
                                        <div className="flex flex-wrap items-center gap-2 text-sm">
                                            <span className="font-medium">
                                                {
                                                    SOURCE_TYPE_LABEL[
                                                        item.sourceType
                                                    ]
                                                }
                                            </span>
                                            <span className="num">
                                                {item.sourceDocumentNo}
                                            </span>
                                            <span className="text-xs text-muted-foreground">
                                                {item.dueStateLabel} ·{" "}
                                                {item.dueDate}
                                            </span>
                                        </div>
                                        <div className="mt-1 flex flex-wrap items-center justify-between gap-2 text-sm">
                                            <span className="text-muted-foreground">
                                                开放余额
                                            </span>
                                            <MoneyValue
                                                value={open}
                                                taxBasis="gross"
                                            />
                                        </div>
                                    </div>
                                </div>
                                {checked ? (
                                    <div
                                        className={cn(
                                            surfaceInsetClassName,
                                            "flex items-center gap-2 px-3 py-2",
                                        )}
                                    >
                                        <Label
                                            htmlFor={`amt-${item.payableAccountId}`}
                                            className="text-xs whitespace-nowrap"
                                        >
                                            本次分配
                                        </Label>
                                        <Input
                                            id={`amt-${item.payableAccountId}`}
                                            className="num h-control min-h-0"
                                            inputMode="decimal"
                                            value={
                                                amounts[
                                                    item.payableAccountId
                                                ] ?? ""
                                            }
                                            onChange={(e) =>
                                                onAmountChange(
                                                    item.payableAccountId,
                                                    e.target.value,
                                                )
                                            }
                                        />
                                    </div>
                                ) : null}
                            </div>
                        )
                    })
                )}
            </div>
        </section>
    )
}
