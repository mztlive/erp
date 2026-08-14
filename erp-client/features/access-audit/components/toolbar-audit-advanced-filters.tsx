"use client"

import * as React from "react"
import { FilterIcon } from "lucide-react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    InputGroup,
    InputGroupInput,
} from "@/components/ui/input-group"
import {
    Popover,
    PopoverContent,
    PopoverTrigger,
} from "@/components/ui/popover"

type DebouncedAuditFilters = {
    actorId?: string
    traceId?: string
    objectType?: string
    objectId?: string
}

type AuditAdvancedFiltersProps = {
    advancedAuditActive: boolean
    debouncedFilters: DebouncedAuditFilters
    setDebouncedFilters: React.Dispatch<
        React.SetStateAction<DebouncedAuditFilters>
    >
    actorId?: string
    traceId?: string
    objectType?: string
    objectId?: string
    patchFilterUrl: (patch: Record<string, string | null | undefined>) => void
}

function AuditAdvancedFilters({
    advancedAuditActive,
    debouncedFilters,
    setDebouncedFilters,
    actorId,
    traceId,
    objectType,
    objectId,
    patchFilterUrl,
}: AuditAdvancedFiltersProps) {
    return (
        <Popover>
            <PopoverTrigger
                render={
                    <Button type="button" variant="outline" size="sm" />
                }
            >
                <FilterIcon data-icon="inline-start" aria-hidden="true" />
                高级筛选
                {advancedAuditActive ? (
                    <Badge variant="info">已启用</Badge>
                ) : null}
            </PopoverTrigger>
            <PopoverContent align="end" className="w-80 space-y-3">
                <div>
                    <div className="font-medium">高级筛选</div>
                    <p className="mt-1 text-xs text-muted-foreground">
                        操作者、对象与请求追踪号。
                    </p>
                </div>
                <label className="grid gap-1.5 text-sm">
                    <span>操作者</span>
                    <InputGroup>
                        <InputGroupInput
                            value={debouncedFilters.actorId ?? actorId ?? ""}
                            onChange={(e) =>
                                setDebouncedFilters((prev) => ({
                                    ...prev,
                                    actorId: e.target.value,
                                }))
                            }
                            placeholder="操作者姓名或 ID"
                            aria-label="操作者"
                        />
                    </InputGroup>
                </label>
                <label className="grid gap-1.5 text-sm">
                    <span>请求追踪号</span>
                    <InputGroup>
                        <InputGroupInput
                            value={debouncedFilters.traceId ?? traceId ?? ""}
                            onChange={(e) =>
                                setDebouncedFilters((prev) => ({
                                    ...prev,
                                    traceId: e.target.value,
                                }))
                            }
                            placeholder="精确匹配"
                            aria-label="请求追踪号"
                        />
                    </InputGroup>
                </label>
                <label className="grid gap-1.5 text-sm">
                    <span>对象类型</span>
                    <InputGroup>
                        <InputGroupInput
                            value={
                                debouncedFilters.objectType ?? objectType ?? ""
                            }
                            onChange={(e) =>
                                setDebouncedFilters((prev) => ({
                                    ...prev,
                                    objectType: e.target.value,
                                }))
                            }
                            placeholder="如 role / sales_order"
                            aria-label="对象类型"
                        />
                    </InputGroup>
                </label>
                <label className="grid gap-1.5 text-sm">
                    <span>对象编号</span>
                    <InputGroup>
                        <InputGroupInput
                            value={
                                debouncedFilters.objectId ?? objectId ?? ""
                            }
                            onChange={(e) =>
                                setDebouncedFilters((prev) => ({
                                    ...prev,
                                    objectId: e.target.value,
                                }))
                            }
                            placeholder="对象名称或编号"
                            aria-label="对象名称或编号"
                        />
                    </InputGroup>
                </label>
                <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    disabled={!advancedAuditActive}
                    onClick={() => {
                        setDebouncedFilters({})
                        patchFilterUrl({
                            actorId: null,
                            traceId: null,
                            objectType: null,
                            objectId: null,
                        })
                    }}
                >
                    清除高级筛选
                </Button>
            </PopoverContent>
        </Popover>
    )
}

export { AuditAdvancedFilters, type DebouncedAuditFilters }
