"use client"

import { FilePlus2Icon, RefreshCwIcon, WalletCardsIcon } from "lucide-react"

import { PageActions } from "@/components/business"
import { ListWorkspaceHeader } from "@/components/business/list-workspace"
import type { SupplierAccountsListView } from "@/features/supplier-payables/types"

export interface SupplierAccountsHeaderProps {
    data: SupplierAccountsListView | undefined
    isError: boolean
    isFetching: boolean
    onRefresh: () => void
    onRegisterInvoice: () => void
    onRegisterPayment: () => void
    canRegisterPayment: boolean
    paymentBlockedReason?: string
    onSettle: () => void
}

export function SupplierAccountsHeader({
    data,
    isError,
    isFetching,
    onRefresh,
    onRegisterInvoice,
    onRegisterPayment,
    canRegisterPayment,
    paymentBlockedReason,
    onSettle,
}: SupplierAccountsHeaderProps) {
    const queriedAt = data?.queriedAt

    return (
        <ListWorkspaceHeader
            eyebrow="财务"
            title="供应商往来"
            description={
                <>
                    查看应付、付款与进项发票。
                    <span className="ml-3 text-xs" role="status">
                        {isError ? (
                            "查询失败"
                        ) : isFetching ? (
                            "正在更新…"
                        ) : queriedAt ? (
                            <time dateTime={queriedAt}>
                                更新于 {queriedAt.slice(11, 16)}
                            </time>
                        ) : (
                            "正在查询"
                        )}
                    </span>
                </>
            }
        >
            <PageActions
                actions={[
                    {
                        actionKey: "refresh",
                        id: "supplier-payables-header-refresh",
                        label: "刷新",
                        icon: RefreshCwIcon,
                        variant: "ghost",
                        className:
                            "text-muted-foreground hover:text-foreground",
                        onClick: onRefresh,
                    },
                    {
                        actionKey: "register-invoice",
                        id: "supplier-payables-header-register-invoice",
                        label: "登记进项发票",
                        icon: FilePlus2Icon,
                        variant: "outline",
                        disabled: !data?.canRegisterInvoice,
                        title: data?.canRegisterInvoice
                            ? undefined
                            : "当前无进项发票登记权限",
                        onClick: onRegisterInvoice,
                    },
                    {
                        actionKey: "register-payment",
                        id: "supplier-payables-header-register-payment",
                        label: "登记付款",
                        icon: WalletCardsIcon,
                        disabled:
                            !data?.canRegisterPayment || !canRegisterPayment,
                        title:
                            data?.canRegisterPayment && canRegisterPayment
                                ? undefined
                                : (paymentBlockedReason ??
                                  "当前无付款登记权限"),
                        onClick: onRegisterPayment,
                    },
                    {
                        actionKey: "settle",
                        id: "supplier-payables-header-settle",
                        label: "去对账结算",
                        variant: "outline",
                        onClick: onSettle,
                    },
                ]}
            />
        </ListWorkspaceHeader>
    )
}
