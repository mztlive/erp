"use client"

import * as React from "react"
import { DownloadIcon, SearchIcon, XIcon } from "lucide-react"

import { ListToolbar } from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { OptionCombobox } from "@/components/business"
import {
    AuditAdvancedFilters,
    type DebouncedAuditFilters,
} from "@/features/access-audit/components/toolbar-audit-advanced-filters"
import { AuditFilters } from "@/features/access-audit/components/toolbar-audit-filters"
import { ConfigFilters } from "@/features/access-audit/components/toolbar-config-filters"

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
                !isAudit ? (
                    <ConfigFilters
                        org={org}
                        status={status}
                        risk={risk}
                        orgOptions={orgOptions}
                        patchFilterUrl={patchFilterUrl}
                    />
                ) : (
                    <AuditFilters
                        fromParam={fromParam}
                        toParam={toParam}
                        action={action}
                        patchFilterUrl={patchFilterUrl}
                    />
                )
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
                        <AuditAdvancedFilters
                            advancedAuditActive={advancedAuditActive}
                            debouncedFilters={debouncedFilters}
                            setDebouncedFilters={setDebouncedFilters}
                            actorId={actorId}
                            traceId={traceId}
                            objectType={objectType}
                            objectId={objectId}
                            patchFilterUrl={patchFilterUrl}
                        />
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
