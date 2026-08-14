"use client"

import * as React from "react"
import { SearchIcon } from "lucide-react"

import { ListToolbar, surfacePanelClassName } from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import type { MallSyncViewName } from "@/features/mall-sync/types"
import { VIEW_LABEL } from "@/features/mall-sync/types"
import { parseView, VIEWS } from "@/features/mall-sync/lib/presentation"

type MallSyncViewToolbarProps = {
    view: MallSyncViewName
    searchInput: string
    searchInputRef: React.RefObject<HTMLInputElement | null>
    onSearchChange: (value: string) => void
    hasActiveFilters: boolean
    onClearFilters: () => void
    onViewChange: (next: MallSyncViewName) => void
}

export function MallSyncViewToolbar({
    view,
    searchInput,
    searchInputRef,
    onSearchChange,
    hasActiveFilters,
    onClearFilters,
    onViewChange,
}: MallSyncViewToolbarProps) {
    return (
        <div
            className={`${surfacePanelClassName} sticky top-0 z-10 space-y-2.5 px-3 py-2.5`}
        >
            <Tabs
                value={view}
                onValueChange={(v) => {
                    onViewChange(parseView(v))
                }}
            >
                <TabsList
                    variant="line"
                    className="w-full justify-start overflow-x-auto"
                >
                    {VIEWS.map((v) => (
                        <TabsTrigger key={v} value={v}>
                            {VIEW_LABEL[v]}
                        </TabsTrigger>
                    ))}
                </TabsList>
            </Tabs>
            <ListToolbar
                aria-label="商城同步筛选"
                search={
                    <InputGroup>
                        <InputGroupAddon>
                            <SearchIcon aria-hidden="true" />
                        </InputGroupAddon>
                        <InputGroupInput
                            ref={searchInputRef}
                            placeholder={
                                view === "snapshots" || view === "mapping"
                                    ? "商城单号 / ERP 单号 / 任务号"
                                    : view === "jobs"
                                      ? "任务号"
                                      : "搜索仅对来源数据、同步任务与映射任务生效"
                            }
                            value={searchInput}
                            onChange={(e) => onSearchChange(e.target.value)}
                            aria-label="搜索"
                        />
                    </InputGroup>
                }
                actions={
                    hasActiveFilters ? (
                        <Button
                            type="button"
                            variant="ghost"
                            size="sm"
                            onClick={onClearFilters}
                        >
                            清除筛选
                        </Button>
                    ) : null
                }
            />
        </div>
    )
}
