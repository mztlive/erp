"use client"

import * as React from "react"
import { SearchIcon } from "lucide-react"

import { ListToolbar, OptionCombobox } from "@/components/business"
import { Button } from "@/components/ui/button"
import { DatePicker } from "@/components/ui/date-picker"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { SupplierSearchCombobox } from "@/features/entity-selectors"
import type { SettlementsUrlState } from "@/features/supplier-settlements/lib/url-state"
import {
    DIFF_TYPE_LABEL,
    STATUS_LABEL,
} from "@/features/supplier-settlements/types"

export function SettlementListToolbar({
    urlState,
    patchUrl,
    total,
    hasActiveFilters,
    onClearFilters,
}: {
    urlState: SettlementsUrlState
    patchUrl: (patch: Partial<SettlementsUrlState>) => void
    total: number
    hasActiveFilters: boolean
    onClearFilters: () => void
}) {
    const [searchDraft, setSearchDraft] = React.useState(urlState.q ?? "")

    React.useEffect(() => {
        setSearchDraft(urlState.q ?? "")
    }, [urlState.q])

    const commitSearch = React.useCallback(() => {
        patchUrl({ q: searchDraft.trim() || undefined, page: 1 })
    }, [patchUrl, searchDraft])

    return (
        <ListToolbar
            search={
                <div className="flex items-center gap-2">
                    <InputGroup>
                        <InputGroupAddon>
                            <SearchIcon aria-hidden="true" />
                        </InputGroupAddon>
                        <InputGroupInput
                            value={searchDraft}
                            onChange={(e) => setSearchDraft(e.target.value)}
                            onKeyDown={(e) => {
                                if (e.key === "Enter") {
                                    commitSearch()
                                }
                            }}
                            placeholder="结算单号、外部账单号、供应商"
                            aria-label="搜索结算单"
                            data-slot="settlement-list-search"
                        />
                    </InputGroup>
                    <Button
                        type="button"
                        size="sm"
                        variant="secondary"
                        onClick={commitSearch}
                    >
                        搜索
                    </Button>
                </div>
            }
            filters={
                <>
                    <SupplierSearchCombobox
                        value={urlState.supplierId || undefined}
                        onValueChange={(id) =>
                            patchUrl({
                                supplierId: id || undefined,
                                page: 1,
                            })
                        }
                        purpose="filter"
                        className="w-[12rem]"
                        aria-label="供应商"
                        placeholder="全部供应商"
                    />
                    <OptionCombobox
                        value={urlState.status || null}
                        onValueChange={(v) =>
                            patchUrl({
                                status: v || undefined,
                                page: 1,
                            })
                        }
                        options={[
                            { value: "", label: "全部状态" },
                            ...(
                                Object.keys(STATUS_LABEL) as Array<
                                    keyof typeof STATUS_LABEL
                                >
                            ).map((k) => ({
                                value: k,
                                label: STATUS_LABEL[k],
                            })),
                        ]}
                        className="w-[9rem]"
                        size="sm"
                        aria-label="状态"
                        allowClear={false}
                    />
                    <OptionCombobox
                        value={urlState.differenceType || null}
                        onValueChange={(v) =>
                            patchUrl({
                                differenceType: (v ||
                                    undefined) as SettlementsUrlState["differenceType"],
                                page: 1,
                            })
                        }
                        options={[
                            { value: "", label: "全部差异" },
                            ...(
                                Object.keys(DIFF_TYPE_LABEL) as Array<
                                    keyof typeof DIFF_TYPE_LABEL
                                >
                            ).map((k) => ({
                                value: k,
                                label: DIFF_TYPE_LABEL[k],
                            })),
                        ]}
                        className="w-[9rem]"
                        size="sm"
                        aria-label="差异类型"
                        allowClear={false}
                    />
                </>
            }
            secondary={
                <>
                    <label className="flex items-center gap-1 text-xs text-muted-foreground">
                        期间自
                        <DatePicker
                            className="w-[9rem]"
                            value={urlState.periodFrom || undefined}
                            onValueChange={(next) =>
                                patchUrl({
                                    periodFrom: next || undefined,
                                    page: 1,
                                })
                            }
                        />
                    </label>
                    <label className="flex items-center gap-1 text-xs text-muted-foreground">
                        至
                        <DatePicker
                            className="w-[9rem]"
                            value={urlState.periodTo || undefined}
                            onValueChange={(next) =>
                                patchUrl({
                                    periodTo: next || undefined,
                                    page: 1,
                                })
                            }
                        />
                    </label>
                </>
            }
            actions={
                <div className="flex items-center gap-2">
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
                </div>
            }
        />
    )
}
