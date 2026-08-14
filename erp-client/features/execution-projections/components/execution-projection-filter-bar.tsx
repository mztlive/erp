"use client"

import * as React from "react"
import { SearchIcon } from "lucide-react"

import { ListToolbar, OptionCombobox } from "@/components/business"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import type {
    DeliveryStatus,
    LatencyBand,
    ProjectionSource,
    ReconciliationStatus,
} from "@/features/execution-projections/types"
import {
    DELIVERY_STATUS_LABEL,
    LATENCY_LABEL,
} from "@/features/execution-projections/types"

type ReplaceParams = (patch: Record<string, string | null | undefined>) => void

export function ExecutionProjectionFilterBar({
    replaceParams,
    searchInputRef,
    searchDraft,
    onSearchDraftChange,
    mallId,
    deliveryStatus,
    latency,
    reconciliation,
    source,
    malls,
    total,
}: {
    replaceParams: ReplaceParams
    searchInputRef: React.RefObject<HTMLInputElement | null>
    searchDraft: string
    onSearchDraftChange: (value: string) => void
    mallId: string
    deliveryStatus: string
    latency: LatencyBand | "all"
    reconciliation: ReconciliationStatus | "all"
    source: ProjectionSource | "all"
    malls: Array<{ id: string; name: string }>
    total: number
}) {
    return (
        <ListToolbar
            search={
                <InputGroup className="max-w-sm">
                    <InputGroupAddon>
                        <SearchIcon aria-hidden="true" />
                    </InputGroupAddon>
                    <InputGroupInput
                        ref={searchInputRef}
                        value={searchDraft}
                        onChange={(e) => onSearchDraftChange(e.target.value)}
                        onKeyDown={(e) => {
                            if (e.key === "Enter") {
                                replaceParams({
                                    q: searchDraft.trim() || null,
                                    page: "1",
                                })
                            }
                        }}
                        placeholder="销售单号、客户"
                        aria-label="搜索执行信息"
                    />
                </InputGroup>
            }
            filters={
                <>
                    <OptionCombobox
                        aria-label="目标商城"
                        value={mallId}
                        onValueChange={(v) =>
                            replaceParams({
                                mall: v ?? "all",
                                page: "1",
                            })
                        }
                        options={[
                            { value: "all", label: "全部商城" },
                            ...malls.map((m) => ({
                                value: m.id,
                                label: m.name,
                            })),
                        ]}
                        className="w-[9rem]"
                        size="sm"
                        allowClear={false}
                        placeholder="全部商城"
                    />
                    <OptionCombobox
                        aria-label="接收状态"
                        value={deliveryStatus}
                        onValueChange={(v) =>
                            replaceParams({
                                deliveryStatus: v ?? "all",
                                page: "1",
                            })
                        }
                        options={[
                            { value: "all", label: "全部接收状态" },
                            ...(
                                [
                                    "UNKNOWN",
                                    "FAILED",
                                    "ESCALATED_MANUAL",
                                    "RETRYING",
                                    "SENDING",
                                    "PENDING",
                                    "ACKED",
                                ] as DeliveryStatus[]
                            ).map((s) => ({
                                value: s,
                                label: DELIVERY_STATUS_LABEL[s],
                            })),
                            {
                                value: "UNKNOWN,FAILED,ESCALATED_MANUAL",
                                label: "未知+失败+转人工",
                            },
                        ]}
                        className="w-[11rem]"
                        size="sm"
                        allowClear={false}
                        placeholder="全部接收状态"
                    />
                    <OptionCombobox
                        aria-label="等待时长分组"
                        value={latency}
                        onValueChange={(v) =>
                            replaceParams({
                                latency: v ?? "all",
                                page: "1",
                            })
                        }
                        options={[
                            {
                                value: "all",
                                label: "等待时长：全部",
                            },
                            ...(
                                Object.keys(LATENCY_LABEL) as LatencyBand[]
                            ).map((k) => ({
                                value: k,
                                label: LATENCY_LABEL[k],
                            })),
                        ]}
                        className="w-[9rem]"
                        size="sm"
                        allowClear={false}
                        placeholder="等待时长：全部"
                    />
                </>
            }
            secondary={
                <>
                    <OptionCombobox
                        aria-label="版本差异"
                        value={reconciliation}
                        onValueChange={(v) =>
                            replaceParams({
                                reconciliation: v ?? "all",
                                page: "1",
                            })
                        }
                        options={[
                            { value: "all", label: "对账：全部" },
                            {
                                value: "VERSION_MISMATCH",
                                label: "仅版本差异",
                            },
                            { value: "MATCHED", label: "版本一致" },
                        ]}
                        className="w-[9rem]"
                        size="sm"
                        allowClear={false}
                        placeholder="对账：全部"
                    />
                    <OptionCombobox
                        aria-label="数据来源"
                        value={source}
                        onValueChange={(v) =>
                            replaceParams({
                                source: v ?? "all",
                                page: "1",
                            })
                        }
                        options={[
                            { value: "all", label: "来源：全部" },
                            {
                                value: "ERP_SALES_REVISION",
                                label: "ERP 销售版本",
                            },
                            {
                                value: "MIGRATION_BASELINE",
                                label: "迁移基线",
                            },
                        ]}
                        className="w-[10rem]"
                        size="sm"
                        allowClear={false}
                        placeholder="来源：全部"
                    />
                </>
            }
            actions={
                <span className="text-xs text-muted-foreground">
                    <span className="num">{total}</span> 条
                </span>
            }
        />
    )
}
