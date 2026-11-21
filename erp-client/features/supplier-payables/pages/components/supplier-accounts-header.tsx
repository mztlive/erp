"use client"

import { FilePlus2Icon, RefreshCwIcon } from "lucide-react"

import { PageActions } from "@/components/business"
import {
    ListWorkspaceHeader,
    ListWorkspaceViews,
    listWorkspaceStyles,
} from "@/components/business/list-workspace"
import {
    SUPPLIER_ACCOUNT_VIEWS,
    VIEW_LABEL,
    type SupplierAccountsView,
} from "@/features/supplier-payables/types"
import { toAutomationIdSegment } from "@/lib/automation-id"

interface SupplierAccountsHeaderProps {
    view: SupplierAccountsView
    showViews: boolean
    canRegisterInvoice: boolean
    prioritizeQuery: boolean
    queriedAt?: string
    onViewChange: (view: SupplierAccountsView) => void
    onRefresh: () => void
    onRegisterInvoice: () => void
}

export function SupplierAccountsHeader({
    view,
    showViews,
    canRegisterInvoice,
    prioritizeQuery,
    queriedAt,
    onViewChange,
    onRefresh,
    onRegisterInvoice,
}: SupplierAccountsHeaderProps) {
    return (
        <ListWorkspaceHeader
            title="供应商往来"
            views={
                showViews ? (
                    <div id="supplier-payables-view-tabs">
                        <ListWorkspaceViews
                            variant="inline"
                            ariaLabel="供应商往来工作视图"
                            items={SUPPLIER_ACCOUNT_VIEWS.map((item) => ({
                                id: `supplier-payables-view-tabs-trigger-${toAutomationIdSegment(item)}`,
                                label: VIEW_LABEL[item],
                                active: item === view,
                                onClick: () => onViewChange(item),
                            }))}
                        />
                    </div>
                ) : undefined
            }
        >
            {queriedAt ? (
                <time
                    dateTime={queriedAt}
                    className="text-xs text-muted-foreground"
                >
                    更新于 {queriedAt.slice(11, 16)}
                </time>
            ) : null}
            <PageActions
                size="sm"
                actions={[
                    {
                        actionKey: "refresh",
                        id: "supplier-payables-header-refresh",
                        label: "刷新",
                        icon: RefreshCwIcon,
                        variant: "ghost",
                        className: listWorkspaceStyles.quietButton,
                        onClick: onRefresh,
                    },
                    {
                        hidden: !canRegisterInvoice,
                        actionKey: "register-invoice",
                        id: "supplier-payables-header-register-invoice",
                        label: "登记进项发票",
                        icon: FilePlus2Icon,
                        variant: prioritizeQuery ? "ghost" : "default",
                        className: prioritizeQuery
                            ? listWorkspaceStyles.quietButton
                            : undefined,
                        onClick: onRegisterInvoice,
                    },
                ]}
            />
        </ListWorkspaceHeader>
    )
}
