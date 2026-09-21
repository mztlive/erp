"use client"

import * as React from "react"

import { PlusIcon, PackageSearchIcon, ChevronDownIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import {
    Collapsible,
    CollapsibleTrigger,
    CollapsibleContent,
} from "@/components/ui/collapsible"
import { QuantityValue } from "@/components/business"
import { toast } from "@/components/ui/toast"
import { SellableSkuSelectDialog } from "@/features/sales-orders/components/sellable-sku-select-dialog"
import { SalesOrderCreateDueDateBatchBar } from "@/features/sales-orders/components/sales-order-create-due-date-batch-bar"
import { SalesOrderCreateLineItemTable } from "@/features/sales-orders/components/sales-order-create-line-item-table"
import type { SalesOrderCreateFormApi } from "@/features/sales-orders/lib/sales-order-create-form-types"
import { applyDueDateToLines } from "@/features/sales-orders/lib/sales-order-create-model"
import { applySellablePicksToLines } from "@/features/sales-orders/lib/sales-order-create-sku-picks"
import type { SellableSkuPick } from "@/features/sales-orders/lib/sellable-sku-pick"
import type { SalesLineProcurementResponsibility } from "@/features/sales-orders/types"

export type SalesOrderCreateLineItemsSectionProps = {
    form: SalesOrderCreateFormApi
    procurementOwners?: ReadonlyMap<string, SalesLineProcurementResponsibility>
    procurementFetching?: boolean
    procurementError?: boolean
}

type SkuPickerState = { mode: "add" } | { mode: "replace"; rowIndex: number }

export function SalesOrderCreateLineItemsSection({
    form,
    procurementOwners,
    procurementFetching = false,
    procurementError = false,
}: SalesOrderCreateLineItemsSectionProps) {
    const [remarkOpen, setRemarkOpen] = React.useState(
        Boolean(form.getFieldValue("remark")),
    )
    const [picker, setPicker] = React.useState<SkuPickerState | null>(null)

    const handleConfirmPicks = React.useCallback(
        (picks: readonly SellableSkuPick[]) => {
            if (picks.length === 0) return
            const nature = form.getFieldValue("nature")
            const lineItems = form.getFieldValue("lineItems")
            form.setFieldValue(
                "lineItems",
                applySellablePicksToLines(
                    lineItems,
                    picks,
                    nature,
                    picker?.mode === "replace" ? picker.rowIndex : undefined,
                ),
            )
        },
        [form, picker],
    )

    const handleApplyDueDate = React.useCallback(
        (dueDate: string) => {
            const lineItems = form.getFieldValue("lineItems")
            const next = applyDueDateToLines(lineItems, dueDate)
            form.setFieldValue("lineItems", next)
            toast.add({
                title: "已批量设置交期",
                description: `已将 ${next.length} 条明细的承诺交付日设为 ${dueDate}。`,
                type: "success",
                timeout: 3000,
            })
        },
        [form],
    )

    return (
        <>
            <section
                id="sales-line-items-section"
                tabIndex={-1}
                className="min-w-0 space-y-3"
                aria-labelledby="sales-create-lines-title"
            >
                <form.Subscribe
                    selector={(state) => ({
                        nature: state.values.nature,
                        lines: state.values.lineItems,
                    })}
                >
                    {({ nature, lines }) => (
                        <>
                            <div className="flex flex-wrap items-center justify-between gap-3">
                                <div className="flex flex-wrap items-baseline gap-3">
                                    <h2
                                        id="sales-create-lines-title"
                                        className="font-heading text-base font-semibold"
                                    >
                                        销售明细
                                    </h2>
                                    <span className="text-sm text-muted-foreground">
                                        {nature === "card_voucher" ? (
                                            "卡券 · 仅一条明细"
                                        ) : (
                                            <>
                                                已添加{" "}
                                                <QuantityValue
                                                    unit=""
                                                    value={String(
                                                        lines.filter((line) =>
                                                            line.sku.trim(),
                                                        ).length,
                                                    )}
                                                />{" "}
                                                项商品
                                            </>
                                        )}
                                    </span>
                                </div>
                                {nature === "physical_service" ? (
                                    <div className="flex flex-wrap items-center gap-3">
                                        {lines.length > 0 ? (
                                            <SalesOrderCreateDueDateBatchBar
                                                lineCount={lines.length}
                                                onApply={handleApplyDueDate}
                                            />
                                        ) : null}
                                        <Button
                                            id="sales-orders-create-line-items-add"
                                            type="button"
                                            variant="outline"
                                            onClick={() =>
                                                setPicker({ mode: "add" })
                                            }
                                        >
                                            <PlusIcon aria-hidden="true" />
                                            添加商品
                                        </Button>
                                    </div>
                                ) : null}
                            </div>
                            {nature === "physical_service" &&
                            lines.length === 0 ? (
                                <div className="flex min-h-24 items-center justify-center gap-3 rounded-md border border-dashed border-border bg-muted/10 px-4 py-4">
                                    <PackageSearchIcon
                                        className="size-5 shrink-0 text-muted-foreground"
                                        aria-hidden="true"
                                    />
                                    <div className="space-y-1">
                                        <p className="text-sm font-medium">
                                            尚未添加商品
                                        </p>
                                        <p className="text-sm text-muted-foreground">
                                            点击“添加商品”，选择后填写数量、含税单价和交付日。
                                        </p>
                                    </div>
                                </div>
                            ) : null}
                        </>
                    )}
                </form.Subscribe>
                <SalesOrderCreateLineItemTable
                    form={form}
                    procurementOwners={procurementOwners}
                    procurementFetching={procurementFetching}
                    procurementError={procurementError}
                    onPickSku={(rowIndex) =>
                        setPicker({ mode: "replace", rowIndex })
                    }
                />
                <SellableSkuSelectDialog
                    open={picker != null}
                    onOpenChange={(open) => {
                        if (!open) setPicker(null)
                    }}
                    multiple={picker?.mode !== "replace"}
                    excludeProductKind="VOUCHER"
                    title={
                        picker?.mode === "replace" ? "更换销售商品" : "添加商品"
                    }
                    onConfirm={handleConfirmPicks}
                />
            </section>
            <form.AppField name="remark">
                {(field) => (
                    <Collapsible
                        open={remarkOpen || field.state.meta.errors.length > 0}
                        onOpenChange={setRemarkOpen}
                        className="border-t border-grid pt-3"
                    >
                        <CollapsibleTrigger
                            id="sales-orders-create-remark-toggle"
                            render={
                                <Button
                                    type="button"
                                    variant="ghost"
                                    size="sm"
                                    className="-ml-2 text-muted-foreground"
                                />
                            }
                        >
                            <ChevronDownIcon
                                className="size-4 transition-transform in-data-[panel-open]:rotate-180"
                                aria-hidden="true"
                            />
                            {field.state.value
                                ? "内部说明（已填写）"
                                : "内部说明（选填）"}
                        </CollapsibleTrigger>
                        <CollapsibleContent keepMounted>
                            <field.TextareaField
                                id="sales-orders-create-remark"
                                label="内部说明（选填）"
                                hideLabel
                                className="pt-2"
                                placeholder="补充客户确认、交付或内部协同说明"
                                rows={2}
                            />
                        </CollapsibleContent>
                    </Collapsible>
                )}
            </form.AppField>
        </>
    )
}
