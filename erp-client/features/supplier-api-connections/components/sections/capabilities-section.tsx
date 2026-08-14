"use client"

import * as React from "react"
import type { ColumnDef } from "@tanstack/react-table"

import {
    BusinessEmptyState,
    BusinessStatusBadge,
    BusinessTableFrame,
    DataTable,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import type {
    CapabilityView,
    ConnectionCenterView,
} from "@/features/supplier-api-connections/types"

export function CapabilitiesSection({
    conn,
    onOpenConfig,
}: {
    conn: ConnectionCenterView
    onOpenConfig: () => void
}) {
    const canConfigure = conn.capabilities.some((capability) =>
        capability.allowedActions?.includes("UPDATE_CAPABILITIES"),
    )
    const configureBlocker = conn.capabilities
        .flatMap((capability) => capability.actionBlockers ?? [])
        .find((blocker) => blocker.action === "UPDATE_CAPABILITIES")
    const columns = React.useMemo<ColumnDef<CapabilityView>[]>(
        () => [
            {
                id: "code",
                accessorFn: (r) => r.capabilityLabel,
                header: "能力",
                meta: { label: "能力", width: "reference" },
                cell: ({ row }) => (
                    <div className="text-sm font-medium">
                        {row.original.capabilityLabel}
                    </div>
                ),
            },
            {
                id: "status",
                header: "能力状态",
                meta: { label: "能力状态", width: "status" },
                cell: ({ row }) => (
                    <BusinessStatusBadge
                        context="list"
                        label={row.original.statusLabel}
                        tone={
                            row.original.status === "ENABLED"
                                ? "success"
                                : "neutral"
                        }
                    />
                ),
            },
            {
                id: "req",
                header: "业务需求确认",
                meta: { label: "业务需求" },
                cell: ({ row }) => (
                    <span className="text-sm">
                        {row.original.businessRequirementLabel}
                    </span>
                ),
            },
            {
                id: "verify",
                header: "验证",
                meta: { label: "验证" },
                cell: ({ row }) => (
                    <span className="text-sm">
                        {row.original.verificationLabel}
                    </span>
                ),
            },
            {
                id: "note",
                header: "边界说明",
                meta: { label: "边界" },
                cell: () => (
                    <span className="text-xs text-muted-foreground">
                        连接级 ≠ 供给级 · 见供应商供给/商品发布
                    </span>
                ),
            },
            {
                id: "actions",
                header: "动作",
                meta: { label: "动作" },
                cell: () => (
                    <span className="text-xs text-muted-foreground">—</span>
                ),
            },
        ],
        [],
    )

    return (
        <div className="space-y-3">
            <Alert>
                <AlertTitle>能力边界</AlertTitle>
                <AlertDescription>
                    下表为<strong>连接级</strong>
                    统一能力声明，不表示每条供给都可用。供给/发布级能力由供应商供给
                    / 商品发布返回。能力启停由系统管理员配置。
                </AlertDescription>
            </Alert>
            {canConfigure || configureBlocker ? (
                <div className="flex justify-end">
                    <Button
                        type="button"
                        size="sm"
                        disabled={!canConfigure}
                        title={configureBlocker?.message}
                        onClick={onOpenConfig}
                    >
                        配置能力
                    </Button>
                </div>
            ) : null}
            <BusinessTableFrame
                title="能力矩阵"
                description="连接级能力 × 状态 × 业务需求 × 验证；不等于商品级可用"
                table={
                    <DataTable
                        data={conn.capabilities}
                        columns={columns}
                        getRowId={(r) => r.capabilityCode}
                        rowCount={conn.capabilities.length}
                        caption="连接能力矩阵"
                        density="compact"
                        layout="flush"
                        showPagination={false}
                        defaultColumnPinning={{ left: ["code"] }}
                        emptyState={
                            <BusinessEmptyState
                                kind="no-data"
                                className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                                title="尚未配置能力"
                                description="可配置能力启停；业务需求与验证状态随后端数据返回。"
                            />
                        }
                    />
                }
            />
        </div>
    )
}
