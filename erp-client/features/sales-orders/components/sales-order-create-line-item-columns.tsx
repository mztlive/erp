"use client"

import { CircleHelpIcon } from "lucide-react"

import {
    MoneyValue,
    taxAmountToneClass,
    type EditableLineItemColumn,
} from "@/components/business"
import { DetailHint } from "@/components/business/detail-presentation"
import { StatusBadge } from "@/components/ui/status-badge"
import {
    SalesOrderCreateLineGiftToggle,
    SalesOrderCreateLinePriceEditor,
} from "@/features/sales-orders/components/sales-order-create-line-price-editor"
import { SalesOrderCreateLineSkuEditor } from "@/features/sales-orders/components/sales-order-create-line-sku-editor"
import { SkuThumbnail } from "@/features/sales-orders/components/sku-thumbnail"
import type { SalesOrderCreateFormApi } from "@/features/sales-orders/lib/sales-order-create-form-types"
import type { CreateSalesOrderFormValues } from "@/features/sales-orders/lib/sales-order-create-model"
import {
    CARD_FORM_OPTIONS,
    calculateTotals,
    deriveVoucherGiftPreview,
} from "@/features/sales-orders/lib/sales-order-create-model"
import type {
    SalesLineProcurementResponsibility,
    SalesOrderDraftLineInput,
} from "@/features/sales-orders/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    SalesProcurementBasisEditor,
    SalesProcurementPriceEditor,
} from "./sales-order-procurement-price-editor"

function ProcurementOwnerPreview({
    line,
    responsibility,
    status,
}: {
    line: SalesOrderDraftLineInput
    responsibility?: SalesLineProcurementResponsibility
    status: { fetching: boolean; error: boolean }
}) {
    return (
        <div
            className="flex min-h-control w-24 items-center"
            data-testid={`sales-line-procurement-owner-${line.rowKey}`}
        >
            {!line.sku.trim() ? (
                <span className="text-xs text-muted-foreground">待选商品</span>
            ) : status.fetching ? (
                <span className="text-xs text-muted-foreground">匹配中…</span>
            ) : responsibility?.resolved && responsibility.ownerName ? (
                <span
                    className="truncate font-medium text-foreground"
                    title={responsibility.ownerName}
                >
                    {responsibility.ownerName}
                </span>
            ) : status.error ? (
                <StatusBadge
                    tone="destructive"
                    appearance="text"
                    label="匹配失败"
                    title="采购负责人暂时无法匹配，请稍后重试"
                />
            ) : (
                <StatusBadge
                    tone="destructive"
                    appearance="text"
                    label="待配置"
                    title="暂未匹配采购负责人，请联系管理员维护采购责任规则"
                />
            )}
        </div>
    )
}

