"use client"

import { DownloadIcon, FileTextIcon, WalletIcon } from "lucide-react"

import {
    DataFreshness,
    PageActions,
    PageHeader,
    surfaceInsetClassName,
} from "@/components/business"
import { cn } from "@/lib/utils"
import type { CustomerAccountsListView } from "@/features/customer-receivables/types"
import { freshnessText } from "@/lib/ui-text"

type CustomerReceivablesHeaderProps = {
    data: CustomerAccountsListView | undefined
    onExport: () => void
    onRegisterInvoice: () => void
    onRegisterReceipt: () => void
    canRegisterInvoice?: boolean
    canRegisterReceipt?: boolean
    canExport?: boolean
    permissionReason?: string
    invoiceBlockedReason?: string
    embedded?: boolean
    salesOrderNo?: string
}

/**
 * 客户往来页头。登记销项发票直接进入核销会话，不提供审批流程选择。
 */
export function CustomerReceivablesHeader({
    data,
    onExport,
    onRegisterInvoice,
    onRegisterReceipt,
    canRegisterInvoice = Boolean(data?.canRegister),
    canRegisterReceipt = Boolean(data?.canRegister),
    canExport = Boolean(data?.canExport),
    permissionReason,
    invoiceBlockedReason,
    embedded = false,
    salesOrderNo,
}: CustomerReceivablesHeaderProps) {
    const actions = (
        <PageActions
            actions={[
                {
                    actionKey: "export",
                    label: "导出",
                    icon: DownloadIcon,
                    variant: "outline",
                    mobileVisibility: "hide",
                    disabled: !canExport || !data || data.total === 0,
                    title: canExport ? undefined : permissionReason,
                    onClick: onExport,
                },
                {
                    actionKey: "register-invoice",
                    label: "登记销项发票",
                    icon: FileTextIcon,
                    variant: "outline",
                    mobileVisibility: embedded ? "show" : "hide",
                    disabled: !canRegisterInvoice,
                    title: canRegisterInvoice
                        ? undefined
                        : (invoiceBlockedReason ??
                          permissionReason ??
                          "当前无销项发票登记权限"),
                    onClick: onRegisterInvoice,
                },
                {
                    actionKey: "register-receipt",
                    label: "登记回款",
                    icon: WalletIcon,
                    mobileVisibility: embedded ? "show" : "hide",
                    disabled: !canRegisterReceipt,
                    title: canRegisterReceipt
                        ? undefined
                        : (permissionReason ?? "当前无回款登记权限"),
                    onClick: onRegisterReceipt,
                },
            ]}
        />
    )

    if (embedded) {
        return (
            <div
                className={cn(
                    surfaceInsetClassName,
                    "flex flex-wrap items-center justify-between gap-3 px-3 py-3",
                )}
            >
                <div className="flex min-w-0 flex-col gap-1">
                    <h3 className="text-sm font-medium">本单回款与开票</h3>
                    <p className="text-xs text-muted-foreground">
                        {salesOrderNo
                            ? `当前仅处理销售单 ${salesOrderNo} 的往来记录。`
                            : "当前仅处理本销售单的往来记录。"}
                    </p>
                    {data ? (
                        <DataFreshness
                            updatedAt={freshnessText.dataUpdatedAt}
                            dateTime={data.queriedAt}
                            state="fresh"
                            label="客户往来"
                        />
                    ) : null}
                </div>
                {actions}
            </div>
        )
    }

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
            actions={actions}
        />
    )
}
