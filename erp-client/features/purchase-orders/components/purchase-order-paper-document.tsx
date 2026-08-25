"use client"

import {
    MoneyValue,
    PaperDocument,
    QuantityValue,
    RateValue,
} from "@/components/business"
import {
    FULFILLMENT_RESPONSIBILITY_LABEL,
    PURCHASE_TYPE_LABEL,
    type PurchaseOrderCenterView,
} from "@/features/purchase-orders/types"
import { multiplyFixed } from "@/lib/fixed-decimal"

type PurchaseOrderPaperLine =
    PurchaseOrderCenterView["currentContent"]["lines"][number]

/**
 * 已落库采购单的纸质投影。金额以对象中心当前内容为准，无成本权限时掩码。
 */
export function PurchaseOrderPaperDocument({
    order,
}: {
    order: PurchaseOrderCenterView
}) {
    const { identity, header, currentContent, progress } = order
    const costMasked = currentContent.costMasked
    const documentNumber =
        identity.purchaseNo ?? identity.draftLabel ?? identity.purchaseOrderId

    return (
        <PaperDocument<PurchaseOrderPaperLine>
            frame="bare"
            title="采购单"
            subtitle={PURCHASE_TYPE_LABEL[header.purchaseType]}
            documentNumber={documentNumber}
            status={{
                label: identity.statusLabel,
                tone: identity.statusTone,
            }}
            version={
                identity.revisionNo == null
                    ? undefined
                    : `v${identity.revisionNo}`
            }
            parties={[
                {
                    id: "supplier",
                    label: "供应商",
                    name: header.supplierSnapshot,
                    fields: [
                        {
                            id: "payment",
                            label: "付款条件",
                            value: header.paymentTermLabel,
                        },
                        {
                            id: "owner",
                            label: "采购负责人",
                            value: header.ownerName,
                        },
                    ],
                },
                {
                    id: "source",
                    label: "来源销售单",
                    name: header.salesOrderNo,
                    fields: [
                        {
                            id: "submitted",
                            label: "提交时间",
                            value: header.submittedAt ?? "—",
                            numeric: true,
                        },
                        {
                            id: "expected",
                            label: "预计交期",
                            value: header.expectedDate ?? "—",
                            numeric: true,
                        },
                    ],
                },
            ]}
            metadata={[
                {
                    id: "fulfillment",
                    label: "履约责任",
                    value: FULFILLMENT_RESPONSIBILITY_LABEL[
                        header.fulfillmentResponsibility
                    ],
                },
                {
                    id: "lines",
                    label: "明细行数",
                    value: `${currentContent.lines.length} 行`,
                    numeric: true,
                },
                {
                    id: "payment-progress",
                    label: "付款进度",
                    value: progress.payment,
                },
                {
                    id: "fulfillment-progress",
                    label: "履约进度",
                    value: progress.fulfillment,
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
                            {row.lineType === "LOGISTICS_FEE" ? (
                                <div className="mt-1 text-xs text-muted-foreground">
                                    物流费用
                                    {row.logisticsFeeReason
                                        ? ` · ${row.logisticsFeeReason}`
                                        : ""}
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
                    cell: (row) =>
                        row.lineType === "LOGISTICS_FEE" || !row.quantity ? (
                            "—"
                        ) : (
                            <QuantityValue
                                value={row.quantity}
                                unit={row.unit ?? ""}
                            />
                        ),
                },
                {
                    id: "cost",
                    header: "含税成本",
                    align: "end",
                    numeric: true,
                    cell: (row) =>
                        costMasked ? (
                            <MaskedAmount />
                        ) : (
                            <MoneyValue value={row.unitCostGross} />
                        ),
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
                    cell: (row) =>
                        costMasked ? (
                            <MaskedAmount />
                        ) : (
                            <MoneyValue value={row.grossAmount} />
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
            rows={currentContent.lines}
            getRowId={(row) => row.lineId}
            totals={[
                {
                    id: "net",
                    label: "不含税金额",
                    value: costMasked ? (
                        <MaskedAmount />
                    ) : (
                        <MoneyValue value={currentContent.totals.net} />
                    ),
                },
                {
                    id: "tax",
                    label: "税额",
                    value: costMasked ? (
                        <MaskedAmount />
                    ) : (
                        <MoneyValue value={currentContent.totals.tax} />
                    ),
                },
                {
                    id: "gross",
                    label: "含税合计",
                    value: costMasked ? (
                        <MaskedAmount />
                    ) : (
                        <MoneyValue value={currentContent.totals.gross} />
                    ),
                    emphasized: true,
                },
            ]}
            remarks={
                costMasked
                    ? "当前角色无成本字段权限：金额已隐藏。状态以系统记录为准。"
                    : "系统业务数据的打印件；金额与状态以系统记录为准。"
            }
            signature={
                <div className="space-y-8 text-sm">
                    <div>
                        <div className="text-muted-foreground">采购负责人</div>
                        <div className="mt-6 border-b border-dashed border-border pb-1">
                            {header.ownerName}
                        </div>
                    </div>
                    <div>
                        <div className="text-muted-foreground">日期</div>
                        <div className="num mt-6 border-b border-dashed border-border pb-1">
                            {header.submittedAt?.slice(0, 10) ?? "—"}
                        </div>
                    </div>
                </div>
            }
        />
    )
}

function MaskedAmount() {
    return <span className="text-muted-foreground">•••</span>
}
