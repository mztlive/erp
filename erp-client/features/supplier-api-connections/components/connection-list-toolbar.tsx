"use client"

import { SearchIcon } from "lucide-react"

import { ListToolbar, OptionCombobox } from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { SupplierSearchCombobox } from "@/features/entity-selectors"
import type { ConnectionsUrlState } from "@/features/supplier-api-connections/lib/url-state"
import { CAPABILITY_LABEL } from "@/features/supplier-api-connections/types"

/** 连接列表工具栏：搜索、环境/状态/供应商筛选、能力与清除。 */
export function ConnectionListToolbar({
    urlState,
    patchUrl,
    searchDraft,
    onSearchDraftChange,
    onClearFilters,
}: {
    urlState: ConnectionsUrlState
    patchUrl: (patch: Partial<ConnectionsUrlState>) => void
    searchDraft: string
    onSearchDraftChange: (value: string) => void
    onClearFilters: () => void
}) {
    return (
        <ListToolbar
            search={
                <InputGroup className="max-w-md">
                    <InputGroupAddon>
                        <SearchIcon className="size-4" aria-hidden="true" />
                    </InputGroupAddon>
                    <InputGroupInput
                        placeholder="连接代码、供应商名称"
                        value={searchDraft}
                        onChange={(e) => onSearchDraftChange(e.target.value)}
                        onKeyDown={(e) => {
                            if (e.key === "Enter") {
                                patchUrl({
                                    q: searchDraft.trim() || undefined,
                                    page: 1,
                                })
                            }
                        }}
                        aria-label="搜索连接"
                    />
                </InputGroup>
            }
            filters={
                <>
                    <OptionCombobox
                        value={urlState.environment}
                        onValueChange={(v) => {
                            if (v == null) return
                            patchUrl({
                                environment:
                                    v as ConnectionsUrlState["environment"],
                                page: 1,
                            })
                        }}
                        options={[
                            { value: "ALL", label: "全部环境" },
                            { value: "PRODUCTION", label: "生产" },
                            { value: "STAGING", label: "测试" },
                            { value: "DEVELOPMENT", label: "开发" },
                        ]}
                        className="w-[7.5rem]"
                        size="sm"
                        placeholder="环境"
                        allowClear={false}
                        aria-label="环境"
                    />
                    <OptionCombobox
                        value={urlState.status ?? "default"}
                        onValueChange={(v) => {
                            if (v == null || v === "default") {
                                patchUrl({
                                    status: undefined,
                                    page: 1,
                                })
                            } else if (v === "all") {
                                patchUrl({
                                    status: "ENABLED,DISABLED,FAULTED,PENDING_CONFIG",
                                    page: 1,
                                })
                            } else {
                                patchUrl({ status: v, page: 1 })
                            }
                        }}
                        options={[
                            {
                                value: "default",
                                label: "启用+故障+待配置",
                            },
                            { value: "all", label: "全部状态" },
                            { value: "ENABLED", label: "启用" },
                            { value: "FAULTED", label: "故障" },
                            { value: "DISABLED", label: "停用" },
                            {
                                value: "PENDING_CONFIG",
                                label: "待配置",
                            },
                        ]}
                        className="w-[8rem]"
                        size="sm"
                        placeholder="状态"
                        allowClear={false}
                        aria-label="连接状态"
                    />
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
                        placeholder="全部供应商"
                        aria-label="供应商"
                    />
                </>
            }
            secondary={
                <OptionCombobox
                    value={urlState.capability ?? ""}
                    onValueChange={(v) =>
                        patchUrl({
                            capability: v || undefined,
                            page: 1,
                        })
                    }
                    options={[
                        { value: "", label: "全部能力" },
                        ...(
                            Object.keys(CAPABILITY_LABEL) as Array<
                                keyof typeof CAPABILITY_LABEL
                            >
                        ).map((k) => ({
                            value: k,
                            label: CAPABILITY_LABEL[k],
                        })),
                    ]}
                    className="w-[8rem]"
                    size="sm"
                    placeholder="能力"
                    allowClear={false}
                    aria-label="能力"
                />
            }
            actions={
                <Button
                    type="button"
                    size="sm"
                    variant="ghost"
                    onClick={onClearFilters}
                    title="清除筛选，保留当前环境"
                    aria-label="清除筛选（保留当前环境）"
                >
                    清除筛选
                </Button>
            }
        />
    )
}
