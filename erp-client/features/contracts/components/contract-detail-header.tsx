"use client"

import * as React from "react"
import Link from "next/link"
import { useRouter } from "next/navigation"
import { HistoryIcon, PrinterIcon } from "lucide-react"

import { DocumentHeader, PageActions, PageHeader } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { contractOwnerLabel } from "@/features/contracts/types"
import type { ContractCenterView } from "@/features/contracts/types"
import {
    isExpiringWithin30Days,
    type ContractDetailSectionId,
} from "@/features/contracts/lib/contract-detail-helpers"

type ContractDetailHeaderProps = {
    contract: ContractCenterView
    activeSection: ContractDetailSectionId
    onPaperOpen: () => void
}

/**
 * 详情页头：对象头部、有效期提示与分区导航（导航写 URL，保持可回退）。
 */
export function ContractDetailHeader({
    contract,
    activeSection,
    onPaperOpen,
}: ContractDetailHeaderProps) {
    const router = useRouter()

    const baseHref = `/sales/contracts/${contract.contractId}`
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

    const navItems: {
        id: ContractDetailSectionId
        label: string
        href: string
    }[] = [
        { id: "overview", label: "概览", href: baseHref },
        {
            id: "settlement",
            label: "结算与开票",
            href: `${baseHref}?section=settlement`,
        },
        {
            id: "attachments",
            label: "附件",
            href: `${baseHref}?section=attachments`,
        },
        {
            id: "sales-orders",
            label: "关联销售单",
            href: `${baseHref}?section=sales-orders`,
        },
        {
            id: "versions",
            label: "版本与审计",
            href: `${baseHref}?section=versions`,
        },
    ]

    const rev = contract.currentRevision

    return (
        <>
            <PageHeader
                variant="object-chrome"
                actions={
                    <PageActions
                        actions={[
                            {
                                actionKey: "back",
                                label: "返回列表",
                                variant: "outline",
                                render: <Link href="/sales/contracts" />,
                            },
                        ]}
                    />
                }
            />

            <DocumentHeader
                density="compact"
                title={contract.contractNo}
                documentNumber={contract.contractNo}
                version={`v${rev.revisionNo}`}
                primaryStatus={{
                    label: contract.statusLabel,
                    tone: contract.statusTone,
                }}
                meta={
                    <span className="inline-flex flex-wrap items-center gap-x-1.5 gap-y-0.5">
                        <span>
                            客户{" "}
                            <span className="font-medium text-foreground">
                                {contract.customer.displayName}
                            </span>
                        </span>
                        <span aria-hidden="true">·</span>
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
                            type="button"
                            size="sm"
                            render={
                                <Link
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
                        type="button"
                        size="sm"
                        variant="outline"
                        disabled={!canPrint}
                        onClick={onPaperOpen}
                    >
                        <PrinterIcon
                            data-icon="inline-start"
                            aria-hidden="true"
                        />
                        纸质预览
                    </Button>
                }
            />

            <div className="flex flex-wrap items-center gap-2 text-sm text-muted-foreground">
                <span>
                    有效期{" "}
                    <span className="num text-foreground">
                        {rev.validFrom} 至 {rev.validTo}
                    </span>
                </span>
                <span>·</span>
                <span>{contractOwnerLabel(contract.ownerLabel)}</span>
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

            <nav
                aria-label="对象分区"
                className="flex flex-wrap gap-2 border-b border-grid pb-2"
            >
                {navItems.map((item) => {
                    const active = activeSection === item.id
                    return (
                        <Button
                            key={item.id}
                            type="button"
                            size="sm"
                            variant={active ? "secondary" : "ghost"}
                            aria-current={active ? "page" : undefined}
                            onClick={(event) => {
                                event.preventDefault()
                                router.replace(item.href, { scroll: false })
                            }}
                        >
                            {item.id === "versions" ? (
                                <HistoryIcon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                />
                            ) : null}
                            {item.label}
                        </Button>
                    )
                })}
            </nav>
        </>
    )
}
