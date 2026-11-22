"use client"

import * as React from "react"
import Link from "next/link"
import { ArrowLeftIcon, DownloadIcon } from "lucide-react"

import {
    BusinessStatusBadge,
    FundsScopeBanner,
    ListDataTable,
    ListFilterSection,
    ListFilterSections,
    ListSearchField,
    ListWorkspaceFilterBar,
    ListWorkspacePage,
    MoneyValue,
    QuickPreviewSheet,
    TableEmptyCell,
    TableIdentityCell,
    TableTextCell,
    listWorkspaceStyles,
} from "@/components/business"
import {
    PreviewAmount,
    PreviewFact,
    PreviewNote,
    PreviewSection,
} from "@/components/business/financial-preview"
import { OrganizationUnitFilter } from "@/features/organization/components/organization-unit-filter"
import { selectedOrgUnitIds } from "@/features/organization/lib/organization-ids"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import type { StatusTone } from "@/components/ui/status-badge"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { personDirectoryLabel } from "@/features/entity-selectors/api/person-directory"
import { PersonDirectoryFilter } from "@/features/entity-selectors/components/person-directory-filter"
import { usePersonDirectorySelected } from "@/features/entity-selectors/hooks/person-directory"
import type { ColumnDef } from "@tanstack/react-table"
import { getErrorMessage } from "@/lib/api/errors"
import { restoreListRowFocus } from "@/lib/list-row-focus"
import { hasPermission } from "@/lib/permissions"
import { excelExportText, listFilterText } from "@/lib/ui-text"
import { useSearchHotkey } from "@/lib/use-search-hotkey"
import { useScopeExcelExport } from "@/lib/use-scope-list-export"
import {
    requestStatusLabels,
    type RequestStatus,
} from "@/features/invoice-requests/api"
import type { ScopedInvoiceRequestWire } from "@/features/invoice-requests/scoped"
import { exportInvoiceRequestScopeExcel } from "@/features/invoice-requests/scoped-export"
import { InvoiceRequestDetails } from "./request-details"
import {
    useInvoiceRequestScopeDetailQuery,
    useInvoiceRequestScopeListQuery,
} from "@/features/invoice-requests/hooks/scoped-queries"
import { useInvoiceRequestScopeUrlState } from "@/features/invoice-requests/hooks/use-scope-url-state"

const toolbarPrefix = "invoice-request-scope-toolbar"
const trayFilterKeys = ["applicantUserIds", "handlerUserIds", "orgUnitIds"]
const foldedChipKeys = [...trayFilterKeys, "salesOrderId", "status"]

const requestStatusTones: Record<RequestStatus, StatusTone> = {
    draft: "neutral",
    rejected: "destructive",
    in_approval: "warning",
    approved: "info",
    completed: "success",
}

type ScopedRequestWithOptionalNames = ScopedInvoiceRequestWire & {
    applicant_user_name?: string | null
    applicant_name?: string | null
    created_by_name?: string | null
    handler_user_name?: string | null
    handler_name?: string | null
}

function isInternalIdentifier(value: string): boolean {
    const trimmed = value.trim()
    return (
        /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(
            trimmed,
        ) || /^[a-z]{2,6}_[a-z0-9_-]{6,}$/i.test(trimmed)
    )
}

function requestStatus(status: string): RequestStatus | undefined {
    const normalized = status.trim().toLowerCase()
    return Object.hasOwn(requestStatusLabels, normalized)
        ? (normalized as RequestStatus)
        : undefined
}

function requestStatusLabel(status: RequestStatus | undefined): string {
    return status ? requestStatusLabels[status] : "状态待确认"
}

function resolveScopedPersonName(
    row: ScopedRequestWithOptionalNames,
    role: "applicant" | "handler",
    resolvedNames: ReadonlyMap<string, string>,
): string {
    if (role === "applicant") {
        const inline =
            row.applicant_user_name?.trim() ||
            row.applicant_name?.trim() ||
            row.created_by_name?.trim() ||
            ""
        if (inline) return inline
        const fromDirectory = resolvedNames.get(row.applicant_user_id)
        if (fromDirectory) return fromDirectory
        const rawId = row.applicant_user_id.trim()
        return !rawId || isInternalIdentifier(rawId) ? "当前不可见" : rawId
    }
    if (!row.handler_user_id) return ""
    const inline =
        row.handler_user_name?.trim() || row.handler_name?.trim() || ""
    if (inline) return inline
    const fromDirectory = resolvedNames.get(row.handler_user_id)
    if (fromDirectory) return fromDirectory
    const rawId = row.handler_user_id.trim()
    return !rawId || isInternalIdentifier(rawId) ? "当前不可见" : rawId
}

