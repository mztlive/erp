"use client"

import * as React from "react"
import { ChevronDownIcon, FilterIcon, SearchIcon } from "lucide-react"

import { ListToolbar } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"

export function SalesOrdersListFilterBar(props: {
    searchDraft: string
    onSearchDraftChange: (value: string) => void
    onSubmit: () => void
    filterPanelOpen: boolean
    onToggleFilterPanel: () => void
    hasStructuredFilters: boolean
    total: number
    filtersActive: boolean
    onClearFilters: () => void
    filterPanel: React.ReactNode
}) {
    const {
        searchDraft,
        onSearchDraftChange,
        onSubmit,
        filterPanelOpen,
        onToggleFilterPanel,
        hasStructuredFilters,
        total,
        filtersActive,
        onClearFilters,
        filterPanel,
    } = props

    return (
        <form
            onSubmit={(event) => {
                event.preventDefault()
                onSubmit()
            }}
        >
            <ListToolbar
                search={
                    <InputGroup>
                        <InputGroupAddon>
                            <SearchIcon aria-hidden="true" />
                        </InputGroupAddon>
                        <InputGroupInput
                            data-slot="so-list-search"
                            value={searchDraft}
                            onChange={(event) => {
                                onSearchDraftChange(event.target.value)
                            }}
                            placeholder="销售单号"
                            aria-label="搜索销售单号"
                        />
                    </InputGroup>
                }
                filters={
                    <>
                        {!filterPanelOpen ? (
                            <Button type="submit" size="sm">
                                <SearchIcon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                />
                                搜索
                            </Button>
                        ) : null}
                        <Button
                            type="button"
                            variant="outline"
                            size="sm"
                            aria-expanded={filterPanelOpen}
                            aria-controls="sales-order-filter-panel"
                            onClick={onToggleFilterPanel}
                        >
                            <FilterIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            高级筛选
                            {hasStructuredFilters ? (
                                <Badge variant="info">已启用</Badge>
                            ) : null}
                            <ChevronDownIcon
                                data-icon="inline-end"
                                aria-hidden="true"
                                className={
                                    filterPanelOpen
                                        ? "rotate-180 transition-transform"
                                        : "transition-transform"
                                }
                            />
                        </Button>
                    </>
                }
                secondary={filterPanelOpen ? filterPanel : undefined}
                actions={
                    <div className="flex items-center gap-2 text-xs text-muted-foreground">
                        <span aria-live="polite">
                            共 {total.toLocaleString("zh-CN")} 条
                        </span>
                        <span
                            className="hidden md:inline"
                            aria-hidden="true"
                        >
                            ·
                        </span>
                        <span className="hidden md:inline">
                            / 聚焦搜索 · ↑↓ 选择行 · Enter 打开详情
                        </span>
                        {filtersActive ? (
                            <Button
                                type="button"
                                size="xs"
                                variant="ghost"
                                onClick={onClearFilters}
                            >
                                清除筛选
                            </Button>
                        ) : null}
                    </div>
                }
            />
        </form>
    )
}
