"use client"

import { DocumentTotals, MoneyValue, QuantityValue } from "@/components/business"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import type { PurchaseOrderCenterView } from "@/features/purchase-orders/types"
import { SectionTitle } from "@/features/purchase-orders/components/purchase-order-preview-overview"

/** 预览右列：采购明细行表与系统金额合计。 */
export function PurchaseOrderPreviewLines({
    order,
}: {
    order: PurchaseOrderCenterView
}) {
    const { currentContent } = order
    const costMasked = currentContent.costMasked

    return (
        <div className="flex flex-col gap-4 p-4 md:p-5">
            <section className="space-y-2" aria-label="采购明细">
                <div className="flex items-center justify-between gap-2">
                    <SectionTitle>采购明细</SectionTitle>
                    <span className="text-xs text-muted-foreground">
                        {currentContent.lines.length} 行 · 来源{" "}
                        {currentContent.source === "DRAFT"
                            ? "草稿"
                            : currentContent.source === "SUBMISSION"
                              ? "已提交内容"
                              : "生效版本"}
                    </span>
                </div>
                <div className="overflow-hidden rounded-lg border border-border">
                    <Table data-density="compact">
                        <TableHeader>
                            <TableRow>
                                <TableHead>项目</TableHead>
                                <TableHead className="hidden md:table-cell">
                                    类型
                                </TableHead>
                                <TableHead data-align="end">数量</TableHead>
                                <TableHead
                                    data-align="end"
                                    className="hidden sm:table-cell"
                                >
                                    含税单价
                                </TableHead>
                                <TableHead data-align="end">行含税</TableHead>
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            {currentContent.lines.map((line) => (
                                <TableRow key={line.lineId}>
                                    <TableCell className="max-w-[14rem] whitespace-normal">
                                        <div className="font-medium text-foreground">
                                            {line.itemName}
                                        </div>
                                        {line.itemSku ? (
                                            <div className="num mt-0.5 text-xs text-muted-foreground">
                                                {line.itemSku}
                                            </div>
                                        ) : null}
                                        {line.procurementConfirmationLineId ? (
                                            <div className="mt-0.5 text-tiny text-muted-foreground">
                                                {line.salesAllocationLabel ??
                                                    `确认分行 · ${line.itemName}`}
                                            </div>
                                        ) : null}
                                        {line.logisticsFeeReason ? (
                                            <div className="mt-0.5 text-tiny text-muted-foreground">
                                                {line.logisticsFeeReason}
                                            </div>
                                        ) : null}
                                    </TableCell>
                                    <TableCell className="hidden text-xs text-muted-foreground md:table-cell">
                                        {line.lineType === "LOGISTICS_FEE"
                                            ? "物流费用"
                                            : "商品/服务"}
                                    </TableCell>
                                    <TableCell data-align="end">
                                        {line.lineType === "LOGISTICS_FEE" ? (
                                            "—"
                                        ) : (
                                            <QuantityValue
                                                value={line.quantity ?? "0"}
                                                unit={line.unit}
                                            />
                                        )}
                                    </TableCell>
                                    <TableCell
                                        data-align="end"
                                        className="hidden sm:table-cell"
                                    >
                                        {costMasked ? (
                                            <span className="text-muted-foreground">
                                                •••
                                            </span>
                                        ) : (
                                            <MoneyValue
                                                value={line.unitCostGross}
                                            />
                                        )}
                                    </TableCell>
                                    <TableCell data-align="end">
                                        {costMasked ? (
                                            <span className="text-muted-foreground">
                                                •••
                                            </span>
                                        ) : (
                                            <MoneyValue
                                                value={line.grossAmount}
                                                taxBasis="gross"
                                            />
                                        )}
                                    </TableCell>
                                </TableRow>
                            ))}
                        </TableBody>
                    </Table>
                </div>
                {costMasked ? (
                    <p className="text-tiny text-muted-foreground">
                        当前角色无成本字段权限：金额已隐藏。
                    </p>
                ) : null}
            </section>

            <DocumentTotals
                title="金额合计（系统计算）"
                className="max-w-md self-end"
                items={[
                    {
                        id: "gross",
                        label: "含税金额",
                        value: costMasked ? (
                            "•••"
                        ) : (
                            <MoneyValue value={currentContent.totals.gross} />
                        ),
                        basis: "含税",
                    },
                    {
                        id: "net",
                        label: "不含税金额",
                        value: costMasked ? (
                            "•••"
                        ) : (
                            <MoneyValue value={currentContent.totals.net} />
                        ),
                        basis: "不含税",
                    },
                    {
                        id: "tax",
                        label: "税额",
                        value: costMasked ? (
                            "•••"
                        ) : (
                            <MoneyValue value={currentContent.totals.tax} />
                        ),
                    },
                    ...(order.payableSummary
                        ? [
                              {
                                  id: "payable",
                                  label: "应付未结",
                                  value: costMasked ? (
                                      "•••"
                                  ) : (
                                      <MoneyValue
                                          value={
                                              order.payableSummary
                                                  .payableOpenAmount
                                          }
                                      />
                                  ),
                                  basis: "含税" as const,
                              },
                              {
                                  id: "paid",
                                  label: "已付核销",
                                  value: costMasked ? (
                                      "•••"
                                  ) : (
                                      <MoneyValue
                                          value={
                                              order.payableSummary
                                                  .paidAllocatedAmount
                                          }
                                      />
                                  ),
                              },
                          ]
                        : []),
                ]}
                warning={
                    costMasked
                        ? "销售/仓储角色成本已隐藏"
                        : currentContent.source === "SUBMISSION"
                          ? "当前为已提交内容，审核不得改字段"
                          : undefined
                }
            />
        </div>
    )
}
