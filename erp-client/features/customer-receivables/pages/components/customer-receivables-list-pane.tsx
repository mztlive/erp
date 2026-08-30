"use client"

import type { ColumnDef } from "@tanstack/react-table"

import { BusinessEmptyState, BusinessFailureState } from "@/components/business"
import type {
    CustomerAccountsListView,
    ReceiptRow,
    ReceivableAccountRow,
    SalesInvoiceRow,
} from "@/features/customer-receivables/types"
import { CustomerReceivablesMetrics } from "./customer-receivables-metrics"
import { CustomerReceivablesTable } from "./customer-receivables-table"
import {
    CustomerReceivablesToolbar,
    type ReceivableAppliedChip,
} from "./customer-receivables-toolbar"
import type { useCustomerReceivablesUrlState } from "../hooks/use-customer-receivables-url-state"

type Props = {
    data: CustomerAccountsListView | undefined
    urlState: ReturnType<typeof useCustomerReceivablesUrlState>
    appliedChips: readonly ReceivableAppliedChip[]
    isPending: boolean
    isError: boolean
    error: unknown
    onRetry: () => void
    receivableColumns: ColumnDef<ReceivableAccountRow>[]
    receiptColumns: ColumnDef<ReceiptRow>[]
    invoiceColumns: ColumnDef<SalesInvoiceRow>[]
}

/** 客户往来列表分区：权限/范围状态、指标、筛选和表格。 */
export function CustomerReceivablesListPane({
    data,
    urlState,
    appliedChips,
    isPending,
    isError,
    error,
    onRetry,
    receivableColumns,
    receiptColumns,
    invoiceColumns,
}: Props) {
    if (data && !data.moduleAllowed) {
        return (
            <BusinessFailureState
                kind="permission"
                description="无客户往来模块权限或权限已收回。"
            />
        )
    }
    if (data && !data.hasDataScope) {
        return (
            <BusinessEmptyState
                kind="no-scope"
                title="当前角色未配置客户往来范围"
                description="不得用 0 元假装无应收。请申请财务数据范围。"
            />
        )
    }

    const metrics = data?.metrics
    return (
        <>
            {!isError ? (
                <CustomerReceivablesMetrics
                    view={urlState.view}
                    due={urlState.due}
                    metrics={metrics}
                    queriedAt={data?.queriedAt}
                    patchUrl={urlState.patchUrl}
                />
            ) : null}

            <CustomerReceivablesTable
                view={urlState.view}
                data={data}
                isPending={isPending}
                isError={isError}
                error={error}
                onRetry={onRetry}
                metrics={metrics}
                pagination={urlState.pagination}
                receivableColumns={receivableColumns}
                receiptColumns={receiptColumns}
                invoiceColumns={invoiceColumns}
                toolbar={
                    <CustomerReceivablesToolbar
                        view={urlState.view}
                        searchDraft={urlState.searchDraft}
                        setSearchDraft={urlState.setSearchDraft}
                        searchInputRef={urlState.searchInputRef}
                        counterpartyPartyIdDraft={
                            urlState.counterpartyPartyIdDraft
                        }
                        setCounterpartyPartyIdDraft={
                            urlState.setCounterpartyPartyIdDraft
                        }
                        dueDraft={urlState.dueDraft}
                        setDueDraft={urlState.setDueDraft}
                        statusDraft={urlState.statusDraft}
                        setStatusDraft={urlState.setStatusDraft}
                        reviewStatusDraft={urlState.reviewStatusDraft}
                        setReviewStatusDraft={urlState.setReviewStatusDraft}
                        panelOpen={urlState.panelOpen}
                        setPanelOpen={urlState.setPanelOpen}
                        hasStructuredFilters={urlState.hasStructuredFilters}
                        hasActiveFilters={urlState.hasActiveFilters}
                        appliedChips={appliedChips}
                        removeFilter={urlState.removeFilter}
                        applyFilters={urlState.applyFilters}
                        resetMoreFilters={urlState.resetMoreFilters}
                        clearFilters={urlState.clearFilters}
                    />
                }
                patchUrl={urlState.patchUrl}
                onPaginationChange={urlState.handlePaginationChange}
                clearFilters={urlState.clearFilters}
            />
        </>
    )
}
