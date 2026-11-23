"use client"

import * as React from "react"
import Link from "next/link"
import { ArrowLeftIcon, DownloadIcon } from "lucide-react"

import {
    BusinessFailureState,
    FundsScopeBanner,
    FundsScopeSummaryBar,
    MoneyValue,
    QuickPreviewSheet,
} from "@/components/business"
import {
    PreviewAmount,
    PreviewNote,
} from "@/components/business/financial-preview"
import {
    ListDataTable,
    ListFilterSection,
    ListFilterSections,
    ListSearchField,
    ListWorkSurface,
    ListWorkspaceFilterBar,
    ListWorkspacePage,
    listWorkspaceStyles,
    type ListDataTableProps,
} from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import { Skeleton } from "@/components/ui/skeleton"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { personDirectoryLabel } from "@/features/entity-selectors/api/person-directory"
import { PersonDirectoryFilter } from "@/features/entity-selectors/components/person-directory-filter"
import { usePersonDirectorySelected } from "@/features/entity-selectors/hooks/person-directory"
import { OrganizationUnitFilter } from "@/features/organization/components/organization-unit-filter"
import { selectedOrgUnitIds } from "@/features/organization/lib/organization-ids"
import {
    allocationNumber,
    allocationPayableNumber,
    createSupplierScopedAllocationColumns,
    createSupplierScopedPayableColumns,
    createSupplierScopedPaymentColumns,
    payableSourceNumber,
} from "@/features/supplier-payables/components/scoped-columns"
import {
    useSupplierScopeDetailQuery,
    useSupplierScopeListQuery,
} from "@/features/supplier-payables/hooks/scoped-queries"
import { exportSupplierScopeCsv } from "@/features/supplier-payables/lib/scoped-export"
import { useSupplierScopeUrlState } from "@/features/supplier-payables/pages/hooks/use-supplier-scope-url-state"
import { VIEW_LABEL } from "@/features/supplier-payables/types"
import { getErrorMessage } from "@/lib/api/errors"
import { hasPermission } from "@/lib/permissions"
import { useScopeListExport } from "@/lib/use-scope-list-export"
import { useSearchHotkey } from "@/lib/use-search-hotkey"
import { listFilterText, scopeText } from "@/lib/ui-text"

const SCOPE_VIEWS = ["payable", "payment", "purchase_invoice"] as const
const LIST_PERMISSIONS = {
    payable: "payable_account:list",
    payment: "supplier_payment:list",
    purchase_invoice: "purchase_invoice_allocation:list",
} as const
const DETAIL_PERMISSIONS = {
    payable: "payable_account:detail",
    payment: "supplier_payment:detail",
} as const
const TABLE_IDS = {
    payable: "supplier-payables-scope-list-payable",
    payment: "supplier-payables-scope-list-payment",
    purchase_invoice: "supplier-payables-scope-list-allocation",
} as const
const TRAY_FILTER_KEYS = ["operatorUserIds", "orgUnitIds"] as const
const APPLIED_CHIP_KEYS = [
    ...TRAY_FILTER_KEYS,
    "supplierId",
    "purchaseOrderId",
    "payableAccountId",
] as const
const toolbarPrefix = "supplier-payables-scope-toolbar"

type PreviewRecord = {
    kind: keyof typeof DETAIL_PERMISSIONS
    id: string
    number: string
    supplierName?: string
}

type ShareWithOptionalName = {
    owner_user_id: string
    owner_user_name?: string | null
    owner_name?: string | null
}

function isInternalIdentifier(value: string): boolean {
    const trimmed = value.trim()
    return (
        /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(
            trimmed,
        ) || /^[a-z]{2,6}_[a-z0-9_-]{6,}$/i.test(trimmed)
    )
}

function resolveGroupedOwnerName(
    share: ShareWithOptionalName,
    resolvedNames: ReadonlyMap<string, string>,
): string {
    const inline =
        share.owner_user_name?.trim() || share.owner_name?.trim() || ""
    if (inline) return inline
    const fromDirectory = resolvedNames.get(share.owner_user_id)
    if (fromDirectory) return fromDirectory
    const rawId = share.owner_user_id.trim()
    if (!rawId || isInternalIdentifier(rawId)) return "负责人（当前不可见）"
    return rawId
}

