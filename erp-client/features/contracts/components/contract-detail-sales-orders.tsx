"use client"

import Link from "next/link"

import {
    BusinessStatusBadge,
    DocumentSection,
    MoneyValue,
    taxAmountToneClass,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import type { ContractCenterView } from "@/features/contracts/types"
import { formatAsOf } from "@/features/contracts/lib/format-as-of"

/** 关联销售单分区：追溯每张销售单使用的合同版本。 */
export function ContractDetailSalesOrders({
    contract,
}: {
    contract: ContractCenterView
}) {
    return (
        <DocumentSection
            title="关联销售单"
            description={
                <>
                    追溯每张销售单使用的合同版本；金额仅作单据摘要。统计截至{" "}
                    <span className="num">
                        {formatAsOf(contract.relatedSalesOrdersAsOf)}
                    </span>
                </>
            }
        >
            {contract.relatedSalesOrders.length === 0 ? (
                <p className="text-sm leading-relaxed text-muted-foreground">
                    暂无关联销售单。
                </p>
            ) : (
                <div className="overflow-hidden rounded-lg border border-border">
                    <Table data-density="compact">
                        <TableHeader>
                            <TableRow>
                                <TableHead>销售单号</TableHead>
                                <TableHead>业务性质</TableHead>
                                <TableHead>合同版本</TableHead>
                                <TableHead>主状态</TableHead>
                                <TableHead>履约 / 回款 / 开票</TableHead>
                                <TableHead data-align="end">含税金额</TableHead>
                                <TableHead data-align="end">操作</TableHead>
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            {contract.relatedSalesOrders.map((so) => (
                                <TableRow key={so.salesOrderId}>
                                    <TableCell className="num font-medium">
                                        {so.documentNumber}
                                    </TableCell>
                                    <TableCell>{so.natureLabel}</TableCell>
                                    <TableCell className="num">
                                        v{so.contractRevisionNo}
                                    </TableCell>
                                    <TableCell>
                                        <BusinessStatusBadge
                                            context="list"
                                            {...so.primaryStatus}
                                        />
                                    </TableCell>
                                    <TableCell className="text-xs text-muted-foreground">
                                        履约 {so.fulfillmentLabel} · 回款{" "}
                                        {so.collectionLabel} · 开票{" "}
                                        {so.invoicingLabel}
                                    </TableCell>
                                    <TableCell data-align="end">
                                        <MoneyValue
                                            value={so.amountGross}
                                            taxBasis="gross"
                                            className={taxAmountToneClass(
                                                "含税金额",
                                            )}
                                        />
                                    </TableCell>
                                    <TableCell data-align="end">
                                        <Button
                                            type="button"
                                            size="xs"
                                            variant="outline"
                                            render={
                                                <Link
                                                    href={`/sales/orders/${so.salesOrderId}`}
                                                />
                                            }
                                        >
                                            打开
                                        </Button>
                                    </TableCell>
                                </TableRow>
                            ))}
                        </TableBody>
                    </Table>
                </div>
            )}
        </DocumentSection>
    )
}
