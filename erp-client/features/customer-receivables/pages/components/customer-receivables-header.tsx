"use client"

import {
    BarChart3Icon,
    DownloadIcon,
    FileTextIcon,
    WalletIcon,
} from "lucide-react"

import {
    DataFreshness,
    PageActions,
    surfaceInsetClassName,
} from "@/components/business"
import {
    ListWorkspaceHeader,
    listWorkspaceQuietAction,
    listWorkspaceStyles,
} from "@/components/business/list-workspace"
import type { CustomerAccountsListView } from "@/features/customer-receivables/types"
import { freshnessText } from "@/lib/ui-text"
import { cn } from "@/lib/utils"

type CustomerReceivablesHeaderProps = {
    data: CustomerAccountsListView | undefined
    onExport: () => void
    onRegisterInvoice: () => void
    onRegisterReceipt: () => void
    canRegisterInvoice: boolean
    canRegisterReceipt: boolean
    startSessionPending?: boolean
    canExport: boolean
    exportPending: boolean
    onOpenScope?: () => void
    views?: React.ReactNode
    embedded?: boolean
    salesOrderNo?: string
}

export function CustomerReceivablesHeader({
    data,
    onExport,
    onRegisterInvoice,
    onRegisterReceipt,
    canRegisterInvoice,
    canRegisterReceipt,
    startSessionPending = false,
    canExport,
    exportPending,
    onOpenScope,
    views,
    embedded = false,
    salesOrderNo,
}: CustomerReceivablesHeaderProps) {
    const actions = (
        <PageActions
            idPrefix={
                embedded
                    ? "customer-receivables-header-embedded"
                    : "customer-receivables-header"
            }
            size="sm"
            className="gap-1.5"
            actions={[
                ...(onOpenScope
                    ? [
                          listWorkspaceQuietAction({
                              actionKey: "scope",
                              id: "customer-receivables-open-scope",
                              label: "按范围汇总",
                              icon: BarChart3Icon,
                              onClick: onOpenScope,
                          }),
                      ]
                    : []),
                listWorkspaceQuietAction({
                    hidden: !canExport,
                    actionKey: "export",
                    id: embedded
                        ? "customer-receivables-header-embedded-export"
                        : "customer-receivables-header-export",
                    label: "导出",
                    icon: DownloadIcon,
                    disabled: !data || data.total === 0,
                    title: exportPending
                        ? "查询完成后可导出"
                        : data?.total
                          ? "导出与当前查询结果一致"
                          : "当前查询没有可导出的记录",
                    onClick: onExport,
                }),
                {
                    hidden: !canRegisterInvoice,
                    actionKey: "register-invoice",
                    id: embedded
                        ? "customer-receivables-header-embedded-register-invoice"
                        : "customer-receivables-header-register-invoice",
                    label: "新建开票申请",
                    icon: FileTextIcon,
                    disabled: startSessionPending,
                    variant: canRegisterReceipt ? "ghost" : "default",
                    className: canRegisterReceipt
                        ? listWorkspaceStyles.quietButton
                        : undefined,
                    onClick: onRegisterInvoice,
                },
                {
                    hidden: !canRegisterReceipt,
                    actionKey: "register-receipt",
                    id: embedded
                        ? "customer-receivables-header-embedded-register-receipt"
                        : "customer-receivables-header-register-receipt",
                    label: startSessionPending ? "创建中…" : "登记回款",
                    icon: WalletIcon,
                    loading: startSessionPending,
                    disabled: startSessionPending,
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
                    <h3 className="font-medium">本单回款与开票</h3>
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
        <ListWorkspaceHeader eyebrow="" title="客户往来" views={views}>
            {actions}
        </ListWorkspaceHeader>
    )
}
