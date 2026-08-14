"use client"

import Link from "next/link"

import {
    BusinessStatusBadge,
    DocumentSummary,
    surfacePanelClassName,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import { contractOwnerLabel } from "@/features/contracts/types"
import type { ContractCenterView } from "@/features/contracts/types"
import { formatAsOf } from "@/features/contracts/lib/format-as-of"

/** 概览分区：合同身份与关联销售摘要（不含合同级金额）。 */
export function ContractDetailOverview({
    contract,
}: {
    contract: ContractCenterView
}) {
    const rev = contract.currentRevision
    const baseHref = `/sales/contracts/${contract.contractId}`

    return (
        <div className="grid gap-4 lg:grid-cols-2">
            <Card size="sm" className={surfacePanelClassName}>
                <CardHeader className="border-b border-border/30">
                    <CardTitle>概览</CardTitle>
                    <CardDescription>
                        展示合同身份、客户、状态、版本与有效期；不含合同级金额。
                    </CardDescription>
                </CardHeader>
                <CardContent>
                    <DocumentSummary
                        columns="two"
                        items={[
                            {
                                id: "no",
                                label: "合同编号",
                                value: contract.contractNo,
                                numeric: true,
                            },
                            {
                                id: "status",
                                label: "状态",
                                value: (
                                    <BusinessStatusBadge
                                        context="preview"
                                        label={contract.statusLabel}
                                        tone={contract.statusTone}
                                    />
                                ),
                            },
                            {
                                id: "customer",
                                label: "客户",
                                value: contract.customer.displayName,
                            },
                            {
                                id: "settlement",
                                label: "结算主体",
                                value: rev.settlementParty.displayName,
                            },
                            {
                                id: "signed",
                                label: "签订日",
                                value: rev.signedAt ?? "—",
                                numeric: true,
                            },
                            {
                                id: "valid",
                                label: "有效期",
                                value: `${rev.validFrom} 至 ${rev.validTo}`,
                                numeric: true,
                            },
                            {
                                id: "rev",
                                label: "当前版本",
                                value: `v${rev.revisionNo}`,
                                numeric: true,
                            },
                            {
                                id: "owner",
                                label: "负责人",
                                value: contractOwnerLabel(
                                    contract.ownerLabel,
                                ),
                            },
                        ]}
                    />
                    <p className="mt-3 text-sm text-muted-foreground">
                        {rev.termsSummary}
                    </p>
                </CardContent>
            </Card>

            <Card size="sm" className={surfacePanelClassName}>
                <CardHeader className="border-b border-border/30">
                    <CardTitle>关联销售摘要</CardTitle>
                    <CardDescription>
                        金额仅为各销售单摘要，不汇总为合同金额。更新于{" "}
                        <span className="num">
                            {formatAsOf(contract.relatedSalesOrdersAsOf)}
                        </span>
                    </CardDescription>
                </CardHeader>
                <CardContent className="space-y-2">
                    <p className="text-sm">
                        关联 {contract.relatedSalesOrders.length} 张
                        {contract.relatedSalesOrders.length > 0
                            ? "（见下方关联销售单分区）"
                            : "。"}
                    </p>
                    <Button
                        type="button"
                        size="sm"
                        variant="secondary"
                        className="rounded-lg shadow-none"
                        render={
                            <Link href={`${baseHref}?section=sales-orders`} />
                        }
                    >
                        查看关联销售单
                    </Button>
                </CardContent>
            </Card>
        </div>
    )
}
