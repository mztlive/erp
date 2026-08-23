"use client"

import * as React from "react"
import { ChevronDownIcon, FilterIcon, SearchIcon } from "lucide-react"

import {
    FilterChip,
    FixedOptionRadioFilter,
    ListToolbar,
    OptionCombobox,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { DateRangePicker } from "@/components/ui/date-picker"
import { Input } from "@/components/ui/input"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { RESULT_FILTER_RADIO_OPTIONS } from "@/features/access-audit/lib/filter-options"
import type {
    AccessFilterDraft,
    AccessFilterKey,
} from "@/features/access-audit/pages/hooks/use-access-list-filters"

export type AccessAppliedChip = Readonly<{
    key: AccessFilterKey
    label: string
}>

type SetState<T> = React.Dispatch<React.SetStateAction<T>>

type AccessListToolbarProps = {
    /** 审计侧提供结构化筛选面板；角色 / 用户授权侧只有关键词。 */
    isAudit: boolean
    searchInputRef: React.RefObject<HTMLInputElement | null>
    searchDraft: string
    setSearchDraft: (value: string) => void
    panelOpen: boolean
    setPanelOpen: SetState<boolean>
    hasStructuredFilters: boolean
    appliedChips: readonly AccessAppliedChip[]
    hasChips: boolean
    removeFilter: (key: AccessFilterKey) => void
    clearAllFilters: () => void
    resetMoreFilters: () => void
    applyFilters: () => void
    filterError: string | null
    draft: AccessFilterDraft
    updateDraft: <Key extends keyof AccessFilterDraft>(
        key: Key,
        value: AccessFilterDraft[Key],
    ) => void
    /** 动作选项：由当前查询结果归纳，保证每个选项都能筛出记录。 */
    actionOptions: readonly { value: string; label: string }[]
}

function AccessListToolbar({
    isAudit,
    searchInputRef,
    searchDraft,
    setSearchDraft,
    panelOpen,
    setPanelOpen,
    hasStructuredFilters,
    appliedChips,
    hasChips,
    removeFilter,
    clearAllFilters,
    resetMoreFilters,
    applyFilters,
    filterError,
    draft,
    updateDraft,
    actionOptions,
}: AccessListToolbarProps) {
    const panelId = React.useId()
    const dateErrorId = React.useId()

    return (
        <form
            onSubmit={(event) => {
                event.preventDefault()
                applyFilters()
            }}
        >
            <ListToolbar
                search={
                    <InputGroup>
                        <InputGroupAddon>
                            <SearchIcon aria-hidden="true" />
                        </InputGroupAddon>
                        <InputGroupInput
                            ref={searchInputRef}
                            value={searchDraft}
                            onChange={(event) =>
                                setSearchDraft(event.target.value)
                            }
                            placeholder={
                                isAudit
                                    ? "操作者、动作、对象、追踪号"
                                    : "角色名称、账号或姓名"
                            }
                            aria-label={
                                isAudit ? "搜索审计事件" : "搜索角色与账号"
                            }
                        />
                    </InputGroup>
                }
                filters={
                    isAudit ? (
                        <Button
                            type="button"
                            variant="outline"
                            aria-expanded={panelOpen}
                            aria-controls={panelId}
                            onClick={() => setPanelOpen((open) => !open)}
                        >
                            <FilterIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            更多筛选
                            {hasStructuredFilters ? (
                                <Badge variant="info">已启用</Badge>
                            ) : null}
                            <ChevronDownIcon
                                data-icon="inline-end"
                                aria-hidden="true"
                                className={
                                    panelOpen
                                        ? "rotate-180 transition-transform"
                                        : "transition-transform"
                                }
                            />
                        </Button>
                    ) : undefined
                }
                secondary={
                    hasChips || (isAudit && panelOpen) ? (
                        <div className="w-full space-y-3">
                            {hasChips ? (
                                <div className="flex flex-wrap items-center gap-2 border-t pt-3">
                                    <span className="text-xs text-muted-foreground">
                                        已筛选
                                    </span>
                                    {appliedChips.map((chip) => (
                                        <FilterChip
                                            key={chip.key}
                                            label={chip.label}
                                            clearLabel={`移除${chip.label}`}
                                            onClear={() =>
                                                removeFilter(chip.key)
                                            }
                                        />
                                    ))}
                                    <Button
                                        type="button"
                                        variant="ghost"
                                        size="xs"
                                        onClick={clearAllFilters}
                                    >
                                        清空全部
                                    </Button>
                                </div>
                            ) : null}
                            {isAudit && panelOpen ? (
                                <div
                                    id={panelId}
                                    className="flex w-full flex-col gap-3 border-t pt-3"
                                    aria-label="审计查询更多筛选条件"
                                >
                                    <FixedOptionRadioFilter
                                        label="结果"
                                        value={draft.result}
                                        onValueChange={(value) =>
                                            updateDraft("result", value)
                                        }
                                        options={RESULT_FILTER_RADIO_OPTIONS}
                                    />
                                    <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-4">
                                        <div className="flex min-w-0 flex-col gap-1.5 text-sm sm:col-span-2">
                                            <span className="text-muted-foreground">
                                                时间范围
                                            </span>
                                            <DateRangePicker
                                                className="w-full"
                                                value={{
                                                    from: draft.from,
                                                    to: draft.to,
                                                }}
                                                onValueChange={(next) => {
                                                    updateDraft(
                                                        "from",
                                                        next?.from ?? "",
                                                    )
                                                    updateDraft(
                                                        "to",
                                                        next?.to ?? "",
                                                    )
                                                }}
                                                placeholder="选择审计时间范围"
                                            />
                                        </div>
                                        <div className="flex min-w-0 flex-col gap-1.5 text-sm">
                                            <span className="text-muted-foreground">
                                                动作
                                            </span>
                                            <OptionCombobox
                                                className="w-full"
                                                value={
                                                    draft.action === "all"
                                                        ? null
                                                        : draft.action
                                                }
                                                onValueChange={(value) =>
                                                    updateDraft(
                                                        "action",
                                                        value ?? "all",
                                                    )
                                                }
                                                options={[...actionOptions]}
                                                placeholder="全部动作"
                                                aria-label="动作"
                                            />
                                        </div>
                                        <div className="flex min-w-0 flex-col gap-1.5 text-sm">
                                            <span className="text-muted-foreground">
                                                操作者
                                            </span>
                                            <Input
                                                className="w-full"
                                                value={draft.actorId}
                                                onChange={(event) =>
                                                    updateDraft(
                                                        "actorId",
                                                        event.target.value,
                                                    )
                                                }
                                                autoComplete="off"
                                                placeholder="操作者姓名或账号"
                                                aria-label="操作者"
                                            />
                                        </div>
                                        <div className="flex min-w-0 flex-col gap-1.5 text-sm">
                                            <span className="text-muted-foreground">
                                                请求追踪号
                                            </span>
                                            <Input
                                                className="w-full"
                                                value={draft.traceId}
                                                onChange={(event) =>
                                                    updateDraft(
                                                        "traceId",
                                                        event.target.value,
                                                    )
                                                }
                                                autoComplete="off"
                                                placeholder="精确匹配"
                                                aria-label="请求追踪号"
                                            />
                                        </div>
                                        <div className="flex min-w-0 flex-col gap-1.5 text-sm">
                                            <span className="text-muted-foreground">
                                                对象编号
                                            </span>
                                            <Input
                                                className="w-full"
                                                value={draft.objectId}
                                                onChange={(event) =>
                                                    updateDraft(
                                                        "objectId",
                                                        event.target.value,
                                                    )
                                                }
                                                autoComplete="off"
                                                placeholder="对象名称或编号"
                                                aria-label="对象编号"
                                            />
                                        </div>
                                    </div>
                                    {filterError ? (
                                        <span
                                            id={dateErrorId}
                                            className="text-xs text-destructive"
                                            role="alert"
                                        >
                                            {filterError}
                                        </span>
                                    ) : null}
                                    <div className="flex flex-col gap-3 border-t pt-3 sm:flex-row sm:items-center sm:justify-between">
                                        <p className="text-xs text-muted-foreground">
                                            将同时应用上方关键词和以下筛选条件；结果也用于导出。
                                        </p>
                                        <div className="flex flex-wrap items-center gap-2 sm:justify-end">
                                            <Button
                                                type="button"
                                                variant="ghost"
                                                onClick={resetMoreFilters}
                                            >
                                                重置更多条件
                                            </Button>
                                            <Button type="submit">
                                                <SearchIcon
                                                    data-icon="inline-start"
                                                    aria-hidden="true"
                                                />
                                                应用全部筛选
                                            </Button>
                                        </div>
                                    </div>
                                </div>
                            ) : null}
                        </div>
                    ) : undefined
                }
            />
        </form>
    )
}

export { AccessListToolbar }
