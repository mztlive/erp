"use client"

import {
    BusinessStatusBadge,
    DocumentSection,
    DocumentSummary,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { contractOwnerLabel } from "@/features/contracts/types"
import type { ContractCenterView } from "@/features/contracts/types"
import { formatAsOf } from "@/features/contracts/lib/format-as-of"

/** 概览分区：合同要点与关联销售摘要（不含合同级金额）。 */
export function ContractDetailOverview({
    contract,
    onOpenSalesOrders,
}: {
    contract: ContractCenterView
    onOpenSalesOrders?: () => void
}) {
    const rev = contract.currentRevision

    return (
        <div className="space-y-8">
            <DocumentSection
                title="合同要点"
                description="身份、客户、状态、版本与有效期；不含合同级金额。"
            >
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
                            value: contractOwnerLabel(contract.ownerLabel),
                        },
                        {
                            id: "payment",
                            label: "付款条件",
                            value:
                                rev.paymentTermSnapshot.label ||
                                rev.termsSummary ||
                                "—",
                            description:
                                rev.paymentTermSnapshot.description &&
                                rev.paymentTermSnapshot.description !==
                                    rev.paymentTermSnapshot.label
                                    ? rev.paymentTermSnapshot.description
                                    : undefined,
                        },
                    ]}
                />
            </DocumentSection>

            <DocumentSection
                title="关联销售摘要"
                description={
                    <>
                        金额仅为各销售单摘要，不汇总为合同金额。更新于{" "}
                        <span className="num">
                            {formatAsOf(contract.relatedSalesOrdersAsOf)}
                        </span>
                    </>
                }
                action={
                    onOpenSalesOrders ? (
                        <Button
                            id="card-contracts-detail-overview-open-sales-orders"
                            type="button"
                            size="sm"
                            variant="outline"
                            onClick={onOpenSalesOrders}
                        >
                            查看关联销售单
                        </Button>
                    ) : null
                }
            >
                <p className="text-sm leading-relaxed">
                    当前关联 {contract.relatedSalesOrders.length} 张销售单
                    {contract.relatedSalesOrders.length > 0
                        ? "，可在「关联销售单」分区查看明细。"
                        : "。"}
                </p>
            </DocumentSection>
        </div>
    )
}
