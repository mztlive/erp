"use client"

import {
    BusinessStatusBadge,
    DetailWorkspaceSection,
    detailWorkspaceStyles,
    DocumentSummary,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { contractOwnerLabel } from "@/features/contracts/types"
import type { ContractCenterView } from "@/features/contracts/types"
import { formatAsOf } from "@/features/contracts/lib/format-as-of"

/** 概览分区：合同要点与关联销售摘要（不含合同级金额）。 */
export function ContractDetailOverview({
    contract,
}: {
    contract: ContractCenterView
}) {
    const rev = contract.currentRevision

    return (
        <DetailWorkspaceSection
            width="form"
            title="合同要点"
            subtitle="合同基本签约信息与有效期限（实际交易金额以具体销售单为准）。"
        >
            <DocumentSummary
                columns="two"
                className="border-0 py-0"
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
                        id: "signing-company",
                        label: "我方签约主体",
                        value: rev.signingCompanyName ?? "历史档案未记录",
                    },
                    {
                        id: "recognition",
                        label: "归档依据",
                        value: rev.recognizedFields
                            ? "智能识别并核对归档"
                            : "历史归档",
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
        </DetailWorkspaceSection>
    )
}

export function ContractDetailRelatedSalesSummary({
    contract,
    onOpenSalesOrders,
}: {
    contract: ContractCenterView
    onOpenSalesOrders: () => void
}) {
    return (
        <DetailWorkspaceSection
            className="first:pt-0 last:pb-0"
            title="关联销售摘要"
            subtitle={
                <>
                    以下为关联销售单汇总，实际交易金额以各销售单为准。更新于{" "}
                    <span className="num">
                        {formatAsOf(contract.relatedSalesOrdersAsOf)}
                    </span>
                </>
            }
            actions={
                <Button
                    id="card-contracts-detail-overview-open-sales-orders"
                    type="button"
                    size="sm"
                    variant="ghost"
                    className={detailWorkspaceStyles.subtleButton}
                    onClick={onOpenSalesOrders}
                >
                    查看关联销售单
                </Button>
            }
        >
            <p className="text-body-compact leading-relaxed">
                当前关联 {contract.relatedSalesOrders.length} 张销售单
                {contract.relatedSalesOrders.length > 0
                    ? "，可在「关联销售单」标签页查看明细。"
                    : "。"}
            </p>
        </DetailWorkspaceSection>
    )
}