function scopeColumns(
    resolvedNames: ReadonlyMap<string, string>,
): ColumnDef<ScopedInvoiceRequestWire>[] {
    return [
        {
            id: "doc",
            header: "申请单号 / 开票抬头",
            meta: { label: "申请单号 / 开票抬头", width: "flex" },
            cell: ({ row }) => {
                const title = row.original.data?.invoice_title?.trim()
                const secondary = [title, row.original.sales_order_no]
                    .filter(Boolean)
                    .join(" · ")
                return (
                    <TableIdentityCell
                        primary={
                            <span className="num">
                                {row.original.request_no}
                            </span>
                        }
                        primaryTitle={row.original.request_no}
                        secondary={secondary || undefined}
                        secondaryTitle={secondary || undefined}
                        numericSecondary={!title}
                    />
                )
            },
        },
        {
            id: "amount",
            header: "申请金额",
            meta: {
                label: "申请金额",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => (
                <MoneyValue
                    value={row.original.amount}
                    display="exact"
                    className="font-semibold"
                />
            ),
        },
        {
            id: "people",
            header: "申请人 / 当前处理人",
            meta: { label: "经办", width: "default" },
            cell: ({ row }) => {
                const applicantLabel = resolveScopedPersonName(
                    row.original as ScopedRequestWithOptionalNames,
                    "applicant",
                    resolvedNames,
                )
                const handlerLabel = resolveScopedPersonName(
                    row.original as ScopedRequestWithOptionalNames,
                    "handler",
                    resolvedNames,
                )
                return (
                    <div className="flex min-w-0 flex-col">
                        <TableTextCell value={applicantLabel} clamp={1} />
                        {row.original.handler_user_id ? (
                            <TableTextCell
                                value={`处理人 ${handlerLabel}`}
                                clamp={1}
                                className="text-xs text-muted-foreground"
                            />
                        ) : (
                            <TableEmptyCell
                                variant="label"
                                label="暂无当前处理人"
                            />
                        )}
                    </div>
                )
            },
        },
        {
            id: "status",
            header: "状态",
            meta: { label: "状态", width: "status" },
            cell: ({ row }) => {
                const status = requestStatus(row.original.status)
                return (
                    <BusinessStatusBadge
                        context="list"
                        label={requestStatusLabel(status)}
                        tone={status ? requestStatusTones[status] : "neutral"}
                        appearance={status === "completed" ? "quiet" : "badge"}
                    />
                )
            },
        },
    ]
}

/** 开票申请范围页：紧凑口径条 + 标准列表 + 768px 详情抽屉。 */
export function InvoiceRequestScopePage() {
    const urlState = useInvoiceRequestScopeUrlState()
    useSearchHotkey(urlState.searchInputRef)
    const directoryOrgUnitIds = selectedOrgUnitIds(urlState.orgDraft)
    const listQuery = useInvoiceRequestScopeListQuery(urlState.query)
    const [detailId, setDetailId] = React.useState<string | null>(null)
    const detailQuery = useInvoiceRequestScopeDetailQuery(detailId)
    const data = listQuery.data
    const noScope = Boolean(
        data && (data.emptyReason === "no_scope" || !data.hasScope),
    )
    const rowCount = noScope || listQuery.isError ? 0 : (data?.total ?? 0)
    const rows = React.useMemo(
        () => (noScope ? [] : [...(data?.requests ?? [])]),
        [data?.requests, noScope],
    )
    const missingPersonIds = React.useMemo(() => {
        const ids = new Set<string>()
        const collect = (row: ScopedRequestWithOptionalNames) => {
            if (
                row.applicant_user_id &&
                !row.applicant_user_name?.trim() &&
                !row.applicant_name?.trim() &&
                !row.created_by_name?.trim()
            ) {
                ids.add(row.applicant_user_id)
            }
            if (
                row.handler_user_id &&
                !row.handler_user_name?.trim() &&
                !row.handler_name?.trim()
            ) {
                ids.add(row.handler_user_id)
            }
        }
        for (const row of data?.requests ?? []) {
            collect(row as ScopedRequestWithOptionalNames)
        }
        if (detailQuery.data) {
            collect(detailQuery.data as ScopedRequestWithOptionalNames)
        }
        return [...ids]
    }, [data?.requests, detailQuery.data])
    const account = useAccountProfileQuery()
    const canReadBusiness = hasPermission(
        account.data?.permissions,
        "business_person:list",
    )
    const selectedPeople = usePersonDirectorySelected(
        "business",
        missingPersonIds,
        canReadBusiness && missingPersonIds.length > 0,
    )
    const resolvedNames = React.useMemo(() => {
        const map = new Map<string, string>()
        if (selectedPeople.isSuccess && selectedPeople.data) {
            for (const item of selectedPeople.data.items) {
                map.set(item.id, item.name.trim() || personDirectoryLabel(item))
            }
        }
        return map
    }, [selectedPeople.data, selectedPeople.isSuccess])
    const columns = React.useMemo(
        () => scopeColumns(resolvedNames),
        [resolvedNames],
    )
    const exportExcel = useScopeExcelExport(exportInvoiceRequestScopeExcel)
    const canExport =
        account.isSuccess &&
        hasPermission(account.data.permissions, "sales_invoice_request:list")
    const appliedChips = [
        ...urlState.appliedChips.map((chip) => ({
            ...chip,
            onClear: () => urlState.removeFilter(chip.key),
        })),
        ...(urlState.status
            ? [
                  {
                      key: "status",
                      label: `状态：${requestStatusLabel(requestStatus(urlState.status))}`,
                      onClear: () =>
                          urlState.patchUrl(
                              { status: null, scopeVersion: null, page: null },
                              { replace: true },
                          ),
                  },
              ]
            : []),
    ]
    const hasFoldedChips = appliedChips.some(({ key }) =>
        foldedChipKeys.includes(key),
    )

    const detail = detailQuery.data
    const detailStatus = detail ? requestStatus(detail.status) : undefined
    const closeDetail = () => {
        const id = detailId
        setDetailId(null)
        if (id) restoreListRowFocus(id)
    }

    return (
        <ListWorkspacePage
            title="开票申请汇总"
            headerActions={
                <div className="flex items-center gap-1">
                    <Button
                        id="invoice-request-scope-back"
                        type="button"
                        variant="ghost"
                        size="sm"
                        className={listWorkspaceStyles.quietButton}
                        render={
                            <Link href="/finance/customer-accounts/invoice-requests" />
                        }
                    >
                        <ArrowLeftIcon
                            data-icon="inline-start"
                            aria-hidden="true"
                        />
                        返回台账
                    </Button>
                    {canExport ? (
                        <Button
                            id="invoice-request-scope-export"
                            type="button"
                            variant="ghost"
                            size="sm"
                            className={listWorkspaceStyles.quietButton}
                            disabled={
                                exportExcel.isPending ||
                                listQuery.isFetching ||
                                listQuery.isError ||
                                !data?.hasScope ||
                                !data.total
                            }
                            title="导出当前筛选下的全部开票申请"
                            onClick={() => exportExcel.mutate(urlState.query)}
                        >
                            <DownloadIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            {exportExcel.isPending
                                ? excelExportText.pending
                                : excelExportText.action}
                        </Button>
                    ) : null}
                </div>
            }
            loading={listQuery.isPending && !data}
            notices={
                <FundsScopeBanner
                    scopeSummary={data?.scopeSummary}
                    asOf={data?.asOf}
                    permissionLimited={
                        data?.requests.some((row) => row.permission_limited) ??
                        false
                    }
                    unassigned={data?.summary?.unassigned}
                />
            }
            surface={{
                ariaLabel: "开票申请范围列表",
                toolbar: (
                    <ListWorkspaceFilterBar
                        morePresentation="popover"
                        idPrefix={toolbarPrefix}
                        formAriaLabel="开票申请范围查询"
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
                                placeholder="申请单号、抬头、事由"
                                aria-label="搜索开票申请范围"
                            />
                        }
                        moreCount={
                            appliedChips.filter(({ key }) =>
                                trayFilterKeys.includes(key),
                            ).length
                        }
                        moreOpen={urlState.panelOpen}
                        onToggleMore={() =>
                            urlState.panelOpen
                                ? urlState.cancelMoreFilters()
                                : urlState.setPanelOpen(true)
                        }
                        morePanelId={`${toolbarPrefix}-more-panel`}
                        morePanelAriaLabel="开票申请范围更多筛选条件"
                        onResetMore={urlState.resetMoreFilters}
                        primaryFilters={
                            urlState.canUseOwner ? (
                                <div className="w-56 max-w-full min-w-0 empty:hidden">
                                    <PersonDirectoryFilter
                                        id={`${toolbarPrefix}-sales-owner`}
                                        category="sales"
                                        label="负责销售"
                                        hideLabel
                                        value={urlState.salesOwnerDraft}
                                        onChange={urlState.setSalesOwnerDraft}
                                        orgUnitIds={directoryOrgUnitIds}
                                        includeDescendants={
                                            urlState.descendantsDraft
                                        }
                                    />
                                </div>
                            ) : null
                        }
                        extraPrimary={
                            urlState.hasActiveFilters && !hasFoldedChips ? (
                                <Button
                                    id={`${toolbarPrefix}-clear-all`}
                                    type="button"
                                    variant="ghost"
                                    size="sm"
                                    onClick={urlState.clearFilters}
                                >
                                    {listFilterText.clear}
                                </Button>
                            ) : null
                        }
                        morePanel={
                            urlState.canUseBusiness || urlState.canUseOrg ? (
                                <ListFilterSections>
                                    {urlState.canUseBusiness && (
                                        <ListFilterSection legend="人员">
                                            <PersonDirectoryFilter
                                                id={`${toolbarPrefix}-applicant`}
                                                category="business"
                                                label="申请人"
                                                value={urlState.applicantDraft}
                                                onChange={
                                                    urlState.setApplicantDraft
                                                }
                                                orgUnitIds={directoryOrgUnitIds}
                                                includeDescendants={
                                                    urlState.descendantsDraft
                                                }
                                            />
                                            <PersonDirectoryFilter
                                                id={`${toolbarPrefix}-handler`}
                                                category="business"
                                                label="当前开票处理人"
                                                value={urlState.handlerDraft}
                                                onChange={
                                                    urlState.setHandlerDraft
                                                }
                                                orgUnitIds={directoryOrgUnitIds}
                                                includeDescendants={
                                                    urlState.descendantsDraft
                                                }
                                            />
                                        </ListFilterSection>
                                    )}
                                    {urlState.canUseOrg && (
                                        <ListFilterSection legend="组织">
                                            <OrganizationUnitFilter
                                                compact
                                                id={`${toolbarPrefix}-org`}
                                                label="业务组织"
                                                value={urlState.orgDraft}
                                                onChange={urlState.setOrgDraft}
                                                includeDescendants={
                                                    urlState.descendantsDraft
                                                }
                                                onDescendantsChange={
                                                    urlState.setDescendantsDraft
                                                }
                                            />
                                        </ListFilterSection>
                                    )}
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
                        chips={appliedChips}
                        foldedChipKeys={foldedChipKeys}
                        onClearAll={urlState.clearFilters}
                        hasPendingChanges={urlState.hasPendingChanges}
                        pendingHint="条件已修改，待查询 · 导出仍按已生效条件"
                    />
                ),
                table: (
                    <ListDataTable
                        id="invoice-request-scope-list"
                        caption="开票申请范围列表"
                        entityName="开票申请"
                        data={rows}
                        columns={columns}
                        defaultColumnOrder={[
                            "doc",
                            "amount",
                            "people",
                            "status",
                        ]}
                        defaultColumnPinning={{ left: ["doc"] }}
                        getRowId={(row) => row.id}
                        rowLabel={(row) => row.request_no}
                        onRowPreview={(row) => setDetailId(row.id)}
                        highlightedRowId={detailId ?? undefined}
                        rowCount={rowCount}
                        pagination={urlState.pagination}
                        pageSizeOptions={[urlState.pagination.pageSize]}
                        onPaginationChange={(next) => {
                            if (
                                listQuery.isPlaceholderData ||
                                next.pageIndex === urlState.pagination.pageIndex
                            )
                                return
                            urlState.handlePaginationChange(
                                next,
                                data?.scopeVersion,
                            )
                        }}
                        showPagination={rowCount > 0}
                        loading={listQuery.isFetching}
                        isError={listQuery.isError}
                        error={listQuery.error}
                        onRetry={() => void listQuery.refetch()}
                        retryButtonId="invoice-request-scope-list-retry"
                        noScope={noScope}
                        hasActiveFilters={urlState.hasActiveFilters}
                        onClearFilters={urlState.clearFilters}
                        clearFiltersButtonId="invoice-request-scope-empty-clear-filters"
                        emptyCopy={{
                            noScopeTitle: "暂无可查看的开票申请数据",
                            noScopeDescription:
                                "您当前尚未配置开票申请的数据查看范围，请联系管理员开通权限后再查询。",
                            filterTitle: "无匹配开票申请",
                            filterDescription: "无匹配记录，可清除筛选后重试。",
                            emptyTitle: "暂无开票申请",
                        }}
                    />
                ),
            }}
        >
            <QuickPreviewSheet
                id="invoice-request-scope-detail-sheet"
                open={Boolean(detailId)}
                onOpenChange={(nextOpen) => {
                    if (!nextOpen) closeDetail()
                }}
                size="detail"
                title={detail?.data?.invoice_title?.trim() || "开票申请详情"}
                identity={
                    detail
                        ? `申请单号：${detail.request_no} · 销售单：${detail.sales_order_no}`
                        : undefined
                }
                summary={
                    detail ? (
                        <BusinessStatusBadge
                            context="preview"
                            label={requestStatusLabel(detailStatus)}
                            tone={
                                detailStatus
                                    ? requestStatusTones[detailStatus]
                                    : "neutral"
                            }
                        />
                    ) : undefined
                }
                footer={
                    <Button
                        id="invoice-request-scope-detail-close"
                        type="button"
                        variant="outline"
                        onClick={closeDetail}
                    >
                        关闭
                    </Button>
                }
            >
                {detailQuery.isPending ? (
                    <div className="space-y-4 px-7 py-6">
                        <Skeleton className="h-24" aria-busy />
                        <Skeleton className="h-44" aria-busy />
                    </div>
                ) : detailQuery.isError ? (
                    <div role="alert" className="space-y-3 px-7 py-6">
                        <p className="text-xs text-muted-foreground">
                            {getErrorMessage(
                                detailQuery.error,
                                "申请详情读取失败",
                            )}
                        </p>
                        <Button
                            id="invoice-request-scope-detail-retry"
                            type="button"
                            variant="outline"
                            size="sm"
                            onClick={() => void detailQuery.refetch()}
                        >
                            重试
                        </Button>
                    </div>
                ) : detail ? (
                    <div className="space-y-6 px-7 py-6 text-body-compact">
                        <PreviewAmount label="申请金额" value={detail.amount}>
                            <span>
                                销售单{" "}
                                <span className="num">
                                    {detail.sales_order_no || "—"}
                                </span>
                            </span>
                        </PreviewAmount>
                        <PreviewSection title="经办信息">
                            <dl className="grid gap-x-8 gap-y-3 sm:grid-cols-2">
                                <PreviewFact label="申请人">
                                    {resolveScopedPersonName(
                                        detail,
                                        "applicant",
                                        resolvedNames,
                                    )}
                                </PreviewFact>
                                <PreviewFact label="当前处理人">
                                    {detail.handler_user_id
                                        ? resolveScopedPersonName(
                                              detail,
                                              "handler",
                                              resolvedNames,
                                          )
                                        : "—"}
                                </PreviewFact>
                            </dl>
                        </PreviewSection>
                        <PreviewSection title="开票资料与明细">
                            <InvoiceRequestDetails data={detail.data} />
                        </PreviewSection>
                        {detail.permission_limited ? (
                            <PreviewNote>
                                部分金额受限时仅显示本人权限内金额
                            </PreviewNote>
                        ) : null}
                    </div>
                ) : (
                    <p className="px-7 py-6 text-xs text-muted-foreground">
                        未找到该申请，可能已超出当前数据范围。
                    </p>
                )}
            </QuickPreviewSheet>
        </ListWorkspacePage>
    )
}