/** 按当前业务性质与表单值组装明细编辑列。 */
export function buildSalesOrderCreateLineItemColumns(
    values: CreateSalesOrderFormValues,
    form: SalesOrderCreateFormApi,
    procurementOwners: ReadonlyMap<
        string,
        SalesLineProcurementResponsibility
    > = new Map(),
    onPickSku?: (rowIndex: number) => void,
    procurementStatus = { fetching: false, error: false },
    inquiryLocked = false,
    skuImages: ReadonlyMap<string, string | undefined> = new Map(),
): EditableLineItemColumn<SalesOrderDraftLineInput>[] {
    const nature = values.nature
    return [
        {
            id: "item",
            header: "销售项目",
            required: true,
            renderValue: ({ item }) => item.name,
            renderEditor: ({ rowIndex }) =>
                inquiryLocked ? (
                    <div className="flex min-w-48 max-w-xs items-center gap-2.5 py-0.5">
                        <SkuThumbnail
                            assetId={skuImages.get(
                                values.lineItems[rowIndex]?.sku ?? "",
                            )}
                            label={values.lineItems[rowIndex]?.name ?? "商品"}
                            className="size-9 rounded-md"
                        />
                        <div className="min-w-0 whitespace-normal break-words">
                            <p className="font-medium text-foreground">
                                {values.lineItems[rowIndex]?.name}
                            </p>
                            {values.lineItems[rowIndex]?.specification ? (
                                <p className="mt-0.5 text-xs text-muted-foreground">
                                    {values.lineItems[rowIndex]?.specification}
                                </p>
                            ) : null}
                        </div>
                    </div>
                ) : (
                    <SalesOrderCreateLineSkuEditor
                        form={form}
                        values={values}
                        nature={nature}
                        rowIndex={rowIndex}
                        onPickSku={onPickSku}
                        imageAssetId={skuImages.get(
                            values.lineItems[rowIndex]?.sku ?? "",
                        )}
                    />
                ),
        },
        {
            id: "quantity",
            header: "数量 / 单位",
            required: true,
            numeric: true,
            align: "end",
            renderValue: ({ item }) => `${item.quantity} ${item.unit}`,
            renderEditor: ({ rowIndex }) => {
                const line = values.lineItems[rowIndex]
                const lineKey = line?.rowKey
                if (!lineKey) return null
                const unitLocked =
                    nature === "card_voucher" || Boolean(line?.sku?.trim())
                return (
                    <div className="flex min-w-24 items-center justify-end gap-1.5">
                        <div className="w-18 shrink-0">
                            <form.AppField
                                name={`lineItems[${rowIndex}].quantity`}
                                listeners={{
                                    // 只有一个收货地址时，该地址承担全部数量。
                                    onChange: ({ value }) => {
                                        const allocations =
                                            form.getFieldValue(
                                                `lineItems[${rowIndex}].deliveryAllocations`,
                                            ) ?? []
                                        if (allocations.length !== 1) return
                                        form.setFieldValue(
                                            `lineItems[${rowIndex}].deliveryAllocations[0].quantity`,
                                            value,
                                        )
                                    },
                                }}
                            >
                                {(field) => (
                                    <field.TextField
                                        id={`sales-orders-create-line-${toAutomationIdSegment(lineKey)}-quantity`}
                                        label="数量"
                                        required
                                        hideLabel
                                        type="text"
                                        inputMode="decimal"
                                        className="gap-0"
                                        inputClassName="num text-right"
                                    />
                                )}
                            </form.AppField>
                        </div>
                        <form.AppField name={`lineItems[${rowIndex}].unit`}>
                            {(field) => (
                                <span
                                    className="min-w-6 shrink-0 text-left text-xs text-muted-foreground"
                                    title={
                                        unitLocked
                                            ? nature === "card_voucher"
                                                ? "卡券单位固定为张"
                                                : "单位随商品基础单位带出，不可改"
                                            : "选择商品后带出基础单位"
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
            header:
                nature === "physical_service" ? (
                    <span className="inline-flex items-center justify-end gap-0.5">
                        含税成交单价
                        <DetailHint
                            id="sales-orders-create-unit-price-hint"
                            label="含税成交单价"
                            icon={
                                <CircleHelpIcon
                                    className="size-3.5"
                                    aria-hidden="true"
                                />
                            }
                        >
                            {inquiryLocked
                                ? "优先带入询价销售价，可修改；未填销售价的商品须补填。赠品单价为 0、不收款。"
                                : "由销售填写含税成交单价；勾选赠品后单价固定为 0、不收款。"}
                        </DetailHint>
                    </span>
                ) : (
                    "含税成交单价"
                ),
            required: true,
            numeric: true,
            align: "end",
            renderValue: ({ item }) => item.unitPriceGross,
            renderEditor: ({ rowIndex }) => {
                const lineKey = values.lineItems[rowIndex]?.rowKey
                if (!lineKey) return null
                return (
                    <SalesOrderCreateLinePriceEditor
                        form={form}
                        rowIndex={rowIndex}
                    />
                )
            },
        },
        ...(nature === "physical_service"
            ? ([
                  {
                      id: "isGift",
                      header: "赠品",
                      align: "center",
                      renderValue: ({ item }) =>
                          item.pricingMode === "GIFT" ? "是" : "—",
                      renderEditor: ({ rowIndex }) => (
                          <SalesOrderCreateLineGiftToggle
                              form={form}
                              rowIndex={rowIndex}
                          />
                      ),
                  },
                  {
                      id: "procurementPriceBasis",
                      header: "取价方式",
                      renderValue: ({ item }) =>
                          inquiryLocked
                              ? "询价覆盖"
                              : item.procurementPriceBasis === "BULK"
                                ? "集采"
                                : item.procurementPriceBasis === "DROPSHIP"
                                  ? "一件代发"
                                  : "未选择",
                      renderEditor: ({ rowIndex }) => (
                          <SalesProcurementBasisEditor
                              form={form}
                              rowIndex={rowIndex}
                              inquiryLocked={inquiryLocked}
                          />
                      ),
                  },
                  {
                      id: "procurementSupplyPrice",
                      header: (
                          <span className="inline-flex items-center justify-end gap-0.5">
                              采购供给单价
                              <DetailHint
                                  id="sales-orders-create-supply-price-hint"
                                  label="采购供给单价"
                                  icon={
                                      <CircleHelpIcon
                                          className="size-3.5"
                                          aria-hidden="true"
                                      />
                                  }
                              >
                                  {inquiryLocked
                                      ? "由询价结果覆盖，不可修改；按单价乘数量估算含税毛利。"
                                      : "按所选集采或一件代发参考价带出，可手工调整；用于估算含税毛利。"}
                              </DetailHint>
                          </span>
                      ),
                      required: true,
                      numeric: true,
                      align: "end",
                      renderValue: ({ item }) =>
                          item.procurementSupplyPrice || "—",
                      renderEditor: ({ rowIndex }) => (
                          <SalesProcurementPriceEditor
                              form={form}
                              rowIndex={rowIndex}
                              inquiryLocked={inquiryLocked}
                          />
                      ),
                  },
              ] satisfies EditableLineItemColumn<SalesOrderDraftLineInput>[])
            : []),
        ...(nature === "card_voucher"
            ? ([
                  {
                      id: "faceValue",
                      header: "面值",
                      required: true,
                      numeric: true,
                      align: "end",
                      renderValue: ({ item }) => item.faceValue || "—",
                      renderEditor: ({ rowIndex }) => {
                          const lineKey = values.lineItems[rowIndex]?.rowKey
                          if (!lineKey) return null
                          return (
                              <div className="min-w-24">
                                  <form.AppField
                                      name={`lineItems[${rowIndex}].faceValue`}
                                  >
                                      {(field) => (
                                          <field.TextField
                                              money
                                              id={`sales-orders-create-line-${toAutomationIdSegment(lineKey)}-face-value`}
                                              label="面值"
                                              required
                                              hideLabel
                                              type="text"
                                              inputMode="decimal"
                                              placeholder="0.00"
                                              className="gap-0"
                                              inputClassName="num min-w-24 text-right"
                                          />
                                      )}
                                  </form.AppField>
                              </div>
                          )
                      },
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
                                  className="num flex min-h-control min-w-16 items-center justify-end text-muted-foreground"
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
                      required: true,
                      renderValue: ({ item }) => item.cardForm || "—",
                      renderEditor: ({ rowIndex }) => {
                          const lineKey = values.lineItems[rowIndex]?.rowKey
                          if (!lineKey) return null
                          return (
                              <div className="w-28">
                                  <form.AppField
                                      name={`lineItems[${rowIndex}].cardForm`}
                                  >
                                      {(field) => (
                                          <field.SelectField
                                              id={`sales-orders-create-line-${toAutomationIdSegment(lineKey)}-card-form`}
                                              label="卡形态"
                                              required
                                              hideLabel
                                              options={CARD_FORM_OPTIONS}
                                              allowClear={false}
                                              className="gap-0"
                                              inputClassName="w-full"
                                          />
                                      )}
                                  </form.AppField>
                              </div>
                          )
                      },
                  },
              ] satisfies EditableLineItemColumn<SalesOrderDraftLineInput>[])
            : ([
                  {
                      id: "fulfillment",
                      header: "期望交付日",
                      required: true,
                      renderValue: ({ item }) => item.dueDate || "—",
                      renderEditor: ({ rowIndex }) => {
                          const lineKey = values.lineItems[rowIndex]?.rowKey
                          if (!lineKey) return null
                          return (
                              <div className="w-36">
                                  <form.AppField
                                      name={`lineItems[${rowIndex}].dueDate`}
                                  >
                                      {(field) => (
                                          <field.DateField
                                              id={`sales-orders-create-line-${toAutomationIdSegment(lineKey)}-due-date`}
                                              label="期望交付日"
                                              disabled={inquiryLocked}
                                              required
                                              size="default"
                                              hideLabel
                                              className="gap-0"
                                              inputClassName="num"
                                          />
                                      )}
                                  </form.AppField>
                              </div>
                          )
                      },
                  },
              ] satisfies EditableLineItemColumn<SalesOrderDraftLineInput>[])),
        ...(nature === "physical_service"
            ? ([
                  {
                      id: "procurementOwner",
                      header: "采购负责人",
                      renderValue: ({ item }) => (
                          <ProcurementOwnerPreview
                              line={item}
                              status={procurementStatus}
                              responsibility={procurementOwners.get(
                                  item.rowKey,
                              )}
                          />
                      ),
                      renderEditor: ({ item }) => (
                          <ProcurementOwnerPreview
                              line={item}
                              status={procurementStatus}
                              responsibility={procurementOwners.get(
                                  item.rowKey,
                              )}
                          />
                      ),
                  },
              ] satisfies EditableLineItemColumn<SalesOrderDraftLineInput>[])
            : []),
        {
            id: "amount",
            header: "含税小计",
            numeric: true,
            align: "end",
            renderValue: ({ item }) => (
                <div className="flex min-h-control items-center justify-end font-medium">
                    <MoneyValue
                        value={
                            calculateTotals([item], values.taxRatePercent).gross
                        }
                        className={taxAmountToneClass("含税小计")}
                    />
                </div>
            ),
        },
    ]
}
