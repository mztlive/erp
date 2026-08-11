"use client"

import * as React from "react"
import { DownloadIcon, FilterIcon, SearchIcon, XIcon } from "lucide-react"

import { ListToolbar, OptionCombobox } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import {
    InputGroup,
    InputGroupAddon,
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

type AccessListToolbarProps = {
    isAudit: boolean
    searchInput: string
    searchInputRef: React.RefObject<HTMLInputElement | null>
    setSearchInput: (value: string) => void
    org?: string
    status?: string
    risk?: string
    orgOptions: { value: string; label: string }[]
    fromParam?: string
    toParam?: string
    action?: string
    resultFilter?: string
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
    hasActiveFilters: boolean
    clearFilters: () => void
    exportBlocked: boolean
    exportBlocker?: { message: string }
    handleExport: () => void
}

function AccessListToolbar({
    isAudit,
    searchInput,
    searchInputRef,
    setSearchInput,
    org,
    status,
    risk,
    orgOptions,
    fromParam,
    toParam,
    action,
    resultFilter,
    advancedAuditActive,
    debouncedFilters,
    setDebouncedFilters,
    actorId,
    traceId,
    objectType,
    objectId,
    patchFilterUrl,
    hasActiveFilters,
    clearFilters,
    exportBlocked,
    exportBlocker,
    handleExport,
}: AccessListToolbarProps) {
    return (
        <ListToolbar
            search={
                <InputGroup>
                    <InputGroupAddon>
                        <SearchIcon aria-hidden="true" />
                    </InputGroupAddon>
                    <InputGroupInput
                        ref={searchInputRef}
                        value={searchInput}
                        onChange={(e) => setSearchInput(e.target.value)}
                        placeholder={
                            isAudit
                                ? "操作者、动作、对象、追踪号"
                                : "角色代码/名称、用户账号"
                        }
                        aria-label="搜索"
                    />
                </InputGroup>
            }
            filters={
                <>
                    {!isAudit ? (
                        <>
                            <OptionCombobox
                                value={org ?? "all"}
                                onValueChange={(v) =>
                                    patchFilterUrl({
                                        org:
                                            (v ?? "all") === "all"
                                                ? null
                                                : (v ?? "all"),
                                    })
                                }
                                options={[
                                    { value: "all", label: "全部组织" },
                                    ...orgOptions,
                                ]}
                                className="w-[9rem]"
                                size="sm"
                                allowClear={false}
                                aria-label="组织"
                                placeholder="全部组织"
                            />
                            <OptionCombobox
                                value={status ?? "all"}
                                onValueChange={(v) =>
                                    patchFilterUrl({
                                        status:
                                            (v ?? "all") === "all"
                                                ? null
                                                : (v ?? "all"),
                                    })
                                }
                                options={[
                                    { value: "all", label: "全部状态" },
                                    { value: "enabled", label: "启用" },
                                    { value: "disabled", label: "停用" },
                                ]}
                                className="w-[8rem]"
                                size="sm"
                                allowClear={false}
                                aria-label="状态"
                                placeholder="全部状态"
                            />
                            <OptionCombobox
                                value={risk ?? "all"}
                                onValueChange={(v) =>
                                    patchFilterUrl({
                                        risk:
                                            (v ?? "all") === "all"
                                                ? null
                                                : (v ?? "all"),
                                    })
                                }
                                options={[
                                    { value: "all", label: "全部风险" },
                                    {
                                        value: "HIGH_PRIVILEGE",
                                        label: "高权限",
                                    },
                                    {
                                        value: "EMPTY_SCOPE",
                                        label: "空数据范围",
                                    },
                                    {
                                        value: "EXPIRING_SOON",
                                        label: "即将过期",
                                    },
                                    {
                                        value: "ACCESS_ADMIN",
                                        label: "权限管理",
                                    },
                                ]}
                                className="w-[9rem]"
                                size="sm"
                                allowClear={false}
                                aria-label="权限风险"
                                placeholder="全部风险"
                            />
                        </>
                    ) : (
                        <>
                            <label className="flex items-center gap-1.5 text-xs text-muted-foreground">
                                起始
                                <Input
                                    type="date"
                                    className="h-8 w-36"
                                    value={fromParam ?? ""}
                                    onChange={(e) =>
                                        patchFilterUrl({
                                            from: e.target.value || null,
                                        })
                                    }
                                    aria-label="审计起始日期"
                                />
                            </label>
                            <label className="flex items-center gap-1.5 text-xs text-muted-foreground">
                                截止
                                <Input
                                    type="date"
                                    className="h-8 w-36"
                                    value={toParam ?? ""}
                                    min={fromParam}
                                    onChange={(e) =>
                                        patchFilterUrl({
                                            to: e.target.value || null,
                                        })
                                    }
                                    aria-label="审计截止日期"
                                />
                            </label>
                            <OptionCombobox
                                value={action ?? "all"}
                                onValueChange={(v) =>
                                    patchFilterUrl({
                                        action:
                                            (v ?? "all") === "all"
                                                ? null
                                                : (v ?? "all"),
                                    })
                                }
                                options={[
                                    { value: "all", label: "全部动作" },
                                    {
                                        value: "UPDATE_ROLE_PERMISSIONS",
                                        label: "修改模块权限",
                                    },
                                    {
                                        value: "EMERGENCY_REVOKE_USER_ROLE",
                                        label: "紧急撤权",
                                    },
                                    {
                                        value: "UPDATE_FIELD_POLICY",
                                        label: "修改字段策略",
                                    },
                                    {
                                        value: "MANAGE_DATA_SCOPE",
                                        label: "修改数据范围",
                                    },
                                    { value: "QUERY_AUDIT", label: "查询审计" },
                                    {
                                        value: "OPEN_SUPPLIER",
                                        label: "打开供应商",
                                    },
                                    {
                                        value: "EXPORT_RECEIVABLE",
                                        label: "导出应收明细",
                                    },
                                    {
                                        value: "CREATE_ADJUSTMENT",
                                        label: "创建库存调整",
                                    },
                                    {
                                        value: "VIEW_CUSTOMER_SENSITIVE",
                                        label: "短时揭示敏感字段",
                                    },
                                    {
                                        value: "PERMISSION_VERSION_BUMP",
                                        label: "权限版本推进",
                                    },
                                ]}
                                className="w-[10rem]"
                                size="sm"
                                allowClear={false}
                                aria-label="动作"
                                placeholder="全部动作"
                            />
                        </>
                    )}
                </>
            }
            secondary={
                isAudit ? (
                    <>
                        <OptionCombobox
                            value={resultFilter ?? "all"}
                            onValueChange={(v) =>
                                patchFilterUrl({
                                    result:
                                        (v ?? "all") === "all"
                                            ? null
                                            : (v ?? "all"),
                                })
                            }
                            options={[
                                { value: "all", label: "全部结果" },
                                { value: "SUCCESS", label: "成功" },
                                { value: "DENIED", label: "拒绝" },
                                { value: "FAILED", label: "失败" },
                                { value: "UNKNOWN", label: "未知" },
                            ]}
                            className="w-[8rem]"
                            size="sm"
                            allowClear={false}
                            aria-label="结果"
                            placeholder="全部结果"
                        />
                        <Popover>
                            <PopoverTrigger
                                render={
                                    <Button
                                        type="button"
                                        variant="outline"
                                        size="sm"
                                    />
                                }
                            >
                                <FilterIcon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                />
                                高级筛选
                                {advancedAuditActive ? (
                                    <Badge variant="info">已启用</Badge>
                                ) : null}
                            </PopoverTrigger>
                            <PopoverContent
                                align="end"
                                className="w-80 space-y-3"
                            >
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
                                            value={
                                                debouncedFilters.actorId ??
                                                actorId ??
                                                ""
                                            }
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
                                            value={
                                                debouncedFilters.traceId ??
                                                traceId ??
                                                ""
                                            }
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
                                                debouncedFilters.objectType ??
                                                objectType ??
                                                ""
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
                                                debouncedFilters.objectId ??
                                                objectId ??
                                                ""
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
                    </>
                ) : undefined
            }
            actions={
                <>
                    {hasActiveFilters ? (
                        <Button
                            type="button"
                            size="sm"
                            variant="ghost"
                            onClick={clearFilters}
                        >
                            <XIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            清除筛选
                        </Button>
                    ) : null}
                    <Button
                        type="button"
                        size="sm"
                        variant="ghost"
                        disabled={exportBlocked}
                        title={exportBlocker?.message}
                        onClick={handleExport}
                    >
                        <DownloadIcon
                            data-icon="inline-start"
                            aria-hidden="true"
                        />
                        {isAudit ? "导出审计" : "导出配置"}
                    </Button>
                </>
            }
        />
    )
}

export { AccessListToolbar }
