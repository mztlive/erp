"use client"

import Link from "next/link"
import { PrinterIcon } from "lucide-react"

import { DocumentHeader } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { contractOwnerLabel } from "@/features/contracts/types"
import type { ContractCenterView } from "@/features/contracts/types"
import { isExpiringWithin30Days } from "@/features/contracts/lib/contract-detail-helpers"

type ContractDetailHeaderProps = {
    contract: ContractCenterView
    onPaperOpen: () => void
}

/**
 * 合同身份卡：独立浮起，与下方 Tabs 主工作面分离，避免套卡过密。
 */
export function ContractDetailHeader({
    contract,
    onPaperOpen,
}: ContractDetailHeaderProps) {
    const canCreateSo = contract.allowedActions.includes("CREATE_SALES_ORDER")
    const canPrint = contract.allowedActions.includes("PRINT")
    const soBlocker = contract.actionBlockers.find(
        (b) => b.action === "CREATE_SALES_ORDER",
    )
    const expiring = isExpiringWithin30Days(contract)
    const archived = contract.attachments.length > 0
    const allAttachmentsSafe =
        archived &&
        contract.attachments.every((file) => file.securityState === "done")

    const rev = contract.currentRevision

    return (
        <DocumentHeader
            title={contract.contractNo}
            documentNumber={contract.contractNo}
            version={`v${rev.revisionNo}`}
            primaryStatus={{
                label: contract.statusLabel,
                tone: contract.statusTone,
            }}
            meta={
                <span className="inline-flex flex-wrap items-center gap-x-2 gap-y-1">
                    <span>
                        客户{" "}
                        <span className="font-medium text-foreground">
                            {contract.customer.displayName}
                        </span>
                    </span>
                    <span aria-hidden="true" className="text-border">
                        ·
                    </span>
                    <span>
                        负责人{" "}
                        <span className="font-medium text-foreground">
                            {contractOwnerLabel(contract.ownerLabel)}
                        </span>
                    </span>
                </span>
            }
            primaryAction={
                canCreateSo ? (
                    <Button
                        id="card-contracts-detail-header-create-sales-order"
                        type="button"
                        size="sm"
                        render={
                            <Link
                                id="card-contracts-detail-header-create-sales-order-link"
                                href={`/sales/orders?mode=create&customerId=${encodeURIComponent(
                                    contract.customer.id,
                                )}&contractId=${encodeURIComponent(contract.contractId)}`}
                            />
                        }
                    >
                        新建销售单
                    </Button>
                ) : (
                    <Button
                        id="card-contracts-detail-header-create-sales-order-disabled"
                        type="button"
                        size="sm"
                        variant="outline"
                        disabled
                        title={soBlocker?.message}
                    >
                        新建销售单
                    </Button>
                )
            }
            secondaryActions={
                <Button
                    id="card-contracts-detail-header-paper-preview"
                    type="button"
                    size="sm"
                    variant="outline"
                    disabled={!canPrint}
                    onClick={onPaperOpen}
                >
                    <PrinterIcon data-icon="inline-start" aria-hidden="true" />
                    纸质预览
                </Button>
            }
        >
            <div className="flex flex-wrap items-center gap-2 text-sm text-muted-foreground">
                <span>
                    有效期{" "}
                    <span className="num text-foreground">
                        {rev.validFrom} 至 {rev.validTo}
                    </span>
                </span>
                {expiring ? (
                    <Badge variant="warning">30 日内将到期</Badge>
                ) : null}
                {allAttachmentsSafe ? (
                    <Badge variant="info">PDF 电子档已归档</Badge>
                ) : null}
            </div>
            {!canCreateSo && soBlocker ? (
                <p className="text-xs text-muted-foreground">
                    新建销售单不可用：{soBlocker.message}
                </p>
            ) : null}
        </DocumentHeader>
    )
}
