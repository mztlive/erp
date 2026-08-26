"use client"

import { FilePlus2Icon, RefreshCwIcon, WalletCardsIcon } from "lucide-react"

import { DataFreshness, PageActions, PageHeader } from "@/components/business"
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
        <PageHeader
            title="供应商往来"
            metadata={
                <DataFreshness
                    updatedAt={
                        isError
                            ? "查询失败"
                            : queriedAt
                              ? queriedAt.slice(11, 16)
                              : "正在查询"
                    }
                    dateTime={queriedAt}
                    state={
                        isError
                            ? "failed"
                            : isFetching
                              ? "syncing"
                              : queriedAt
                                ? "fresh"
                                : "unknown"
                    }
                />
            }
            actions={
                <PageActions
                    actions={[
                        {
                            actionKey: "refresh",
                            label: "刷新",
                            icon: RefreshCwIcon,
                            variant: "ghost",
                            className:
                                "text-muted-foreground hover:text-foreground",
                            onClick: onRefresh,
                        },
                        {
                            actionKey: "register-invoice",
                            label: "登记进项发票",
                            icon: FilePlus2Icon,
                            variant: "outline",
                            mobileVisibility: "hide",
                            disabled: !data?.canRegisterInvoice,
                            title: data?.canRegisterInvoice
                                ? undefined
                                : "当前无进项发票登记权限",
                            onClick: onRegisterInvoice,
                        },
                        {
                            actionKey: "register-payment",
                            label: "登记付款",
                            icon: WalletCardsIcon,
                            mobileVisibility: "hide",
                            disabled:
                                !data?.canRegisterPayment ||
                                !canRegisterPayment,
                            title:
                                data?.canRegisterPayment && canRegisterPayment
                                    ? undefined
                                    : (paymentBlockedReason ??
                                      "当前无付款登记权限"),
                            onClick: onRegisterPayment,
                        },
                        {
                            actionKey: "settle",
                            label: "去对账结算",
                            variant: "outline",
                            mobileVisibility: "hide",
                            onClick: onSettle,
                        },
                    ]}
                />
            }
        />
    )
}
