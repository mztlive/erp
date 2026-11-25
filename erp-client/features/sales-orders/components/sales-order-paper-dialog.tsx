"use client"

import { MoneyValue, PaperDocument, QuantityValue } from "@/components/business"
import { DeliveryAllocationPrintAppendix } from "@/features/regions"
import { welfareScenarioLabel } from "@/lib/business-options"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    buildDeliveryAllocationSummary,
    formatLineDeliveryHint,
    type DeliveryAllocationLineInput,
} from "@/lib/delivery-allocation-summary"
import { NATURE_LABEL, ORIGIN_LABEL } from "@/features/sales-orders/lib/labels"
import type {
    SalesOrderLineItem,
    SalesOrderListItem,
} from "@/features/sales-orders/types"

export function SalesOrderPaperDocument({
    order,
    compact = false,
    onInspectAddresses,
}: {
    order: SalesOrderListItem
    compact?: boolean
    onInspectAddresses?: (lineId?: string) => void
}) {
    const isCard = order.nature === "card_voucher"
    const deliveryLines: DeliveryAllocationLineInput[] = order.lineItems.map(
        (row, index) => ({
            id: row.id,
            lineNo: index + 1,
            itemName: row.name,
            specification: row.sku,
            unit: row.unitName ?? "",
            quantity: row.quantity,
            allocations: row.deliveryAllocations,
        }),
    )
    const deliverySummary = buildDeliveryAllocationSummary(deliveryLines)
    const deliveryItemById = new Map(
        deliverySummary.itemGroups.map((group) => [group.lineId, group]),
    )

    const renderLineName = (row: SalesOrderLineItem) => {
        const group = deliveryItemById.get(row.id)
        const hint = formatLineDeliveryHint(group, deliverySummary)
        return (
            <div>
                <div className="font-medium text-foreground">{row.name}</div>
                {row.sku ? (
                    <div className="num mt-0.5 text-xs text-muted-foreground">
                        {row.sku}
                    </div>
                ) : null}
                {hint ||
                (onInspectAddresses &&
                    group &&
                    group.uniqueAddressCount > 0) ? (
                    <p className="mt-0.5 text-xs text-muted-foreground">
                        {hint ? <span>收货：{hint}</span> : null}
                        {onInspectAddresses &&
                        group &&
                        group.uniqueAddressCount > 0 ? (
                            <button
                                id={`sales-order-paper-line-${toAutomationIdSegment(row.id)}-addresses`}
                                type="button"
                                className={
                                    hint
                                        ? "ml-1.5 inline cursor-pointer text-xs font-medium text-primary hover:underline"
                                        : "inline cursor-pointer text-xs font-medium text-primary hover:underline"
                                }
                                onClick={() => onInspectAddresses(row.id)}
                            >
                                查看明细
                            </button>
                        ) : null}
                    </p>
                ) : null}
            </div>
        )
    }

    return (
        <PaperDocument<SalesOrderLineItem>
            frame="borderless"
            compact={compact}
            issuer={order.sellerEntity}
            title="销售单"
            subtitle={NATURE_LABEL[order.nature]}
            documentNumber={order.documentNumber}
            status={order.primaryStatus}
            version={
                order.currentRevisionNo == null
                    ? "尚未生效"
                    : `v${order.currentRevisionNo}`
            }
            parties={[
                {
                    id: "seller",
                    label: "销售方",
                    name: order.sellerEntity,
                    reference: "结算主体",
                    fields: [
                        {
                            id: "owner",
                            label: "业务负责人",
                            value: order.ownerName,
                        },
                        {
                            id: "submitted",
                            label: "提交时间",
                            value: order.submittedAt,
                            numeric: true,
                        },
                    ],
                },
                {
                    id: "buyer",
                    label: "客户",
                    name: order.customerName,
                    reference: order.contractNumber,
                    fields: [
                        {
                            id: "settlement",
                            label: "结算主体",
                            value: order.settlementEntity,
                        },
                        {
                            id: "contact",
                            label: "联系人",
                            value: order.customerContact ?? "—",
                        },
                    ],
                },
            ]}
            metadata={[
                {
                    id: "payment",
                    label: "付款条件",
                    value: order.paymentTerms,
                },
                {
                    id: "deadline",
                    label: isCard ? "卡券履约期限" : "客户承诺期限摘要",
                    value: order.fulfillmentDeadline,
                    numeric: true,
                },
                {
                    id: "scene",
                    label: "福利场景",
                    value: welfareScenarioLabel(order.welfareScene),
                },
                {
                    id: "origin-system",
                    label: "来源",
                    value: ORIGIN_LABEL[order.originSystem],
                },
            ]}
            lineItemLabel={isCard ? "卡券明细（唯一）" : "销售明细"}
            columns={
                isCard
                    ? [
                          {
                              id: "name",
                              header: "卡券类目",
                              cell: renderLineName,
                          },
                          {
                              id: "face",
                              header: "面额",
                              align: "end",
                              numeric: true,
                              cell: (row) =>
                                  row.faceValue ? (
                                      <MoneyValue
                                          display="exact"
                                          value={row.faceValue}
                                      />
                                  ) : (
                                      "—"
                                  ),
                          },
                          {
                              id: "qty",
                              header: "数量",
                              align: "end",
                              numeric: true,
                              cell: (row) => (
                                  <QuantityValue
                                      value={row.quantity}
                                      unit={row.unitName ?? ""}
                                  />
                              ),
                          },
                          {
                              id: "form",
                              header: "形态",
                              cell: (row) => row.cardForm ?? "—",
                          },
                          {
                              id: "gift",
                              header: "配赠率",
                              align: "end",
                              numeric: true,
                              cell: (row) =>
                                  row.giftRate != null
                                      ? `${row.giftRate}%`
                                      : "—",
                          },
                          {
                              id: "amount",
                              header: "成交金额（含税）",
                              align: "end",
                              numeric: true,
                              cell: (row) => (
                                  <MoneyValue
                                      display="exact"
                                      value={row.amountGross}
                                  />
                              ),
                          },
                      ]
                    : [
                          {
                              id: "name",
                              header: "项目",
                              cell: renderLineName,
                          },
                          {
                              id: "due",
                              header: "期望交付日",
                              numeric: true,
                              cell: (row) => row.dueDate ?? "—",
                          },
                          {
                              id: "qty",
                              header: "数量",
                              align: "end",
                              numeric: true,
                              cell: (row) => (
                                  <QuantityValue
                                      value={row.quantity}
                                      unit={row.unitName ?? ""}
                                  />
                              ),
                          },
                          {
                              id: "price",
                              header: "单价（含税）",
                              align: "end",
                              numeric: true,
                              cell: (row) => (
                                  <MoneyValue
                                      display="exact"
                                      value={row.unitPriceGross}
                                  />
                              ),
                          },
                          {
                              id: "amount",
                              header: "小计（含税）",
                              align: "end",
                              numeric: true,
                              cell: (row) => (
                                  <MoneyValue
                                      display="exact"
                                      value={row.amountGross}
                                  />
                              ),
                          },
                      ]
            }
            rows={order.lineItems}
            getRowId={(row) => row.id}
            totals={[
                {
                    id: "net",
                    label: "不含税金额",
                    value: (
                        <MoneyValue display="exact" value={order.amountNet} />
                    ),
                },
                {
                    id: "tax",
                    label: "税额",
                    value: (
                        <MoneyValue display="exact" value={order.taxAmount} />
                    ),
                },
                {
                    id: "gross",
                    label: "成交金额（含税）",
                    value: (
                        <MoneyValue display="exact" value={order.amountGross} />
                    ),
                    emphasized: true,
                },
                {
                    id: "received",
                    label: "已回款（含税）",
                    value: (
                        <MoneyValue
                            display="exact"
                            value={order.receivedAmount}
                        />
                    ),
                    description: order.collection.label,
                },
                {
                    id: "invoiced",
                    label: "已开票",
                    value: (
                        <MoneyValue
                            display="exact"
                            value={order.invoicedAmount}
                        />
                    ),
                    description: order.invoicing.label,
                },
            ]}
            remarks={
                order.remark ??
                (isCard
                    ? "卡券履约在福利商城执行；本单据仅展示系统内的销售数据。"
                    : undefined)
            }
            signature={
                compact ? (
                    <div className="flex flex-wrap items-center gap-x-6 gap-y-1 text-xs text-muted-foreground">
                        <span>业务负责人：{order.ownerName}</span>
                        <span className="num">
                            提交日期：{order.submittedAt.slice(0, 10)}
                        </span>
                    </div>
                ) : (
                    <div className="space-y-8 text-body-compact">
                        <div>
                            <div className="text-muted-foreground">
                                业务负责人
                            </div>
                            <div className="mt-6 border-b border-dashed border-border pb-1">
                                {order.ownerName}
                            </div>
                        </div>
                        <div>
                            <div className="text-muted-foreground">日期</div>
                            <div className="num mt-6 border-b border-dashed border-border pb-1">
                                {order.submittedAt.slice(0, 10)}
                            </div>
                        </div>
                    </div>
                )
            }
            seal={
                <div
                    className={
                        compact
                            ? "hidden h-24 w-24 items-center justify-center rounded-full border-2 border-dashed border-muted-foreground/40 text-center text-xs text-muted-foreground print:flex"
                            : "flex h-28 w-28 items-center justify-center rounded-full border-2 border-dashed border-muted-foreground/40 text-center text-xs text-muted-foreground"
                    }
                >
                    公司签章
                    <br />
                    签章位
                </div>
            }
            appendix={
                !onInspectAddresses && deliverySummary.totalRows > 0 ? (
                    <DeliveryAllocationPrintAppendix lines={deliveryLines} />
                ) : undefined
            }
        />
    )
}
