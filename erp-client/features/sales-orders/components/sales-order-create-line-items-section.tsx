"use client"

import { Badge } from "@/components/ui/badge"
import type { SalesOrderCreateFormApi } from "@/features/sales-orders/lib/sales-order-create-form-types"
import { SalesOrderCreateLineItemTable } from "@/features/sales-orders/components/sales-order-create-line-item-table"
import type { SalesLineProcurementResponsibility } from "@/features/sales-orders/types"

export type SalesOrderCreateLineItemsSectionProps = {
    form: SalesOrderCreateFormApi
    procurementOwners?: ReadonlyMap<string, SalesLineProcurementResponsibility>
}

export function SalesOrderCreateLineItemsSection({
    form,
    procurementOwners,
}: SalesOrderCreateLineItemsSectionProps) {
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
