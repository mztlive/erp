"use client"

import { DownloadIcon, FileTextIcon, WalletIcon } from "lucide-react"

import { DataFreshness, PageActions, PageHeader } from "@/components/business"
import type { CustomerAccountsListView } from "@/features/customer-receivables/types"
import { freshnessText } from "@/lib/ui-text"

type CustomerReceivablesHeaderProps = {
    data: CustomerAccountsListView | undefined
    onExport: () => void
    onRegisterInvoice: () => void
    onRegisterReceipt: () => void
}

/**
 * 客户往来页头。登记销项发票直接进入核销会话，不提供审批流程选择。
 */
export function CustomerReceivablesHeader({
    data,
    onExport,
    onRegisterInvoice,
    onRegisterReceipt,
}: CustomerReceivablesHeaderProps) {
    return (
        <PageHeader
            title="客户往来"
            metadata={
                data ? (
                    <DataFreshness
                        updatedAt={freshnessText.dataUpdatedAt}
                        dateTime={data.queriedAt}
                        state="fresh"
                        label="客户往来"
                    />
                ) : null
            }
            actions={
                <PageActions
                    actions={[
                        {
                            actionKey: "export",
                            label: "导出",
                            icon: DownloadIcon,
                            variant: "outline",
                            mobileVisibility: "hide",
                            disabled: !data?.canExport || data.total === 0,
                            onClick: onExport,
                        },
                        {
                            actionKey: "register-invoice",
                            label: "登记销项发票",
                            icon: FileTextIcon,
                            variant: "outline",
                            mobileVisibility: "hide",
                            disabled: !data?.canRegister,
                            title: data?.canRegister
                                ? undefined
                                : "当前无销项发票登记权限",
                            onClick: onRegisterInvoice,
                        },
                        {
                            actionKey: "register-receipt",
                            label: "登记回款",
                            icon: WalletIcon,
                            mobileVisibility: "hide",
                            disabled: !data?.canRegister,
                            title: data?.canRegister
                                ? undefined
                                : "当前无回款登记权限",
                            onClick: onRegisterReceipt,
                        },
                    ]}
                />
            }
        />
    )
}
