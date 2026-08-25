"use client"

import { useSelector } from "@tanstack/react-form"

import { EditableLineItemTable, ValidationSummary } from "@/components/business"
import { toFieldErrors } from "@/components/form"
import type { SalesOrderCreateFormApi } from "@/features/sales-orders/lib/sales-order-create-form-types"
import type { SalesLineProcurementResponsibility } from "@/features/sales-orders/types"
import { buildSalesOrderCreateLineItemColumns } from "@/features/sales-orders/components/sales-order-create-line-item-columns"

export type SalesOrderCreateLineItemTableProps = {
    form: SalesOrderCreateFormApi
    procurementOwners?: ReadonlyMap<string, SalesLineProcurementResponsibility>
    onPickSku?: (rowIndex: number) => void
    onAddSkus?: () => void
}

export function SalesOrderCreateLineItemTable({
    form,
    procurementOwners,
    onPickSku,
    onAddSkus,
}: SalesOrderCreateLineItemTableProps) {
    /** 明细表根路径校验错误（如卡券仅一条）在明细区汇总展示。 */
    const lineItemIssues = useSelector(form.store, (state) => {
        return toFieldErrors(state.fieldMeta.lineItems?.errors ?? [])
            .filter((error) => Boolean(error?.message))
            .map((error, index) => ({
                id: `line-items-${index}`,
                label: "销售明细",
                message: error!.message!,
                targetId: "sales-line-items-section",
            }))
    })

    return (
        <form.Subscribe selector={(state) => state.values}>
            {(values) => {
                const columns = buildSalesOrderCreateLineItemColumns(
                    values,
                    form,
                    procurementOwners,
                    onPickSku,
                )
                return (
                    <>
                        <EditableLineItemTable
                            items={values.lineItems}
                            columns={columns}
                            getRowId={(item) => item.rowKey}
                            caption="销售单创建明细"
                            emptyContent="至少需要一条销售明细。"
                            addLabel="选择商品"
                            addDisabledReason={
                                values.nature === "card_voucher"
                                    ? "卡券销售单每个版本必须恰好一条明细"
                                    : undefined
                            }
                            onAddItem={
                                values.nature === "physical_service"
                                    ? onAddSkus
                                    : undefined
                            }
                            onRemoveItem={(_item, _rowId, rowIndex) => {
                                void form.removeFieldValue(
                                    "lineItems",
                                    rowIndex,
                                )
                            }}
                            getRemoveDisabledReason={() =>
                                values.lineItems.length === 1
                                    ? "销售单至少保留一条明细"
                                    : values.nature === "card_voucher"
                                      ? "卡券销售单必须保留唯一明细"
                                      : undefined
                            }
                        />

                        {lineItemIssues.length > 0 ? (
                            <ValidationSummary
                                className="mt-4"
                                issues={lineItemIssues}
                                title={`明细共 ${lineItemIssues.length} 项待处理`}
                            />
                        ) : null}
                    </>
                )
            }}
        </form.Subscribe>
    )
}
