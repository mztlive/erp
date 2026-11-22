"use client"

import type { ReactNode } from "react"
import Link from "next/link"
import { PrinterIcon } from "lucide-react"

import {
    DetailWorkspaceChrome,
    DetailWorkspaceMeta,
    GuardedBusinessAction,
    detailWorkspaceStyles,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { PermissionGuard } from "@/features/auth/components/permission-guard"
import { contractOwnerLabel } from "@/features/contracts/types"
import type { ContractCenterView } from "@/features/contracts/types"

type ContractDetailHeaderProps = {
    contract: ContractCenterView
    tabs: ReactNode
    onPaperOpen: () => void
    onImportRevision?: () => void
    onManageSalesAccess?: () => void
}

/**
 * 合同详情页头：返回列表、对象身份与业务操作。
 */
export function ContractDetailHeader({
    contract,
    tabs,
    onPaperOpen,
    onImportRevision,
    onManageSalesAccess,
}: ContractDetailHeaderProps) {
    const canCreateSo = contract.allowedActions.includes("CREATE_SALES_ORDER")
    const canPrint = contract.allowedActions.includes("PRINT")
    const soBlocker = contract.actionBlockers.find(
        (b) => b.action === "CREATE_SALES_ORDER",
    )

    const rev = contract.currentRevision
    const customerHref = `/sales/customers/${encodeURIComponent(contract.customer.id)}`

    return (
        <DetailWorkspaceChrome
            back={{
                id: "card-contracts-detail-header-back",
                label: "合同列表",
                href: "/sales/contracts",
            }}
            numberLabel="合同编号"
            title={`${contract.customer.displayName} 销售合同`}
            documentNumber={contract.contractNo}
            version={`v${rev.revisionNo}`}
            primaryStatus={{
                label: contract.statusLabel,
                tone: contract.statusTone,
            }}
            tabs={tabs}
            meta={
                <DetailWorkspaceMeta
                    items={[
                        <span key="customer">
                            客户{" "}
                            <Link
                                id="card-contracts-detail-header-customer-link"
                                href={customerHref}
                                className="font-medium text-foreground underline-offset-4 hover:text-primary hover:underline"
                            >
                                {contract.customer.displayName}
                            </Link>
                        </span>,
                        <span key="owner">
                            负责人{" "}
                            <span className="font-medium text-foreground">
                                {contractOwnerLabel(contract.ownerLabel)}
                            </span>
                        </span>,
                        <span key="validity">
                            有效期{" "}
                            <span className="num text-foreground">
                                {rev.validFrom} 至 {rev.validTo}
                            </span>
                        </span>,
                    ]}
                />
            }
            primaryAction={
                <PermissionGuard permission="sales_order:create">
                    {canCreateSo ? (
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
                        <GuardedBusinessAction
                            id="card-contracts-detail-header-create-sales-order-disabled"
                            type="button"
                            size="sm"
                            variant="outline"
                            disabled
                            reason={
                                soBlocker?.message ??
                                "当前合同状态不允许新建销售单"
                            }
                        >
                            新建销售单
                        </GuardedBusinessAction>
                    )}
                </PermissionGuard>
            }
            secondaryActions={
                <>
                    {onManageSalesAccess ? (
                        <Button
                            id="contracts-detail-manage-sales-access"
                            type="button"
                            size="sm"
                            variant="ghost"
                            className={detailWorkspaceStyles.quietButton}
                            onClick={onManageSalesAccess}
                        >
                            共享销售
                        </Button>
                    ) : null}
                    {onImportRevision ? (
                        <Button
                            id="contract-detail-import-revision"
                            type="button"
                            size="sm"
                            variant="ghost"
                            className={detailWorkspaceStyles.quietButton}
                            onClick={onImportRevision}
                        >
                            导入新版本
                        </Button>
                    ) : null}
                    <Button
                        id="card-contracts-detail-header-paper-preview"
                        type="button"
                        size="sm"
                        variant="ghost"
                        className={detailWorkspaceStyles.quietButton}
                        disabled={!canPrint}
                        onClick={onPaperOpen}
                    >
                        <PrinterIcon
                            data-icon="inline-start"
                            aria-hidden="true"
                        />
                        纸质预览
                    </Button>
                </>
            }
        />
    )
}
