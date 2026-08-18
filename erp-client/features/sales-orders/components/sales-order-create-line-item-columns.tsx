"use client"

import { MoneyValue, type EditableLineItemColumn } from "@/components/business"
import {
    CARD_FORM_OPTIONS,
    calculateTotals,
    deriveVoucherGiftPreview,
} from "@/features/sales-orders/lib/sales-order-create-model"
import type { CreateSalesOrderFormValues } from "@/features/sales-orders/lib/sales-order-create-model"
import type { SalesOrderCreateFormApi } from "@/features/sales-orders/lib/sales-order-create-form-types"
import type { SalesOrderDraftLineInput } from "@/features/sales-orders/types"
import { SalesOrderCreateLineSkuEditor } from "@/features/sales-orders/components/sales-order-create-line-sku-editor"

/** 按当前业务性质与表单值组装明细编辑列。 */
export function buildSalesOrderCreateLineItemColumns(
    values: CreateSalesOrderFormValues,
    form: SalesOrderCreateFormApi,
): EditableLineItemColumn<SalesOrderDraftLineInput>[] {
    const nature = values.nature
    return [
        {
            id: "item",
            header: "销售项目",
            renderValue: ({ item }) => item.name,
            renderEditor: ({ rowIndex }) => (
                <SalesOrderCreateLineSkuEditor
                    form={form}
                    values={values}
                    nature={nature}
                    rowIndex={rowIndex}
                />
            ),
        },
        {
            id: "quantity",
            header: "数量 / 单位",
            numeric: true,
            renderValue: ({ item }) => `${item.quantity} ${item.unit}`,
            renderEditor: ({ rowIndex }) => {
                const line = values.lineItems[rowIndex]
                const unitLocked =
                    nature === "card_voucher" || Boolean(line?.sku?.trim())
                return (
                    <div className="flex min-w-32 items-center gap-2">
                        <div className="w-20 shrink-0">
                            <form.AppField
                                name={`lineItems[${rowIndex}].quantity`}
                            >
                                {(field) => (
                                    <field.TextField
                                        label="数量"
                                        hideLabel
                                        type="number"
                                        inputClassName="num"
                                    />
                                )}
                            </form.AppField>
                        </div>
                        <form.AppField name={`lineItems[${rowIndex}].unit`}>
                            {(field) => (
                                <span
                                    className="min-w-8 shrink-0 text-sm text-muted-foreground"
                                    title={
                                        unitLocked
                                            ? nature === "card_voucher"
                                                ? "卡券单位固定为张"
                                                : "单位随 SKU 基础单位带出，不可改"
                                            : "选择 SKU 后带出基础单位"
                                    }
                                >
                                    {field.state.value || "—"}
                                </span>
                            )}
                        </form.AppField>
                    </div>
                )
            },
        },
        {
            id: "unitPrice",
            header: "含税单价",
            numeric: true,
            align: "end",
            renderValue: ({ item }) => item.unitPriceGross,
            renderEditor: ({ rowIndex }) => (
                <form.AppField name={`lineItems[${rowIndex}].unitPriceGross`}>
                    {(field) => (
                        <field.TextField
                            label="含税单价"
                            hideLabel
                            type="number"
                            inputClassName="num min-w-24 text-right"
                        />
                    )}
                </form.AppField>
            ),
        },
        ...(nature === "card_voucher"
            ? ([
                  {
                      id: "faceValue",
                      header: "面值",
                      numeric: true,
                      align: "end",
                      renderValue: ({ item }) => item.faceValue || "—",
                      renderEditor: ({ rowIndex }) => (
                          <div className="min-w-20">
                              <form.AppField
                                  name={`lineItems[${rowIndex}].faceValue`}
                              >
                                  {(field) => (
                                      <field.TextField
                                          label="面值"
                                          hideLabel
                                          type="number"
                                          placeholder="0.00"
                                          className="gap-0"
                                          inputClassName="num min-w-20 text-right"
                                      />
                                  )}
                              </form.AppField>
                          </div>
                      ),
                  },
                  {
                      id: "gift",
                      header: "配赠",
                      numeric: true,
                      align: "end",
                      renderValue: ({ item }) => {
                          const gift = deriveVoucherGiftPreview(
                              item.faceValue,
                              item.unitPriceGross,
                              item.quantity,
                          )
                          return gift ? `${gift.giftRatePercent}%` : "—"
                      },
                      renderEditor: ({ rowIndex }) => {
                          const line = values.lineItems[rowIndex]
                          const gift = deriveVoucherGiftPreview(
                              line?.faceValue ?? "",
                              line?.unitPriceGross ?? "",
                              line?.quantity ?? "",
                          )
                          return (
                              <span
                                  className="num flex h-8 min-w-16 items-center justify-end text-sm tabular-nums text-muted-foreground"
                                  title={
                                      gift
                                          ? `配赠率 ${gift.giftRatePercent}%（金额 ${gift.giftAmount}）。配赠 = 面值小计 − 成交金额，系统计算不可改。`
                                          : "配赠率 = 配赠金额 / 成交金额；填入面值、单价与数量后自动计算"
                                  }
                              >
                                  {gift ? `${gift.giftRatePercent}%` : "—"}
                              </span>
                          )
                      },
                  },
                  {
                      id: "cardForm",
                      header: "卡形态",
                      renderValue: ({ item }) => item.cardForm || "—",
                      renderEditor: ({ rowIndex }) => (
                          <div className="min-w-24">
                              <form.AppField
                                  name={`lineItems[${rowIndex}].cardForm`}
                              >
                                  {(field) => (
                                      <field.SelectField
                                          label="卡形态"
                                          hideLabel
                                          options={CARD_FORM_OPTIONS}
                                          allowClear={false}
                                          className="gap-0"
                                          inputClassName="w-full"
                                      />
                                  )}
                              </form.AppField>
                          </div>
                      ),
                  },
              ] satisfies EditableLineItemColumn<SalesOrderDraftLineInput>[])
            : ([
                  {
                      id: "fulfillment",
                      header: "交付日期",
                      renderValue: ({ item }) => item.dueDate || "—",
                      renderEditor: ({ rowIndex }) => (
                          <div className="min-w-32">
                              <form.AppField
                                  name={`lineItems[${rowIndex}].dueDate`}
                              >
                                  {(field) => (
                                      <field.DateField
                                          label="交付日期"
                                          hideLabel
                                      />
                                  )}
                              </form.AppField>
                          </div>
                      ),
                  },
              ] satisfies EditableLineItemColumn<SalesOrderDraftLineInput>[])),
        {
            id: "amount",
            header: "含税小计",
            numeric: true,
            align: "end",
            renderValue: ({ item }) => (
                <MoneyValue
                    value={calculateTotals([item], values.taxRatePercent).gross}
                />
            ),
        },
    ]
}
