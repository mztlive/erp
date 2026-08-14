"use client"

import { FilePlus2Icon, RefreshCwIcon, WalletCardsIcon } from "lucide-react"

import { DataFreshness, PageActions, PageHeader } from "@/components/business"
import type { SupplierAccountsListView } from "@/features/supplier-payables/types"

export interface SupplierAccountsHeaderProps {
    data: SupplierAccountsListView
    onRefresh: () => void
    onRegisterInvoice: () => void
    onRegisterPayment: () => void
    onSettle: () => void
}

export function SupplierAccountsHeader({
    data,
    onRefresh,
    onRegisterInvoice,
    onRegisterPayment,
    onSettle,
}: SupplierAccountsHeaderProps) {
    return (
        <PageHeader
            title="供应商往来"
            breadcrumbs={[
                {
                    id: "fin",
                    label: "财务",
                    href: "/finance/supplier-accounts",
                },
                { id: "ap", label: "供应商往来", current: true },
            ]}
            metadata={
                <div className="flex flex-wrap items-center gap-x-4 gap-y-1">
                    <DataFreshness
                        updatedAt={new Date(data.queriedAt).toLocaleString(
                            "zh-CN",
                        )}
                        dateTime={data.queriedAt}
                        label="数据更新于"
                    />
                    <p className="text-xs text-muted-foreground">
                        {data.payablePriorityPolicy.state === "AVAILABLE"
                            ? "混合来源按系统优先级分配"
                            : data.payablePriorityPolicy.state === "MISSING"
                              ? "混合来源分配规则未配置"
                              : "混合来源分配规则已更新"}
                    </p>
                </div>
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
                            disabled: !data.canRegisterInvoice,
                            title: data.canRegisterInvoice
                                ? undefined
                                : "当前无进项发票登记权限",
                            onClick: onRegisterInvoice,
                        },
                        {
                            actionKey: "register-payment",
                            label: "登记付款",
                            icon: WalletCardsIcon,
                            mobileVisibility: "hide",
                            disabled: !data.canRegisterPayment,
                            title: data.canRegisterPayment
                                ? undefined
                                : "当前无付款登记权限",
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
