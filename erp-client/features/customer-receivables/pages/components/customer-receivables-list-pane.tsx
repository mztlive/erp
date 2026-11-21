"use client"

import type { CustomerAccountPreviewTarget } from "@/features/customer-receivables/components/column-types"

import type { ColumnDef } from "@tanstack/react-table"

import { BusinessFailureState } from "@/components/business"
import { listWorkspaceEmptyStateClassName } from "@/components/business/list-workspace"
import { Skeleton } from "@/components/ui/skeleton"
import { Button } from "@/components/ui/button"
import type { CustomerAccountsLedgerListView } from "@/features/customer-receivables/api/list-view"
import type {
    ReceiptRow,
    ReceivableAccountRow,
    SalesInvoiceRow,
} from "@/features/customer-receivables/types"
import { CustomerReceivablesTable } from "./customer-receivables-table"
import {
    CustomerReceivablesToolbar,
    type ReceivableAppliedChip,
} from "./customer-receivables-toolbar"
import type { useCustomerReceivablesUrlState } from "../hooks/use-customer-receivables-url-state"

type Props = {
    data: CustomerAccountsLedgerListView | undefined
    urlState: ReturnType<typeof useCustomerReceivablesUrlState>
    appliedChips: readonly ReceivableAppliedChip[]
    profilePending: boolean
    isPending: boolean
    isFetching: boolean
    isPlaceholder: boolean
    canReadView: boolean
    profileError: unknown
    onRetryProfile: () => void
    embedded: boolean
    canPreview: (kind: "receivable" | "receipt" | "invoice") => boolean
    isError: boolean
    error: unknown
    onRetry: () => void
    receivableColumns: ColumnDef<ReceivableAccountRow>[]
    receiptColumns: ColumnDef<ReceiptRow>[]
    invoiceColumns: ColumnDef<SalesInvoiceRow>[]
    preview: CustomerAccountPreviewTarget | null
    onPreview: (target: CustomerAccountPreviewTarget) => void
}

/** 客户往来列表分区：权限、范围状态、筛选和表格。 */
export function CustomerReceivablesListPane({
    data,
    urlState,
    appliedChips,
    profilePending,
    isPending,
    isFetching,
    isPlaceholder,
    canReadView,
    profileError,
    onRetryProfile,
    embedded,
    canPreview,
    isError,
    error,
    onRetry,
    receivableColumns,
    receiptColumns,
    invoiceColumns,
    preview,
    onPreview,
}: Props) {
    if (profileError) {
        return (
            <BusinessFailureState
                idPrefix="customer-receivables-profile-failure"
                title="暂时无法核对客户往来权限"
                error={profileError}
                action={
                    <Button
                        id="customer-receivables-list-retry"
                        type="button"
                        variant="outline"
                        size="sm"
                        onClick={onRetryProfile}
                    >
                        重试
                    </Button>
                }
            />
        )
    }
    if (profilePending || (isPending && !isError)) {
        return (
            <div
                role="status"
                aria-label="正在加载客户往来"
                aria-busy="true"
                className="flex flex-col gap-3"
            >
                <Skeleton className="h-10 rounded-lg" />
                <Skeleton className="h-96 rounded-none" />
            </div>
        )
    }
    if (!canReadView || (data && !data.moduleAllowed)) {
        return (
            <BusinessFailureState
                kind="permission"
                className={listWorkspaceEmptyStateClassName}
                description="无当前客户往来视图权限或权限已收回。"
            />
        )
    }

    const noScope = Boolean(data && !data.hasDataScope)
    return (
        <CustomerReceivablesTable
            view={urlState.view}
            data={data}
            loading={isFetching}
            isError={isError}
            error={error}
            onRetry={onRetry}
            noScope={noScope}
            hasActiveFilters={urlState.hasActiveFilters}
            onClearFilters={urlState.clearFilters}
            canPreview={canPreview}
            pagination={urlState.pagination}
            receivableColumns={receivableColumns}
            receiptColumns={receiptColumns}
            invoiceColumns={invoiceColumns}
            preview={preview}
            onPreview={onPreview}
            toolbar={
                !noScope || isError ? (
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
                        statusDraft={urlState.statusDraft}
                        setStatusDraft={urlState.setStatusDraft}
                        panelOpen={urlState.panelOpen}
                        setPanelOpen={urlState.setPanelOpen}
                        appliedChips={appliedChips}
                        removeFilter={urlState.removeFilter}
                        applyFilters={urlState.applyFilters}
                        resetMoreFilters={urlState.resetMoreFilters}
                        cancelMoreFilters={urlState.cancelMoreFilters}
                        clearFilters={urlState.clearFilters}
                        hasPendingChanges={urlState.hasPendingChanges}
                        loading={isFetching}
                        failed={isError}
                    />
                ) : null
            }
            embedded={embedded}
            onPaginationChange={(next) => {
                if (!isPlaceholder) urlState.handlePaginationChange(next)
            }}
        />
    )
}
