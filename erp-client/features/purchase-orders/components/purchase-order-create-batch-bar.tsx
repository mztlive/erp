"use client"

import * as React from "react"
import { ListFilterIcon, SparklesIcon } from "lucide-react"
import { OptionCombobox } from "@/components/business/option-combobox"
import { cn } from "@/lib/utils"
import {
    Tooltip,
    TooltipContent,
    TooltipTrigger,
} from "@/components/ui/tooltip"
import { Button } from "@/components/ui/button"
import { Switch } from "@/components/ui/switch"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
    DialogTrigger,
} from "@/components/ui/dialog"
import type { SourcingSupplierOption } from "@/features/purchase-orders/lib/purchase-order-create-model"
import { FULFILLMENT_RESPONSIBILITY_LABEL } from "@/features/purchase-orders/types"

export type PurchaseOrderCreateBatchBarProps = {
    compact?: boolean
    selectedCount: number
    participatingCount?: number
    options: readonly SourcingSupplierOption[]
    disabled?: boolean
    matchDisabled?: boolean
    getApplicableCount: (basisId: string) => number
    onApply: (basisId: string) => void
    onSetParticipation?: (included: boolean) => void
    onMatchBest: () => void
}

/** 批量设置只在确认后应用，取消不会修改分配草稿。 */
export function PurchaseOrderCreateBatchBar({
    compact = false,
    selectedCount,
    participatingCount = 0,
    options,
    disabled,
    matchDisabled,
    getApplicableCount,
    onApply,
    onMatchBest,
    onSetParticipation,
}: PurchaseOrderCreateBatchBarProps) {
    const [open, setOpen] = React.useState(false)
    const [basisId, setBasisId] = React.useState<string | null>(null)
    React.useEffect(() => {
        if (!options.some((option) => option.basisId === basisId))
            setBasisId(options[0]?.basisId ?? null)
    }, [basisId, options])
    const applicableCount = basisId ? getApplicableCount(basisId) : 0
    const allParticipating =
        selectedCount > 0 && participatingCount === selectedCount
    const partiallyParticipating = participatingCount > 0 && !allParticipating
    return (
        <div
            className={cn(
                "flex flex-wrap items-center gap-2",
                !compact && "rounded-md bg-muted/40 px-3 py-2",
            )}
        >
            {onSetParticipation && selectedCount > 0 ? (
                <label
                    htmlFor="procurement-orders-create-batch-participation"
                    className="mr-1 inline-flex min-h-8 cursor-pointer items-center gap-3 text-xs font-medium"
                >
                    <Tooltip>
                        <TooltipTrigger
                            render={
                                <Switch
                                    id="procurement-orders-create-batch-participation"
                                    checked={allParticipating}
                                    disabled={disabled}
                                    onCheckedChange={onSetParticipation}
                                    aria-label={`批量本次分配（已选 ${selectedCount} 项）`}
                                    aria-describedby="procurement-orders-create-batch-participation-state"
                                />
                            }
                        />
                        <TooltipContent>
                            已选 {selectedCount} 项，{participatingCount}{" "}
                            项参与本次分配。
                            {allParticipating
                                ? "关闭后，已选商品全部暂不分配，保留原方案。"
                                : "开启后，已选商品全部参与本次分配。"}
                        </TooltipContent>
                    </Tooltip>
                    <span id="procurement-orders-create-batch-participation-state">
                        {partiallyParticipating
                            ? "部分参与"
                            : allParticipating
                              ? "本次分配"
                              : "暂不分配"}
                    </span>
                </label>
            ) : null}
            <Dialog open={open} onOpenChange={setOpen}>
                <Tooltip>
                    <TooltipTrigger
                        render={
                            <DialogTrigger
                                id="procurement-orders-create-batch-toggle"
                                render={
                                    <Button
                                        type="button"
                                        size={compact ? "icon-sm" : "sm"}
                                        variant={compact ? "ghost" : "outline"}
                                        aria-label="批量指定来源"
                                        disabled={
                                            disabled || selectedCount === 0
                                        }
                                    />
                                }
                            />
                        }
                    >
                        <ListFilterIcon aria-hidden="true" />
                        {!compact
                            ? `批量指定来源${selectedCount > 0 ? `（${selectedCount}）` : ""}`
                            : null}
                    </TooltipTrigger>
                    <TooltipContent>
                        批量指定来源
                        {selectedCount > 0
                            ? `（已选 ${selectedCount} 行）`
                            : "（请先选择商品）"}
                    </TooltipContent>
                </Tooltip>
                <DialogContent
                    closeButtonId="procurement-orders-create-batch-close"
                    className="sm:max-w-lg"
                >
                    <DialogHeader>
                        <DialogTitle>批量指定来源</DialogTitle>
                        <DialogDescription>
                            已选 {selectedCount}{" "}
                            行。确认后覆盖可应用商品的供给来源、数量、交期及拆分方案，保留各商品是否参与本次分配的选择。
                        </DialogDescription>
                    </DialogHeader>
                    <div className="space-y-3">
                        <OptionCombobox
                            id="procurement-orders-create-batch-option"
                            className="w-full min-w-0"
                            value={basisId}
                            onValueChange={setBasisId}
                            allowClear={false}
                            disabled={disabled || options.length === 0}
                            placeholder={
                                options.length
                                    ? "选择供给来源 / 履约方式"
                                    : "选中行没有可指定的履约方案"
                            }
                            aria-label="批量指定履约方案"
                            options={options.map((option) => ({
                                value: option.basisId,
                                label:
                                    option.sourceType === "EXISTING_STOCK"
                                        ? `${option.supplierName} · 现货`
                                        : `${option.supplierName} · ${FULFILLMENT_RESPONSIBILITY_LABEL[option.fulfillmentResponsibility]}`,
                                keywords: `${option.sourceType} ${option.supplierId} ${option.warehouseName ?? ""} ${option.fulfillmentResponsibility}`,
                            }))}
                        />
                        <p
                            role="status"
                            className="text-sm text-muted-foreground"
                        >
                            将更新 {applicableCount} 行。
                            {selectedCount > applicableCount
                                ? `其余 ${selectedCount - applicableCount} 行不支持该来源，保留原方案。`
                                : ""}
                        </p>
                    </div>
                    <DialogFooter>
                        <Button
                            id="procurement-orders-create-batch-cancel"
                            type="button"
                            variant="outline"
                            onClick={() => setOpen(false)}
                        >
                            取消
                        </Button>
                        <Button
                            id="procurement-orders-create-batch-apply"
                            type="button"
                            disabled={
                                disabled ||
                                !basisId ||
                                applicableCount === 0 ||
                                selectedCount === 0
                            }
                            data-testid="purchase-create-batch-apply"
                            onClick={() => {
                                if (
                                    !basisId ||
                                    !applicableCount ||
                                    !selectedCount
                                )
                                    return
                                onApply(basisId)
                                setOpen(false)
                            }}
                        >
                            应用到选中行
                        </Button>
                    </DialogFooter>
                </DialogContent>
            </Dialog>
            <Tooltip>
                <TooltipTrigger
                    render={
                        <Button
                            id="procurement-orders-create-batch-match"
                            type="button"
                            size={compact ? "icon-sm" : "sm"}
                            aria-label="重新自动分配（直发优先）"
                            variant={compact ? "ghost" : "outline"}
                            className={compact ? undefined : "ml-auto"}
                            disabled={disabled || matchDisabled}
                            onClick={onMatchBest}
                            data-testid="purchase-create-match-best"
                        />
                    }
                >
                    <SparklesIcon aria-hidden="true" />
                    {!compact ? "重新自动分配（直发优先）" : null}
                </TooltipTrigger>
                <TooltipContent>
                    重新自动分配（直发优先）：仅重新推荐本次分配的商品，将替换这些商品的手工调整与拆分方案；暂不分配的商品保持原方案。
                </TooltipContent>
            </Tooltip>
        </div>
    )
}
