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
    procurementFetching?: boolean
    procurementError?: boolean
}

export function SalesOrderCreateLineItemTable({
    form,
    procurementOwners,
    onPickSku,
    procurementFetching = false,
    procurementError = false,
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
        <>
            <form.AppField name="lineItems" mode="array">
                {() => null}
            </form.AppField>
            <form.Subscribe selector={(state) => state.values}>
                {(values) => {
                    const columns = buildSalesOrderCreateLineItemColumns(
                        values,
                        form,
                        procurementOwners,
                        onPickSku,
                        {
                            fetching: procurementFetching,
                            error: procurementError,
                        },
                    )
                    return (
                        <>
                            {values.lineItems.length > 0 ? (
                                <EditableLineItemTable
                                    id="sales-orders-create-line-items"
                                    items={values.lineItems}
                                    columns={columns}
                                    getRowId={(item) => item.rowKey}
                                    caption="销售单创建明细"
                                    emptyContent="至少需要一条销售明细。"
                                    onRemoveItem={(_item, _rowId, rowIndex) => {
                                        void form.removeFieldValue(
                                            "lineItems",
                                            rowIndex,
                                        )
                                    }}
                                    getRemoveDisabledReason={() =>
                                        values.nature === "card_voucher"
                                            ? "卡券销售单必须保留唯一明细"
                                            : undefined
                                    }
                                />
                            ) : null}

                            {values.nature === "physical_service" &&
                            values.lineItems.some((line) => line.sku.trim()) &&
                            !procurementFetching &&
                            (procurementError ||
                                values.lineItems.some(
                                    (line) =>
                                        line.sku.trim() &&
                                        !procurementOwners?.get(line.rowKey)
                                            ?.resolved,
                                )) ? (
                                <p
                                    role="status"
                                    className="mt-3 text-sm text-destructive"
                                >
                                    {procurementError
                                        ? "采购负责人暂时无法匹配，请稍后重试。"
                                        : "部分商品尚未匹配采购负责人，请联系管理员维护采购责任规则。"}
                                    可以保存草稿，完成匹配后才能提交审批。
                                </p>
                            ) : null}
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
        </>
    )
}
