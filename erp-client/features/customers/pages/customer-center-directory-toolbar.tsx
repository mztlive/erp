"use client"

import { SearchIcon } from "lucide-react"

import { ListToolbar, OptionCombobox } from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupButton,
    InputGroupInput,
} from "@/components/ui/input-group"
import { cn } from "@/lib/utils"
import {
    SCOPE_LABELS,
    SCOPE_ORDER,
} from "@/features/customers/lib/filter-customers"
import type { DirectoryStatus } from "@/features/customers/lib/directory-url"
import type { CustomerScope } from "@/features/customers/types"

/**
 * 客户中心目录工具条：关键词搜索、范围切换、状态筛选与计数/清除。
 * 纯展示组件，筛选变更通过回调交由页面写 URL。
 */
export function CustomerCenterDirectoryToolbar({
    searchDraft,
    onSearchDraftChange,
    onSearch,
    scope,
    onScopeChange,
    status,
    onStatusChange,
    canReadAll,
    total,
    hasActiveFilters,
    onClearFilters,
}: {
    searchDraft: string
    onSearchDraftChange: (value: string) => void
    onSearch: (query: string) => void
    scope: CustomerScope
    onScopeChange: (scope: CustomerScope) => void
    status: DirectoryStatus
    onStatusChange: (status: DirectoryStatus) => void
    canReadAll: boolean
    total: number
    hasActiveFilters: boolean
    onClearFilters: () => void
}) {
    return (
        <ListToolbar
            search={
                <InputGroup className="max-w-md">
                    <InputGroupAddon>
                        <SearchIcon aria-hidden="true" />
                    </InputGroupAddon>
                    <InputGroupInput
                        data-slot="customer-search"
                        value={searchDraft}
                        onChange={(e) => onSearchDraftChange(e.target.value)}
                        onKeyDown={(e) => {
                            if (e.key === "Enter") {
                                onSearch(searchDraft.trim())
                            }
                        }}
                        placeholder="客户名称或客户编号"
                        aria-label="搜索客户"
                    />
                    <InputGroupAddon align="inline-end">
                        <InputGroupButton
                            aria-label="执行客户搜索"
                            onClick={() => onSearch(searchDraft.trim())}
                        >
                            搜索
                        </InputGroupButton>
                    </InputGroupAddon>
                </InputGroup>
            }
            filters={
                <div className="flex flex-wrap items-center gap-2">
                    <div
                        role="group"
                        aria-label="客户范围"
                        className="inline-flex items-center rounded-lg bg-muted p-0.5 ring-1 ring-foreground/10"
                    >
                        {SCOPE_ORDER.filter(
                            (key) =>
                                key !== "all_authorized" || canReadAll,
                        ).map((key) => {
                            const active = scope === key
                            return (
                                <button
                                    key={key}
                                    type="button"
                                    aria-pressed={active}
                                    onClick={() => onScopeChange(key)}
                                    className={cn(
                                        "inline-flex h-7 items-center gap-1.5 rounded-md px-2.5 text-sm transition-all outline-none focus-visible:ring-2 focus-visible:ring-ring",
                                        active
                                            ? "bg-card font-medium text-foreground shadow-sm ring-1 ring-foreground/10"
                                            : "font-normal text-muted-foreground hover:bg-foreground/5 hover:text-foreground",
                                    )}
                                >
                                    {SCOPE_LABELS[key]}
                                </button>
                            )
                        })}
                    </div>
                    <OptionCombobox
                        aria-label="客户状态"
                        value={status}
                        onValueChange={(v) =>
                            onStatusChange(
                                (v ?? "active") as DirectoryStatus,
                            )
                        }
                        options={[
                            { value: "active", label: "启用" },
                            {
                                value: "disabled",
                                label: "停用",
                            },
                            { value: "all", label: "全部状态" },
                        ]}
                        className="w-[7.5rem]"
                        size="sm"
                        allowClear={false}
                        placeholder="客户状态"
                    />
                </div>
            }
            actions={
                <>
                    <span
                        className="text-xs text-muted-foreground"
                        aria-live="polite"
                    >
                        共 {total.toLocaleString("zh-CN")} 条
                    </span>
                    {hasActiveFilters ? (
                        <Button
                            type="button"
                            size="xs"
                            variant="ghost"
                            onClick={onClearFilters}
                        >
                            清除筛选
                        </Button>
                    ) : null}
                </>
            }
        />
    )
}
