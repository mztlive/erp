"use client"

import * as React from "react"

import { Badge } from "@/components/ui/badge"
import {
    SellableSkuSelectDialog,
    type SellableSkuPick,
} from "@/features/entity-selectors"
import { SalesOrderCreateLineItemTable } from "@/features/sales-orders/components/sales-order-create-line-item-table"
import type { SalesOrderCreateFormApi } from "@/features/sales-orders/lib/sales-order-create-form-types"
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

    return (
        <section
            id="sales-line-items-section"
            className="border-b border-grid p-4 md:p-5 lg:p-6"
        >
            <div className="mb-4 flex items-center justify-between gap-2">
                <h2 className="font-heading text-sm font-semibold">销售明细</h2>
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
