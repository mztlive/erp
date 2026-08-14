"use client"

import { SearchIcon } from "lucide-react"

import {
    ListToolbar,
    MultiOptionCombobox,
    OptionCombobox,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { DatePicker } from "@/components/ui/date-picker"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { SupplierSearchCombobox } from "@/features/entity-selectors"
import type {
    SupplierOrdersUrlState,
    SupplierOrdersUrlUpdater,
} from "@/features/supplier-orders/lib/url-state"
import type {
    CancelStatus,
    ListView,
    RefundStatus,
    SupplierFulfillmentStatus,
} from "@/features/supplier-orders/types"
import {
    CANCEL_STATUS_LABEL,
    CANCEL_STATUSES,
    FULFILLMENT_STATUS_LABEL,
    FULFILLMENT_STATUSES,
    REFUND_STATUS_LABEL,
    REFUND_STATUSES,
    VIEW_LABEL,
} from "@/features/supplier-orders/types"

export type SupplierOrdersListToolbarProps = {
    url: SupplierOrdersUrlState
    total: number
    hasActiveFilters: boolean
    updateUrl: SupplierOrdersUrlUpdater
    clearFilters: () => void
    searchDraft: string
    onSearchDraftChange: (value: string) => void
    onSearchCommit: () => void
    onSearchBlur: () => void
}

export function SupplierOrdersListToolbar({
    url,
    total,
    hasActiveFilters,
    updateUrl,
    clearFilters,
    searchDraft,
    onSearchDraftChange,
    onSearchCommit,
    onSearchBlur,
}: SupplierOrdersListToolbarProps) {
    return (
        <ListToolbar
            search={
                <InputGroup className="w-full max-w-sm">
                    <InputGroupAddon>
                        <SearchIcon className="size-3.5" />
                    </InputGroupAddon>
                    <InputGroupInput
                        data-slot="sfo-list-search"
                        value={searchDraft}
                        onChange={(e) => onSearchDraftChange(e.target.value)}
                        onKeyDown={(e) => {
                            if (e.key === "Enter") {
                                onSearchCommit()
                            }
                        }}
                        onBlur={onSearchBlur}
                        placeholder="供应商订单号、商城订单、外部单号"
                        aria-label="搜索供应商订单"
                    />
                </InputGroup>
            }
            filters={
                <>
                    <ToggleGroup
                        value={[url.view]}
                        onValueChange={(values) => {
                            const next = values[0] as ListView | undefined
                            if (next) updateUrl({ view: next, page: 1 })
                        }}
                        variant="outline"
                        size="sm"
                        spacing={0}
                        aria-label="列表视图"
                    >
                        {(Object.keys(VIEW_LABEL) as ListView[]).map((v) => (
                            <ToggleGroupItem key={v} value={v}>
                                {VIEW_LABEL[v]}
                            </ToggleGroupItem>
                        ))}
                    </ToggleGroup>
                    <SupplierSearchCombobox
                        value={url.supplierId || undefined}
                        onValueChange={(id) =>
                            updateUrl({
                                supplierId: id || undefined,
                                page: 1,
                            })
                        }
                        purpose="filter"
                        aria-label="供应商"
                        className="w-[12rem]"
                        placeholder="全部供应商"
                    />
                    <MultiOptionCombobox
                        value={url.fulfillmentStatuses ?? []}
                        onValueChange={(values) =>
                            updateUrl({
                                fulfillmentStatuses:
                                    values.length > 0
                                        ? (values as SupplierFulfillmentStatus[])
                                        : undefined,
                                page: 1,
                            })
                        }
                        options={FULFILLMENT_STATUSES.map((s) => ({
                            value: s,
                            label: FULFILLMENT_STATUS_LABEL[s],
                        }))}
                        aria-label="履约状态"
                        className="w-[10rem]"
                        size="sm"
                        placeholder="履约·全部"
                    />
                </>
            }
            secondary={
                <>
                    <OptionCombobox
                        value={url.cancelStatuses?.[0] ?? ""}
                        onValueChange={(v) =>
                            updateUrl({
                                cancelStatuses: v
                                    ? [v as CancelStatus]
                                    : undefined,
                                page: 1,
                            })
                        }
                        options={[
                            { value: "", label: "取消·全部" },
                            ...CANCEL_STATUSES.map((s) => ({
                                value: s,
                                label: CANCEL_STATUS_LABEL[s],
                            })),
                        ]}
                        aria-label="取消状态"
                        className="w-[7.5rem]"
                        size="sm"
                        allowClear={false}
                        placeholder="取消·全部"
                    />
                    <OptionCombobox
                        value={url.refundStatuses?.[0] ?? ""}
                        onValueChange={(v) =>
                            updateUrl({
                                refundStatuses: v
                                    ? [v as RefundStatus]
                                    : undefined,
                                page: 1,
                            })
                        }
                        options={[
                            { value: "", label: "退款·全部" },
                            ...REFUND_STATUSES.map((s) => ({
                                value: s,
                                label: REFUND_STATUS_LABEL[s],
                            })),
                        ]}
                        aria-label="退款状态"
                        className="w-[7.5rem]"
                        size="sm"
                        allowClear={false}
                        placeholder="退款·全部"
                    />
                    <span className="flex items-center gap-1 text-xs text-muted-foreground">
                        支付自
                        <DatePicker
                            className="w-[9.5rem]"
                            value={url.paidFrom || undefined}
                            onValueChange={(next) =>
                                updateUrl({
                                    paidFrom: next || undefined,
                                    page: 1,
                                })
                            }
                        />
                    </span>
                    <span className="flex items-center gap-1 text-xs text-muted-foreground">
                        至
                        <DatePicker
                            className="w-[9.5rem]"
                            value={url.paidTo || undefined}
                            onValueChange={(next) =>
                                updateUrl({
                                    paidTo: next || undefined,
                                    page: 1,
                                })
                            }
                        />
                    </span>
                </>
            }
            actions={
                <div className="flex items-center gap-2 text-xs text-muted-foreground">
                    <span aria-live="polite">
                        共 {total.toLocaleString("zh-CN")} 条
                    </span>
                    {hasActiveFilters ? (
                        <Button
                            type="button"
                            size="xs"
                            variant="ghost"
                            onClick={clearFilters}
                        >
                            清除筛选
                        </Button>
                    ) : null}
                </div>
            }
        />
    )
}