/** 供应商往来范围页：统一 ListWorkspacePage、紧凑口径条、无边框金额指标带与 768px 详情抽屉。 */
export function SupplierScopePage() {
    const urlState = useSupplierScopeUrlState()
    useSearchHotkey(urlState.searchInputRef)
    const account = useAccountProfileQuery()
    const permissions = account.isSuccess ? account.data.permissions : undefined
    const allowedViews = SCOPE_VIEWS.filter((view) =>
        hasPermission(permissions, LIST_PERMISSIONS[view]),
    )
    const canReadList = allowedViews.includes(urlState.view)
    const canPreviewPayable = hasPermission(
        permissions,
        DETAIL_PERMISSIONS.payable,
    )
    const canPreviewPayment = hasPermission(
        permissions,
        DETAIL_PERMISSIONS.payment,
    )
    const directoryOrgUnitIds = selectedOrgUnitIds(urlState.orgDraft)
    const listQuery = useSupplierScopeListQuery(urlState.query, canReadList)
    const [preview, setPreview] = React.useState<PreviewRecord | null>(null)
    const lastFocusedRow = React.useRef<PreviewRecord | null>(null)
    const activePreview =
        canReadList &&
        preview?.kind === urlState.view &&
        hasPermission(permissions, DETAIL_PERMISSIONS[preview.kind])
            ? preview
            : null
    const detailQuery = useSupplierScopeDetailQuery(
        activePreview?.kind ?? null,
        activePreview?.id ?? null,
    )
    const openPreview = (record: PreviewRecord) => {
        lastFocusedRow.current = record
        setPreview(record)
    }
    const { exporting, handleExport } = useScopeListExport(() =>
        exportSupplierScopeCsv(urlState.query),
    )

    const payableColumns = React.useMemo(createSupplierScopedPayableColumns, [])
    const paymentColumns = React.useMemo(createSupplierScopedPaymentColumns, [])
    const allocationColumns = React.useMemo(
        createSupplierScopedAllocationColumns,
        [],
    )
    const data =
        canReadList &&
        !listQuery.isError &&
        listQuery.data?.view === urlState.view
            ? listQuery.data
            : undefined
    const allShares = React.useMemo(
        () => data?.summary?.grouped ?? [],
        [data?.summary?.grouped],
    )
    const missingNameIds = React.useMemo(
        () =>
            allShares
                .filter((share) => {
                    const item = share as ShareWithOptionalName
                    return (
                        !item.owner_user_name?.trim() &&
                        !item.owner_name?.trim()
                    )
                })
                .map((share) => share.owner_user_id)
                .filter(Boolean),
        [allShares],
    )
    const selectedPeople = usePersonDirectorySelected(
        "procurement",
        missingNameIds,
        urlState.canUseOwner && missingNameIds.length > 0,
    )
    const resolvedOwnerNames = React.useMemo(() => {
        const map = new Map<string, string>()
        if (selectedPeople.isSuccess && selectedPeople.data) {
            for (const item of selectedPeople.data.items) {
                map.set(item.id, item.name.trim() || personDirectoryLabel(item))
            }
        }
        return map
    }, [selectedPeople.data, selectedPeople.isSuccess])

    const initialLoading =
        account.isPending || (canReadList && listQuery.isPending)
    const viewLabel = VIEW_LABEL[urlState.view]
    const hasAppliedChips = urlState.appliedChips.some(({ key }) =>
        APPLIED_CHIP_KEYS.some((chipKey) => chipKey === key),
    )
    const tableState = {
        id: TABLE_IDS[urlState.view],
        caption: viewLabel,
        entityName: "供应商往来记录",
        getRowId: (row: { id: string }) => row.id,
        rowCount: data?.total ?? 0,
        pagination: urlState.pagination,
        pageSizeOptions: [urlState.pagination.pageSize],
        onPaginationChange: (next) => {
            if (
                !data ||
                listQuery.isPlaceholderData ||
                next.pageIndex === urlState.pagination.pageIndex
            )
                return
            urlState.handlePaginationChange(next, data.scopeVersion)
        },
        showPagination: Boolean(data && data.hasScope && data.total > 0),
        showColumnVisibility: canReadList && !initialLoading,
        scrollable: true,
        loading: account.isPending || (canReadList && listQuery.isFetching),
        isError: canReadList ? listQuery.isError : account.isError,
        error: canReadList ? listQuery.error : account.error,
        onRetry: () =>
            void (canReadList ? listQuery.refetch() : account.refetch()),
        retryButtonId: "supplier-payables-scope-list-retry",
        noScope: Boolean(
            data && (data.emptyReason === "no_scope" || !data.hasScope),
        ),
        hasActiveFilters: urlState.hasActiveFilters,
        onClearFilters: urlState.clearFilters,
        clearFiltersButtonId: "supplier-payables-scope-empty-clear-filters",
        emptyCopy: {
            noScopeTitle: "暂无可查看的供应商往来数据",
            noScopeDescription:
                "您当前尚未配置供应商往来的数据查看范围，请联系管理员开通相应权限后再查询。",
            filterTitle: "无匹配往来记录",
            emptyTitle: "暂无供应商往来记录",
        },
    } satisfies Omit<ListDataTableProps<{ id: string }>, "columns" | "data">

    const table =
        account.isSuccess && !canReadList ? (
            <BusinessFailureState
                kind="permission"
                className="mx-(--scaffold-px) my-3"
            />
        ) : urlState.view === "payable" ? (
            <ListDataTable
                key={TABLE_IDS.payable}
                {...tableState}
                data={[...(data?.payables ?? [])]}
                columns={payableColumns}
                rowLabel={payableSourceNumber}
                defaultColumnOrder={["source", "visible", "whole", "status"]}
                defaultColumnPinning={{ left: ["source"] }}
                onRowPreview={
                    canPreviewPayable
                        ? (row) =>
                              openPreview({
                                  kind: "payable",
                                  id: row.id,
                                  number: payableSourceNumber(row),
                                  supplierName:
                                      row.supplier_name?.trim() || undefined,
                              })
                        : undefined
                }
                highlightedRowId={activePreview?.id}
            />
        ) : urlState.view === "payment" ? (
            <ListDataTable
                key={TABLE_IDS.payment}
                {...tableState}
                data={[...(data?.payments ?? [])]}
                columns={paymentColumns}
                rowLabel={(row) => row.payment_no}
                defaultColumnOrder={["doc", "visible", "whole", "status"]}
                defaultColumnPinning={{ left: ["doc"] }}
                onRowPreview={
                    canPreviewPayment
                        ? (row) =>
                              openPreview({
                                  kind: "payment",
                                  id: row.id,
                                  number: row.payment_no,
                                  supplierName:
                                      row.supplier_name?.trim() || undefined,
                              })
                        : undefined
                }
                highlightedRowId={activePreview?.id}
            />
        ) : (
            <ListDataTable
                key={TABLE_IDS.purchase_invoice}
                {...tableState}
                data={[...(data?.allocations ?? [])]}
                columns={allocationColumns}
                rowLabel={(row) =>
                    `${allocationNumber(row)}（${allocationPayableNumber(row)}）`
                }
                defaultColumnOrder={["doc", "visible", "whole"]}
                defaultColumnPinning={{ left: ["doc"] }}
            />
        )

    const detail = detailQuery.data
    const previewNumber =
        detail?.kind === "payable"
            ? payableSourceNumber(detail.payable)
            : (detail?.payment.payment_no ?? activePreview?.number)
    const previewSupplierName =
        detail?.kind === "payable"
            ? detail.payable.supplier_name?.trim()
            : detail?.kind === "payment"
              ? detail.payment.supplier_name?.trim()
              : activePreview?.supplierName
    const limited =
        detail?.kind === "payable"
            ? detail.payable.permission_limited
            : detail?.kind === "payment"
              ? detail.payment.permission_limited
              : false
    const showViews =
        account.isSuccess &&
        (allowedViews.length > 1 || (allowedViews.length === 1 && !canReadList))
    const listLimited =
        data?.summary?.permission_limited ||
        data?.payables.some((row) => row.permission_limited) ||
        data?.payments.some((row) => row.permission_limited) ||
        data?.allocations.some((row) => row.permission_limited) ||
        false

    const ownerGroups = allShares.map((share) => ({
        id: share.owner_user_id,
        name: resolveGroupedOwnerName(share, resolvedOwnerNames),
        amount: share.visible_share,
    }))

    return (
        <ListWorkspacePage
            title="供应商往来汇总"
            loading={account.isPending}
            views={
                showViews ? (
                    <Tabs
                        value={canReadList ? urlState.view : null}
                        onValueChange={(nextView) => {
                            const view = allowedViews.find(
                                (item) => item === nextView,
                            )
                            if (!view || view === urlState.view) return
                            setPreview(null)
                            urlState.changeView(view)
                        }}
                    >
                        <TabsList
                            variant="line"
                            aria-label="供应商往来视图"
                            className="w-full gap-5 overflow-x-auto"
                        >
                            {allowedViews.map((view) => (
                                <TabsTrigger
                                    key={view}
                                    value={view}
                                    id={`supplier-payables-scope-view-${view}`}
                                    className="min-h-workspace-view font-normal data-active:font-medium md:min-h-12"
                                >
                                    {VIEW_LABEL[view]}
                                </TabsTrigger>
                            ))}
                        </TabsList>
                    </Tabs>
                ) : undefined
            }
            headerActions={
                <div className="flex items-center gap-1">
                    <Button
                        id="supplier-payables-scope-back"
                        type="button"
                        variant="ghost"
                        size="sm"
                        className={listWorkspaceStyles.quietButton}
                        render={
                            <Link
                                href={`/finance/supplier-accounts?view=${urlState.view}`}
                            />
                        }
                    >
                        <ArrowLeftIcon
                            data-icon="inline-start"
                            aria-hidden="true"
                        />
                        返回台账
                    </Button>
                    {canReadList ? (
                        <LoadingButton
                            id="supplier-payables-scope-export"
                            type="button"
                            loading={exporting}
                            variant="ghost"
                            size="sm"
                            className={listWorkspaceStyles.quietButton}
                            disabled={
                                !data ||
                                listQuery.isPlaceholderData ||
                                data.total === 0
                            }
                            onClick={() => void handleExport()}
                        >
                            <DownloadIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            {exporting ? "导出中…" : "导出当前范围"}
                        </LoadingButton>
                    ) : null}
                </div>
            }
        >
            <div className="flex min-w-0 flex-col">
                <FundsScopeBanner
                    scopeSummary={data?.scopeSummary}
                    asOf={data?.asOf}
                    permissionLimited={listLimited}
                    unassigned={data?.summary?.unassigned}
                />
                {data?.summary ? (
                    <FundsScopeSummaryBar
                        wholeTotal={data.summary.whole_total}
                        unassigned={data.summary.unassigned}
                        groups={ownerGroups}
                        popoverIdPrefix="supplier-payables-scope-summary"
                    />
                ) : null}
            </div>

            <ListWorkSurface
                ariaLabel={`${viewLabel} · 供应商往来范围`}
                toolbar={
                    initialLoading ? (
                        <Skeleton
                            className="h-control w-full sm:w-60 lg:w-64 xl:w-72"
                            aria-busy
                        />
                    ) : canReadList ? (
                        <ListWorkspaceFilterBar
                            morePresentation="popover"
                            idPrefix={toolbarPrefix}
                            formAriaLabel="供应商往来范围查询"
                            onSubmit={urlState.applyFilters}
                            queryButtonId={`${toolbarPrefix}-apply`}
                            moreButtonId={`${toolbarPrefix}-more-filters`}
                            clearButtonId={`${toolbarPrefix}-clear-all`}
                            search={
                                <ListSearchField
                                    id={`${toolbarPrefix}-search`}
                                    searchInputRef={urlState.searchInputRef}
                                    value={urlState.searchDraft}
                                    onChange={urlState.setSearchDraft}
                                    placeholder="供应商、采购单、付款单"
                                    aria-label="搜索供应商往来范围"
                                />
                            }
                            moreCount={
                                urlState.appliedChips.filter(({ key }) =>
                                    TRAY_FILTER_KEYS.some(
                                        (trayKey) => trayKey === key,
                                    ),
                                ).length
                            }
                            moreOpen={urlState.panelOpen}
                            onToggleMore={() =>
                                urlState.panelOpen
                                    ? urlState.cancelMoreFilters()
                                    : urlState.setPanelOpen(true)
                            }
                            morePanelId={`${toolbarPrefix}-more-panel`}
                            morePanelAriaLabel="供应商往来范围更多筛选条件"
                            onResetMore={urlState.resetMoreFilters}
                            primaryFilters={
                                urlState.canUseOwner ? (
                                    <div className="w-44 max-w-full min-w-0 empty:hidden">
                                        <PersonDirectoryFilter
                                            id={`${toolbarPrefix}-procurement-owner`}
                                            category="procurement"
                                            label="采购负责人"
                                            hideLabel
                                            value={
                                                urlState.procurementOwnerDraft
                                            }
                                            onChange={
                                                urlState.setProcurementOwnerDraft
                                            }
                                            orgUnitIds={directoryOrgUnitIds}
                                            includeDescendants={
                                                urlState.descendantsDraft
                                            }
                                        />
                                    </div>
                                ) : null
                            }
                            extraPrimary={
                                urlState.hasActiveFilters &&
                                !hasAppliedChips ? (
                                    <Button
                                        id={`${toolbarPrefix}-clear-all`}
                                        type="button"
                                        variant="ghost"
                                        size="sm"
                                        aria-label="清空筛选条件"
                                        className={
                                            listWorkspaceStyles.quietButton
                                        }
                                        onClick={urlState.clearFilters}
                                    >
                                        {listFilterText.clear}
                                    </Button>
                                ) : undefined
                            }
                            morePanel={
                                urlState.canUseBusiness ||
                                urlState.canUseOrg ? (
                                    <ListFilterSections>
                                        <ListFilterSection legend="归属">
                                            {urlState.canUseBusiness ? (
                                                <PersonDirectoryFilter
                                                    id={`${toolbarPrefix}-operator`}
                                                    category="business"
                                                    label={
                                                        urlState.operatorLabel
                                                    }
                                                    value={
                                                        urlState.operatorDraft
                                                    }
                                                    onChange={
                                                        urlState.setOperatorDraft
                                                    }
                                                    orgUnitIds={
                                                        directoryOrgUnitIds
                                                    }
                                                    includeDescendants={
                                                        urlState.descendantsDraft
                                                    }
                                                />
                                            ) : null}
                                            {urlState.canUseOrg ? (
                                                <OrganizationUnitFilter
                                                    compact
                                                    id={`${toolbarPrefix}-org`}
                                                    label="业务组织"
                                                    value={urlState.orgDraft}
                                                    onChange={
                                                        urlState.setOrgDraft
                                                    }
                                                    includeDescendants={
                                                        urlState.descendantsDraft
                                                    }
                                                    onDescendantsChange={
                                                        urlState.setDescendantsDraft
                                                    }
                                                />
                                            ) : null}
                                        </ListFilterSection>
                                    </ListFilterSections>
                                ) : null
                            }
                            resultStatus={
                                listQuery.isError
                                    ? "查询未完成"
                                    : listQuery.isFetching
                                      ? "查询中…"
                                      : null
                            }
                            chips={urlState.appliedChips.map((chip) => ({
                                ...chip,
                                onClear: () => urlState.removeFilter(chip.key),
                            }))}
                            foldedChipKeys={APPLIED_CHIP_KEYS}
                            onClearAll={urlState.clearFilters}
                            hasPendingChanges={urlState.hasPendingChanges}
                            pendingHint="条件已修改，待查询 · 导出仍按已生效条件"
                        />
                    ) : undefined
                }
                table={table}
            />

            <QuickPreviewSheet
                id="supplier-payables-scope-preview-sheet"
                open={activePreview != null}
                onOpenChange={(nextOpen) => {
                    if (!nextOpen) setPreview(null)
                }}
                finalFocus={() => {
                    const focused = lastFocusedRow.current
                    return (
                        (focused &&
                            document
                                .getElementById(TABLE_IDS[focused.kind])
                                ?.querySelector<HTMLElement>(
                                    `[data-row-id="${CSS.escape(focused.id)}"][tabindex]`,
                                )) ||
                        urlState.searchInputRef.current
                    )
                }}
                size="detail"
                identity={
                    activePreview && previewNumber ? (
                        <span className="num">
                            {activePreview.kind === "payable"
                                ? "来源单号"
                                : "付款单号"}
                            ：{previewNumber}
                        </span>
                    ) : undefined
                }
                title={
                    previewSupplierName ||
                    (activePreview?.kind === "payable"
                        ? "应付记录"
                        : activePreview?.kind === "payment"
                          ? "供应商付款"
                          : "往来记录")
                }
                footer={
                    <Button
                        id="supplier-payables-scope-preview-close"
                        type="button"
                        variant="outline"
                        onClick={() => setPreview(null)}
                    >
                        关闭
                    </Button>
                }
            >
                {detailQuery.isPending ? (
                    <div className="px-sheet-inline py-5">
                        <Skeleton className="h-24" aria-busy />
                    </div>
                ) : detailQuery.isError ? (
                    <div className="flex flex-col items-start gap-3 px-sheet-inline py-5">
                        <p className="text-xs text-muted-foreground">
                            {getErrorMessage(
                                detailQuery.error,
                                "详情加载失败，请重试。",
                            )}
                        </p>
                        <Button
                            id="supplier-payables-scope-preview-retry"
                            type="button"
                            size="sm"
                            variant="outline"
                            onClick={() => void detailQuery.refetch()}
                        >
                            重试
                        </Button>
                    </div>
                ) : detail?.kind === "payable" ? (
                    <div className="flex flex-col gap-6 px-sheet-inline py-5 text-body-compact">
                        <PreviewAmount
                            label="权限内已核销"
                            value={detail.payable.visible_settled_share}
                        >
                            <span>
                                整单金额{" "}
                                <MoneyValue
                                    value={detail.payable.gross_total}
                                    unavailableReason={
                                        limited
                                            ? scopeText.wholeRestricted
                                            : undefined
                                    }
                                />
                            </span>
                            <span>
                                未分配{" "}
                                <MoneyValue
                                    value={detail.payable.open_total}
                                    unavailableReason={
                                        limited
                                            ? scopeText.wholeRestricted
                                            : undefined
                                    }
                                />
                            </span>
                        </PreviewAmount>
                        {limited ? (
                            <PreviewNote>
                                {scopeText.limitedOnlyVisibleShare}
                            </PreviewNote>
                        ) : null}
                    </div>
                ) : detail?.kind === "payment" ? (
                    <div className="flex flex-col gap-6 px-sheet-inline py-5 text-body-compact">
                        <PreviewAmount
                            label="权限内已分配"
                            value={detail.payment.visible_allocated_share}
                        >
                            <span>
                                整单金额{" "}
                                <MoneyValue
                                    value={detail.payment.amount}
                                    unavailableReason={
                                        limited
                                            ? scopeText.wholeRestricted
                                            : undefined
                                    }
                                />
                            </span>
                            <span>
                                未分配{" "}
                                <MoneyValue
                                    value={detail.payment.unallocated_amount}
                                    unavailableReason={
                                        limited
                                            ? scopeText.wholeRestricted
                                            : undefined
                                    }
                                />
                            </span>
                        </PreviewAmount>
                        {limited ? (
                            <PreviewNote>
                                {scopeText.limitedOnlyVisibleShare}
                            </PreviewNote>
                        ) : null}
                    </div>
                ) : (
                    <div className="px-sheet-inline py-5 text-body-compact text-muted-foreground">
                        未找到该笔记录，可能已超出当前数据范围。
                    </div>
                )}
            </QuickPreviewSheet>
        </ListWorkspacePage>
    )
}
