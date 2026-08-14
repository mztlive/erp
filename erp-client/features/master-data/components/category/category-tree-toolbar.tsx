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
import { masterDataCopy } from "@/features/master-data/lib/copy"

/** 分类树筛选条：搜索、启停切换、展开/收起。 */
export function CategoryTreeToolbar({
    searchInputRef,
    search,
    onSearchChange,
    lifecycleStatus,
    onLifecycleStatusChange,
    onExpandAll,
    onCollapseAll,
}: {
    searchInputRef: React.RefObject<HTMLInputElement | null>
    search: string
    onSearchChange: (value: string) => void
    lifecycleStatus: "enabled" | "disabled" | "all"
    onLifecycleStatusChange: (value: "enabled" | "disabled" | "all") => void
    onExpandAll: () => void
    onCollapseAll: () => void
}) {
    return (
        <div className={`${surfacePanelClassName} px-3 py-2.5`}>
            <ListToolbar
                aria-label="分类树筛选"
                search={
                    <InputGroup>
                        <InputGroupAddon>
                            <SearchIcon aria-hidden />
                        </InputGroupAddon>
                        <InputGroupInput
                            ref={searchInputRef}
                            value={search}
                            onChange={(e) => onSearchChange(e.target.value)}
                            placeholder={masterDataCopy.categoryTreeSearch}
                            aria-label={masterDataCopy.categoryTreeSearch}
                        />
                    </InputGroup>
                }
                filters={
                    <div
                        role="group"
                        aria-label="生命周期"
                        className="inline-flex gap-1"
                    >
                        {(
                            [
                                ["all", "全部"],
                                ["enabled", "启用"],
                                ["disabled", "停用"],
                            ] as const
                        ).map(([value, label]) => (
                            <Button
                                key={value}
                                type="button"
                                size="sm"
                                variant={
                                    lifecycleStatus === value
                                        ? "secondary"
                                        : "ghost"
                                }
                                onClick={() => onLifecycleStatusChange(value)}
                            >
                                {label}
                            </Button>
                        ))}
                    </div>
                }
                actions={
                    <>
                        <Button
                            type="button"
                            size="sm"
                            variant="ghost"
                            onClick={onExpandAll}
                        >
                            {masterDataCopy.categoryExpandAll}
                        </Button>
                        <Button
                            type="button"
                            size="sm"
                            variant="ghost"
                            onClick={onCollapseAll}
                        >
                            {masterDataCopy.categoryCollapseAll}
                        </Button>
                    </>
                }
            />
        </div>
    )
}
