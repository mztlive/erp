"use client"

import * as React from "react"

import { Badge } from "@/components/ui/badge"
import { toast } from "@/components/ui/toast"
import {
    SellableSkuSelectDialog,
    type SellableSkuPick,
} from "@/features/entity-selectors"
import { SalesOrderCreateDueDateBatchBar } from "@/features/sales-orders/components/sales-order-create-due-date-batch-bar"
import { SalesOrderCreateLineItemTable } from "@/features/sales-orders/components/sales-order-create-line-item-table"
import type { SalesOrderCreateFormApi } from "@/features/sales-orders/lib/sales-order-create-form-types"
import { applyDueDateToLines } from "@/features/sales-orders/lib/sales-order-create-model"
import { applySellablePicksToLines } from "@/features/sales-orders/lib/sales-order-create-sku-picks"
import type { SalesLineProcurementResponsibility } from "@/features/sales-orders/types"

export type SalesOrderCreateLineItemsSectionProps = {
    form: SalesOrderCreateFormApi
    procurementOwners?: ReadonlyMap<string, SalesLineProcurementResponsibility>
}

type SkuPickerState = { mode: "add" } | { mode: "replace"; rowIndex: number }

export function SalesOrderCreateLineItemsSection({
    form,
    procurementOwners,
}: SalesOrderCreateLineItemsSectionProps) {
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
                description: `已将 ${next.length} 条明细的交付日期设为 ${dueDate}。`,
                type: "success",
                timeout: 3000,
            })
        },
        [form],
    )

    return (
        <section
            id="sales-line-items-section"
            className="border-b border-grid p-4 md:p-5 lg:p-6"
        >
            <div className="mb-4 flex flex-col gap-3">
                <div className="flex items-center justify-between gap-2">
                    <h2 className="font-heading text-sm font-semibold">
                        销售明细
                    </h2>
                    <form.Subscribe selector={(state) => state.values.nature}>
                        {(nature) => (
                            <Badge variant="outline" className="font-normal">
                                {nature === "card_voucher"
                                    ? "卡券 · 仅一条"
                                    : "实物/服务 · 可多行"}
                            </Badge>
                        )}
                    </form.Subscribe>
                </div>
                <form.Subscribe
                    selector={(state) => ({
                        nature: state.values.nature,
                        lineCount: state.values.lineItems.length,
                    })}
                >
                    {({ nature, lineCount }) =>
                        nature === "physical_service" ? (
                            <SalesOrderCreateDueDateBatchBar
                                lineCount={lineCount}
                                onApply={handleApplyDueDate}
                            />
                        ) : null
                    }
                </form.Subscribe>
            </div>

            <SalesOrderCreateLineItemTable
                form={form}
                procurementOwners={procurementOwners}
                onPickSku={(rowIndex) =>
                    setPicker({ mode: "replace", rowIndex })
                }
                onAddSkus={() => setPicker({ mode: "add" })}
            />

            <SellableSkuSelectDialog
                open={picker != null}
                onOpenChange={(open) => {
                    if (!open) setPicker(null)
                }}
                multiple
                excludeProductKind="VOUCHER"
                title={picker?.mode === "replace" ? "更换销售商品" : "选择商品"}
                onConfirm={handleConfirmPicks}
            />

            <div className="mt-5">
                <form.AppField name="remark">
                    {(field) => (
                        <field.TextareaField
                            label="内部说明"
                            placeholder="补充客户确认、交付或内部协同说明（可选）"
                            rows={2}
                        />
                    )}
                </form.AppField>
            </div>
        </section>
    )
}
