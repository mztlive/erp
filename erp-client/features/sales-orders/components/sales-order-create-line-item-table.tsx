"use client"

import { useEffect } from "react"
import { useSelector } from "@tanstack/react-form"

import {
    BusinessFailureState,
    EditableLineItemTable,
    ValidationSummary,
} from "@/components/business"
import { toFieldErrors } from "@/components/form"
import type { SalesOrderCreateFormApi } from "@/features/sales-orders/lib/sales-order-create-form-types"
import type { SalesLineProcurementResponsibility } from "@/features/sales-orders/types"
import { buildSalesOrderCreateLineItemColumns } from "@/features/sales-orders/components/sales-order-create-line-item-columns"
import { useSalesLineImagesQuery } from "@/features/sales-orders/hooks/use-sales-line-images-query"

export type SalesOrderCreateLineItemTableProps = {
    inquiryLocked?: boolean
    form: SalesOrderCreateFormApi
    procurementOwners?: ReadonlyMap<string, SalesLineProcurementResponsibility>
    onPickSku?: (rowIndex: number) => void
    procurementFetching?: boolean
    procurementError?: boolean
}

export function SalesOrderCreateLineItemTable({
    form,
    inquiryLocked = false,
    procurementOwners,
    onPickSku,
    procurementFetching = false,
    procurementError = false,
}: SalesOrderCreateLineItemTableProps) {
    const nature = useSelector(form.store, (state) => state.values.nature)
    const lines = useSelector(form.store, (state) => state.values.lineItems)
    const skuImages = useSalesLineImagesQuery(
        nature === "physical_service" ? lines.map((line) => line.sku) : [],
    )
    // 方案带入的行尚无商品池参考价；仅补齐同一修订，后台刷新不覆盖已编辑单价。
    useEffect(() => {
        if (inquiryLocked || nature !== "physical_service") return
        form.getFieldValue("lineItems").forEach((line, index) => {
            if (line.referencePrices) return
            const reference = skuImages.references.get(line.skuRevisionId)
            if (!reference || reference.skuId !== line.sku) return
            form.setFieldValue(`lineItems[${index}].referencePrices`, reference)
            if (
                !line.procurementPriceBasis &&
                !line.procurementSupplyPrice?.trim()
            ) {
                form.setFieldValue(
                    `lineItems[${index}].procurementPriceBasis`,
                    "DROPSHIP",
                )
                form.setFieldValue(
                    `lineItems[${index}].procurementSupplyPrice`,
                    reference.salesVisiblePriceGross,
                )
            }
        })
    }, [form, inquiryLocked, nature, skuImages.references])
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
            {skuImages.error ? (
                <BusinessFailureState
                    id="sales-orders-create-line-images"
                    title="商品图片资料读取失败"
                    error={skuImages.error}
                    onRetry={() => void skuImages.refetch()}
                />
            ) : null}
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
                        inquiryLocked,
                        skuImages.images,
                    )
                    return (
                        <>
                            {values.lineItems.length > 0 ? (
                                <EditableLineItemTable
                                    id="sales-orders-create-line-items"
                                    className="[&_tbody_td]:align-top [&_tbody_td:last-child>div]:min-h-control [&_tbody_td:last-child>div]:items-center"
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
                            values.lineItems.some(
                                (line) =>
                                    line.sku.trim() &&
                                    !procurementOwners?.get(line.rowKey)
                                        ?.resolved,
                            ) ? (
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
