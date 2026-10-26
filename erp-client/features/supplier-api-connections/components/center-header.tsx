"use client"

import { BanIcon, ChevronDownIcon, PlayIcon, RefreshCwIcon } from "lucide-react"

import { DetailPageHeader } from "@/components/business"
import { Button } from "@/components/ui/button"
import type { ConnectionCenterView } from "@/features/supplier-api-connections/types"
import {
    DropdownMenu,
    DropdownMenuContent,
    DropdownMenuItem,
    DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"

/** 连接中心页头：文档头与启停/健康动作。 */
export function CenterHeader({
    conn,
    onBack,
    canRunHealth,
    canEnable,
    canDisable,
    healthPending,
    enablePending,
    disablePending,
    healthBlocker,
    enableBlocker,
    disableBlocker,
    onRunHealth,
    onEnable,
    onDisable,
}: {
    conn: ConnectionCenterView
    onBack: () => void
    canRunHealth: boolean
    canEnable: boolean
    canDisable: boolean
    healthPending: boolean
    enablePending: boolean
    disablePending: boolean
    healthBlocker?: { message: string }
    enableBlocker?: { message: string }
    disableBlocker?: { message: string }
    onRunHealth: () => void
    onEnable: () => void
    onDisable: () => void
}) {
    const showMore = canEnable || enableBlocker || canDisable || disableBlocker

    return (
        <DetailPageHeader
            back={{
                id: "supplier-api-connections-center-back",
                label: "连接列表",
                onClick: onBack,
            }}
            title={conn.supplier.name}
            headingProps={{ className: "text-2xl md:text-3xl" }}
            documentNumber={conn.connectionCode}
            numberLabel="连接代码"
            primaryStatus={{ label: conn.statusLabel, tone: conn.statusTone }}
            meta={conn.connectorTypeLabel}
            primaryAction={
                <div className="flex flex-wrap items-center gap-2">
                    {canRunHealth || healthBlocker ? (
                        <Button
                            id="supplier-api-connections-center-run-health"
                            type="button"
                            disabled={!canRunHealth || healthPending}
                            title={healthBlocker?.message}
                            onClick={onRunHealth}
                        >
                            <RefreshCwIcon aria-hidden="true" />
                            {healthPending ? "正在提交" : "健康检查"}
                        </Button>
                    ) : null}
                    {showMore ? (
                        <DropdownMenu>
                            <DropdownMenuTrigger
                                id="supplier-api-connections-center-more"
                                render={<Button variant="outline" />}
                            >
                                更多
                                <ChevronDownIcon aria-hidden="true" />
                            </DropdownMenuTrigger>
                            <DropdownMenuContent
                                align="end"
                                className="min-w-56"
                            >
                                {canEnable || enableBlocker ? (
                                    <DropdownMenuItem
                                        id="supplier-api-connections-center-enable"
                                        disabled={!canEnable || enablePending}
                                        onClick={onEnable}
                                    >
                                        <PlayIcon aria-hidden="true" />
                                        <span>
                                            启用连接
                                            {enableBlocker ? (
                                                <span className="mt-1 block text-xs text-muted-foreground">
                                                    {enableBlocker.message}
                                                </span>
                                            ) : null}
                                        </span>
                                    </DropdownMenuItem>
                                ) : null}
                                {canDisable || disableBlocker ? (
                                    <DropdownMenuItem
                                        id="supplier-api-connections-center-disable"
                                        variant="destructive"
                                        disabled={!canDisable || disablePending}
                                        onClick={onDisable}
                                    >
                                        <BanIcon aria-hidden="true" />
                                        <span>
                                            停用连接
                                            {disableBlocker ? (
                                                <span className="mt-1 block text-xs text-muted-foreground">
                                                    {disableBlocker.message}
                                                </span>
                                            ) : null}
                                        </span>
                                    </DropdownMenuItem>
                                ) : null}
                            </DropdownMenuContent>
                        </DropdownMenu>
                    ) : null}
                </div>
            }
        />
    )
}
