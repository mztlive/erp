"use client"

import * as React from "react"
import { SearchIcon } from "lucide-react"

import { ListToolbar, OptionCombobox } from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { Label } from "@/components/ui/label"
import { SupplierSearchCombobox } from "@/features/entity-selectors"
import type {
    AllocationTrack,
    PayableSourceType,
    SupplierAccountsView,
} from "@/features/supplier-payables/types"

export interface SupplierAccountsToolbarProps {
    view: SupplierAccountsView
    trackFilter: AllocationTrack | "all"
    supplierId: string | undefined
    sourceType: PayableSourceType | undefined
    status: string | undefined
    searchInput: string
    onSearchInputChange: (value: string) => void
    searchInputRef: React.Ref<HTMLInputElement>
    hasActiveFilters: boolean
    onClearFilters: () => void
    onFilter: (patch: Record<string, string | null | undefined>) => void
}

export function SupplierAccountsToolbar({
    view,
    trackFilter,
    supplierId,
    sourceType,
    status,
    searchInput,
    onSearchInputChange,
    searchInputRef,
    hasActiveFilters,
    onClearFilters,
    onFilter,
}: SupplierAccountsToolbarProps) {
    return (
        <ListToolbar
            search={
                <InputGroup className="max-w-md">
                    <InputGroupAddon>
                        <SearchIcon className="size-4" />
                    </InputGroupAddon>
                    <InputGroupInput
                        ref={searchInputRef}
                        placeholder="供应商、采购单、付款单、发票号"
                        value={searchInput}
                        onChange={(e) =>
                            onSearchInputChange(e.target.value)
                        }
                        aria-label="搜索供应商往来"
                    />
                </InputGroup>
            }
            filters={
                <div className="flex flex-wrap items-end gap-2">
                    <div>
                        <Label className="sr-only">供应商</Label>
                        <SupplierSearchCombobox
                            value={supplierId || undefined}
                            onValueChange={(id) => {
                                onFilter({
                                    supplierId: id || null,
                                    page: null,
                                })
                            }}
                            purpose="filter"
                            className="w-[12rem]"
                            aria-label="供应商"
                            placeholder="全部供应商"
                        />
                    </div>
                    {view === "unallocated" ? (
                        <div>
                            <Label className="sr-only">轨道</Label>
                            <OptionCombobox
                                value={trackFilter}
                                onValueChange={(v) => {
                                    onFilter({
                                        track: v && v !== "all" ? v : null,
                                        page: null,
                                    })
                                }}
                                options={[
                                    {
                                        value: "all",
                                        label: "全部轨道",
                                    },
                                    {
                                        value: "payment",
                                        label: "付款",
                                    },
                                    {
                                        value: "purchase_invoice",
                                        label: "进项票",
                                    },
                                ]}
                                className="w-36"
                                size="sm"
                                allowClear={false}
                                aria-label="轨道"
                                placeholder="轨道"
                            />
                        </div>
                    ) : null}
                    {view === "payable" ? (
                        <>
                            <div>
                                <Label className="sr-only">
                                    来源类型
                                </Label>
                                <OptionCombobox
                                    value={sourceType ?? ""}
                                    onValueChange={(v) => {
                                        onFilter({
                                            sourceType: v || null,
                                            page: null,
                                        })
                                    }}
                                    options={[
                                        {
                                            value: "",
                                            label: "全部来源",
                                        },
                                        {
                                            value: "PURCHASE_ORDER",
                                            label: "采购单",
                                        },
                                        {
                                            value: "SUPPLIER_SETTLEMENT",
                                            label: "供应商结算单",
                                        },
                                    ]}
                                    className="w-[9rem]"
                                    size="sm"
                                    allowClear={false}
                                    aria-label="来源类型"
                                    placeholder="全部来源"
                                />
                            </div>
                            <div>
                                <Label className="sr-only">状态</Label>
                                <OptionCombobox
                                    value={status ?? ""}
                                    onValueChange={(v) => {
                                        onFilter({
                                            status: v || null,
                                            page: null,
                                        })
                                    }}
                                    options={[
                                        {
                                            value: "",
                                            label: "全部状态",
                                        },
                                        {
                                            value: "OPEN",
                                            label: "未结",
                                        },
                                        {
                                            value: "PARTIAL",
                                            label: "部分结清",
                                        },
                                        {
                                            value: "SETTLED",
                                            label: "已结清",
                                        },
                                    ]}
                                    className="w-[8rem]"
                                    size="sm"
                                    allowClear={false}
                                    aria-label="状态"
                                    placeholder="全部状态"
                                />
                            </div>
                        </>
                    ) : null}
                </div>
            }
            actions={
                hasActiveFilters ? (
                    <Button
                        type="button"
                        size="xs"
                        variant="ghost"
                        onClick={onClearFilters}
                        title="清除全部筛选条件，保留当前视图与排序"
                    >
                        清除筛选
                    </Button>
                ) : null
            }
        />
    )
}
