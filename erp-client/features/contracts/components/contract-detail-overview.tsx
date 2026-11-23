"use client"

import * as React from "react"
import Link from "next/link"
import { FileTextIcon } from "lucide-react"

import {
    BusinessStatusBadge,
    DetailWorkspaceSection,
    MoneyValue,
    detailWorkspaceStyles,
    DocumentSummary,
    taxAmountToneClass,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { ContractFilePreviewDialog } from "@/features/contracts/components/contract-file-preview-dialog"
import { contractOwnerLabel } from "@/features/contracts/types"
import type { ContractCenterView } from "@/features/contracts/types"
import {
    contractRemainingDays,
    isExpiringWithin30Days,
} from "@/features/contracts/lib/contract-detail-helpers"
import { formatAsOf } from "@/features/contracts/lib/format-as-of"
import { toAutomationIdSegment } from "@/lib/automation-id"

/** 概览分区：合同要点与关联销售摘要（不含合同级金额）。 */
export function ContractDetailOverview({
    contract,
}: {
    contract: ContractCenterView
}) {
    const rev = contract.currentRevision
    const customerHref = `/sales/customers/${encodeURIComponent(contract.customer.id)}`

    return (
        <DetailWorkspaceSection width="form" title="合同要点">
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
                        value: (
                            <Link
                                id="card-contracts-detail-overview-customer-link"
                                href={customerHref}
                                className="font-medium text-primary underline-offset-4 hover:underline"
                            >
                                {contract.customer.displayName}
                            </Link>
                        ),
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

function validityRemainingCopy(contract: ContractCenterView): string {
    if (contract.status === "EXPIRED") return "合同已到期"
    if (contract.status === "TERMINATED") return "合同已终止"
    const days = contractRemainingDays(contract)
    if (days == null) return "长期有效"
    if (days < 0) return `已超期 ${Math.abs(days)} 天`
    if (days === 0) return "今日到期"
    return `剩余 ${days} 天`
}

export function ContractDetailRelatedSalesSummary({
    contract,
    onOpenSalesOrders,
    onOpenAttachments,
}: {
    contract: ContractCenterView
    onOpenSalesOrders: () => void
    onOpenAttachments: () => void
}) {
    const [previewFile, setPreviewFile] = React.useState<{
        id: string
        name: string
    } | null>(null)

    const rev = contract.currentRevision
    const expiring = isExpiringWithin30Days(contract)
    const remainingText = validityRemainingCopy(contract)
    const readyPdf =
        contract.attachments.find(
            (file) =>
                file.securityState === "done" &&
                file.revisionNo === rev.revisionNo,
        ) ?? contract.attachments.find((file) => file.securityState === "done")
    const recentOrders = contract.relatedSalesOrders.slice(0, 5)

    return (
        <div className="space-y-6">
            <section aria-label="合同效期与电子档" className="space-y-3">
                <div className="flex flex-wrap items-center justify-between gap-2">
                    <h2 className="text-section-title font-semibold tracking-tight text-foreground">
                        效期与电子档
                    </h2>
                    <BusinessStatusBadge
                        context="preview"
                        label={
                            expiring ? "30 日内将到期" : contract.statusLabel
                        }
                        tone={expiring ? "warning" : contract.statusTone}
                    />
                </div>
                <dl className="space-y-2.5 text-body-compact">
                    <div className="flex items-baseline justify-between gap-3">
                        <dt className="shrink-0 text-xs text-muted-foreground">
                            有效状态
                        </dt>
                        <dd className="num text-right font-medium text-foreground">
                            {remainingText}
                        </dd>
                    </div>
                    <div className="flex items-baseline justify-between gap-3">
                        <dt className="shrink-0 text-xs text-muted-foreground">
                            有效期止
                        </dt>
                        <dd className="num text-right text-foreground">
                            {rev.validTo === "9999-12-31"
                                ? "长期有效"
                                : rev.validTo}
                        </dd>
                    </div>
                    <div className="flex items-baseline justify-between gap-3">
                        <dt className="shrink-0 text-xs text-muted-foreground">
                            PDF 电子档
                        </dt>
                        <dd className="text-right">
                            {readyPdf ? (
                                <Button
                                    id="card-contracts-detail-overview-preview-pdf"
                                    type="button"
                                    variant="ghost"
                                    size="sm"
                                    className={
                                        detailWorkspaceStyles.inlineLinkButton
                                    }
                                    onClick={() =>
                                        setPreviewFile({
                                            id: readyPdf.id,
                                            name: readyPdf.name,
                                        })
                                    }
                                >
                                    <FileTextIcon
                                        data-icon="inline-start"
                                        aria-hidden="true"
                                    />
                                    预览当前 PDF
                                </Button>
                            ) : contract.attachments.length > 0 ? (
                                <Button
                                    id="card-contracts-detail-overview-open-attachments"
                                    type="button"
                                    variant="ghost"
                                    size="sm"
                                    className={
                                        detailWorkspaceStyles.inlineLinkButton
                                    }
                                    onClick={onOpenAttachments}
                                >
                                    查看附件状态
                                </Button>
                            ) : (
                                <span className="text-xs text-muted-foreground">
                                    暂无电子档
                                </span>
                            )}
                        </dd>
                    </div>
                </dl>
            </section>

            <section
                aria-label="关联销售摘要"
                className="space-y-3 border-t border-border/60 pt-5"
            >
                <div className="flex flex-wrap items-center justify-between gap-2">
                    <h2 className="text-section-title font-semibold tracking-tight text-foreground">
                        关联销售单
                    </h2>
                    <Button
                        id="card-contracts-detail-overview-open-sales-orders"
                        type="button"
                        size="sm"
                        variant="ghost"
                        className={detailWorkspaceStyles.subtleButton}
                        onClick={onOpenSalesOrders}
                    >
                        查看全部
                    </Button>
                </div>
                <p className="text-xs text-muted-foreground">
                    共{" "}
                    <span className="num font-medium text-foreground">
                        {contract.relatedSalesOrders.length}
                    </span>{" "}
                    张 · 更新于{" "}
                    <span className="num">
                        {formatAsOf(contract.relatedSalesOrdersAsOf)}
                    </span>
                </p>

                {recentOrders.length > 0 ? (
                    <ul className="divide-y divide-border/60">
                        {recentOrders.map((so) => (
                            <li
                                key={so.salesOrderId}
                                className="py-2.5 first:pt-0 last:pb-0"
                            >
                                <div className="flex items-center justify-between gap-2">
                                    <Link
                                        id={`card-contracts-detail-overview-so-${toAutomationIdSegment(so.salesOrderId)}`}
                                        href={`/sales/orders/${encodeURIComponent(so.salesOrderId)}`}
                                        className="num min-w-0 truncate font-medium text-primary underline-offset-4 hover:underline"
                                    >
                                        {so.documentNumber}
                                    </Link>
                                    <BusinessStatusBadge
                                        context="list"
                                        {...so.primaryStatus}
                                    />
                                </div>
                                <div className="mt-1 flex items-center justify-between gap-2 text-xs text-muted-foreground">
                                    <span>
                                        {so.natureLabel} ·{" "}
                                        <span className="num">
                                            v{so.contractRevisionNo}
                                        </span>
                                    </span>
                                    <MoneyValue
                                        value={so.amountGross}
                                        taxBasis="gross"
                                        className={taxAmountToneClass(
                                            "含税金额",
                                        )}
                                    />
                                </div>
                            </li>
                        ))}
                    </ul>
                ) : (
                    <p className="text-xs leading-relaxed text-muted-foreground">
                        暂无关联销售单
                    </p>
                )}
            </section>

            {previewFile ? (
                <ContractFilePreviewDialog
                    contractId={contract.contractId}
                    fileId={previewFile.id}
                    fileName={previewFile.name}
                    open
                    onOpenChange={(open) => {
                        if (!open) setPreviewFile(null)
                    }}
                />
            ) : null}
        </div>
    )
}
