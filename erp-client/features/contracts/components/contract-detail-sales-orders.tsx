"use client"

import Link from "next/link"

import {
    BusinessStatusBadge,
    DetailInlineTable,
    DetailWorkspaceSection,
    MoneyValue,
    detailWorkspaceStyles,
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
import { toAutomationIdSegment } from "@/lib/automation-id"

/** 关联销售单分区：追溯每张销售单使用的合同版本。 */
export function ContractDetailSalesOrders({
    contract,
}: {
    contract: ContractCenterView
}) {
    return (
        <DetailWorkspaceSection
            title="关联销售单"
            subtitle={
                <>
                    追溯每张销售单使用的合同版本；金额仅作单据摘要。统计截至{" "}
                    <span className="num">
                        {formatAsOf(contract.relatedSalesOrdersAsOf)}
                    </span>
                </>
            }
        >
            {contract.relatedSalesOrders.length === 0 ? (
                <p className="text-body-compact text-muted-foreground">
                    暂无关联销售单。
                </p>
            ) : (
                <DetailInlineTable>
                    <Table data-density="compact">
                        <TableHeader>
                            <TableRow>
                                <TableHead>销售单号</TableHead>
                                <TableHead>业务性质</TableHead>
                                <TableHead>合同版本</TableHead>
                                <TableHead>单据状态</TableHead>
                                <TableHead>履约 / 回款 / 开票</TableHead>
                                <TableHead data-align="end">含税金额</TableHead>
                                <TableHead data-align="end">操作</TableHead>
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            {contract.relatedSalesOrders.map((so) => (
                                <TableRow key={so.salesOrderId}>
                                    <TableCell className="num font-medium">
                                        <Link
                                            id={`card-contracts-detail-sales-orders-row-${toAutomationIdSegment(so.salesOrderId)}-no`}
                                            href={`/sales/orders/${encodeURIComponent(so.salesOrderId)}`}
                                            className="text-primary underline-offset-4 hover:underline"
                                        >
                                            {so.documentNumber}
                                        </Link>
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
                                    <TableCell className="text-muted-foreground">
                                        <span className="text-xs">
                                            履约 {so.fulfillmentLabel} · 回款{" "}
                                            {so.collectionLabel} · 开票{" "}
                                            {so.invoicingLabel}
                                        </span>
                                    </TableCell>
                                    <TableCell data-align="end">
                                        <span
                                            className={taxAmountToneClass(
                                                "含税金额",
                                            )}
                                        >
                                            <MoneyValue
                                                value={so.amountGross}
                                                taxBasis="gross"
                                                inheritColor
                                            />
                                        </span>
                                    </TableCell>
                                    <TableCell data-align="end">
                                        <Button
                                            id={`card-contracts-detail-sales-orders-row-${toAutomationIdSegment(so.salesOrderId)}-open`}
                                            type="button"
                                            size="sm"
                                            variant="ghost"
                                            className={
                                                detailWorkspaceStyles.inlineLinkButton
                                            }
                                            render={
                                                <Link
                                                    id={`card-contracts-detail-sales-orders-row-${toAutomationIdSegment(so.salesOrderId)}-open-link`}
                                                    href={`/sales/orders/${encodeURIComponent(so.salesOrderId)}`}
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
                </DetailInlineTable>
            )}
        </DetailWorkspaceSection>
    )
}
