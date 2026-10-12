"use client"

import {
    MoneyValue,
    PaperDocument,
    QuantityValue,
    RateValue,
    taxAmountToneClass,
} from "@/components/business"
import type {
    PurchaseOrderPreview,
    PurchaseOrderPreviewLine,
    SourcingSalesOrder,
} from "@/features/purchase-orders/lib/purchase-order-create-model"
import {
    FULFILLMENT_RESPONSIBILITY_LABEL,
    PO_STATUS_LABEL,
    PO_STATUS_TONE,
    PURCHASE_TYPE_LABEL,
} from "@/features/purchase-orders/types"
import { multiplyFixed } from "@/lib/fixed-decimal"

/**
 * 把拆单预览投影成纸质采购单，金额与明细以当前选源结果为准。
 */
export function PurchaseOrderPreviewPaper({
    preview,
    index,
    sourceOrder,
}: {
    preview: PurchaseOrderPreview
    index: number
    sourceOrder?: SourcingSalesOrder
}) {
    return (
        <div data-testid={`purchase-create-preview-${preview.supplierId}`}>
            <PaperDocument<PurchaseOrderPreviewLine>
                frame="bare"
                title="采购单"
                subtitle="提交预览"
                documentNumber={`预览 ${index + 1}`}
                status={{
                    label: PO_STATUS_LABEL.PENDING_REVIEW,
                    tone: PO_STATUS_TONE.PENDING_REVIEW,
                }}
                parties={[
                    {
                        id: "supplier",
                        label: "供应商",
                        name: preview.supplierName,
                        fields: [
                            {
                                id: "payment",
                                label: "付款条件",
                                value: preview.paymentTermLabel,
                            },
                            {
                                id: "type",
                                label: "采购类型",
                                value: PURCHASE_TYPE_LABEL[
                                    preview.purchaseType
                                ],
                            },
                        ],
                    },
                    {
                        id: "source",
                        label: "来源销售单",
                        name: sourceOrder?.customerName ?? "—",
                        reference: sourceOrder?.salesOrderNo,
                        fields: [
                            {
                                id: "contract",
                                label: "合同",
                                value: sourceOrder?.contractNumber ?? "无合同",
                                numeric: true,
                            },
                            {
                                id: "owner",
                                label: "负责销售",
                                value: sourceOrder?.salesOwnerName ?? "—",
                            },
                        ],
                    },
                ]}
                metadata={[
                    {
                        id: "fulfillment",
                        label: "履约责任",
                        value: FULFILLMENT_RESPONSIBILITY_LABEL[
                            preview.fulfillmentResponsibility
                        ],
                    },
                    ...(preview.fulfillmentResponsibility === "WAREHOUSE"
                        ? [
                              {
                                  id: "target-warehouse",
                                  label: "采购入库目标仓",
                                  value:
                                      preview.targetWarehouseName?.trim() ||
                                      "仓库名称未提供",
                              },
                          ]
                        : []),
                    {
                        id: "lines",
                        label: "明细行数",
                        value: `${preview.lines.length} 行`,
                        numeric: true,
                    },
                    {
                        id: "delivery",
                        label: "预计交期",
                        value: earliestDeliveryDate(preview.lines),
                        numeric: true,
                    },
                    {
                        id: "gross",
                        label: "含税合计",
                        value: <MoneyValue value={preview.totals.gross} />,
                    },
                ]}
                lineItemLabel="采购明细"
                columns={[
                    {
                        id: "item",
                        header: "采购项目",
                        cell: (row) => (
                            <div>
                                <div>{row.itemName}</div>
                                {row.itemSku ? (
                                    <div className="num mt-1 text-xs text-muted-foreground">
                                        {row.itemSku}
                                    </div>
                                ) : null}
                            </div>
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
                                unit={row.unit}
                            />
                        ),
                    },
                    {
                        id: "cost",
                        header: "含税成本",
                        align: "end",
                        numeric: true,
                        cell: (row) => <MoneyValue value={row.unitCostGross} />,
                    },
                    {
                        id: "tax",
                        header: "进项税率",
                        align: "end",
                        numeric: true,
                        cell: (row) => (
                            <RateValue
                                value={multiplyFixed(row.inputTaxRate, "100", {
                                    leftMaxScale: 6,
                                    rightMaxScale: 0,
                                    outputScale: 2,
                                })}
                                precision={2}
                            />
                        ),
                    },
                    {
                        id: "amount",
                        header: "含税金额",
                        align: "end",
                        numeric: true,
                        cell: (row) => (
                            <MoneyValue
                                value={row.grossAmount}
                                className={taxAmountToneClass("含税金额")}
                            />
                        ),
                    },
                    {
                        id: "due",
                        header: "预计交期",
                        align: "end",
                        numeric: true,
                        cell: (row) => row.expectedDeliveryDate || "—",
                    },
                ]}
                rows={preview.lines}
                getRowId={(row) => row.salesOrderLineId}
                totals={[
                    {
                        id: "net",
                        label: "不含税金额",
                        value: <MoneyValue value={preview.totals.net} />,
                    },
                    {
                        id: "tax",
                        label: "税额",
                        value: <MoneyValue value={preview.totals.tax} />,
                    },
                    {
                        id: "gross",
                        label: "含税合计",
                        value: <MoneyValue value={preview.totals.gross} />,
                        emphasized: true,
                    },
                ]}
                remarks="本预览按当前选源结果拆单，确认后创建采购单并提交审批，金额以系统计算为准。"
            />
        </div>
    )
}

function earliestDeliveryDate(
    lines: readonly PurchaseOrderPreviewLine[],
): string {
    const dates = lines
        .map((line) => line.expectedDeliveryDate.trim())
        .filter(Boolean)
        .sort()
    return dates[0] ?? "—"
}
