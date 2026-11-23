"use client"

import { CheckIcon, CircleAlertIcon, SparklesIcon } from "lucide-react"

import { detailWorkspaceStyles } from "@/components/business"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { StatusBadge } from "@/components/ui/status-badge"
import { ROLE_WORKBENCHES, type RoleWorkbenchId } from "@/lib/role-workbenches"
import { cn } from "@/lib/utils"
import { evaluateWorkbenchReadiness } from "../../lib/role-detail-view"

/**
 * 岗位工作台选择器：
 * 纯白通栏细线列表（零套娃卡片），实时核对各工作台入口权限是否就绪，
 * 并在勾选工作台缺少入口权限时提供一键补齐入口权限的快捷操作。
 */
export function RoleWorkbenchPicker({
    value,
    permissions,
    disabled = false,
    onChange,
    onGrantPermissions,
}: {
    value: readonly string[]
    permissions: readonly string[]
    disabled?: boolean
    onChange: (next: string[]) => void
    onGrantPermissions?: (codes: readonly string[]) => void
}) {
    const toggle = (workbenchId: RoleWorkbenchId, checked: boolean) => {
        if (disabled) return
        onChange(
            checked
                ? [...new Set([...value, workbenchId])]
                : value.filter((id) => id !== workbenchId),
        )
    }

    return (
        <ul className="divide-y divide-border/60 border-y border-border/60">
            {ROLE_WORKBENCHES.map((item) => {
                const checkboxId = `role-workbench-${item.id.toLowerCase()}`
                const checked = value.includes(item.id)
                const readiness = evaluateWorkbenchReadiness(
                    item.id,
                    permissions,
                )
                return (
                    <li
                        key={item.id}
                        className={cn(
                            "px-3 py-3.5 transition-colors",
                            checked ? "bg-sidebar-accent/35" : "bg-card",
                        )}
                    >
                        <div className="flex flex-wrap items-start gap-3">
                            <Checkbox
                                id={checkboxId}
                                className="mt-0.5"
                                disabled={disabled}
                                checked={checked}
                                onCheckedChange={(next) =>
                                    toggle(item.id, next === true)
                                }
                            />
                            <div className="min-w-0 flex-1 space-y-2">
                                <div className="flex flex-wrap items-center justify-between gap-2">
                                    <div className="min-w-0 space-y-0.5">
                                        <label
                                            htmlFor={checkboxId}
                                            className={cn(
                                                "block font-medium text-foreground",
                                                !disabled && "cursor-pointer",
                                            )}
                                        >
                                            {item.name}
                                        </label>
                                        <p className="text-xs text-muted-foreground">
                                            {item.description}
                                        </p>
                                    </div>
                                    {checked ? (
                                        <div className="flex shrink-0 flex-wrap items-center gap-1.5">
                                            {readiness.ready ? (
                                                <StatusBadge
                                                    tone="success"
                                                    appearance="quiet"
                                                    label="入口权限已就绪"
                                                />
                                            ) : (
                                                <>
                                                    <StatusBadge
                                                        tone="warning"
                                                        label={`缺少 ${readiness.missingCodes.length} 项入口权限`}
                                                    />
                                                    {!disabled &&
                                                    onGrantPermissions ? (
                                                        <Button
                                                            id={`${checkboxId}-grant-missing`}
                                                            type="button"
                                                            variant="ghost"
                                                            size="xs"
                                                            className={
                                                                detailWorkspaceStyles.inlineLinkButton
                                                            }
                                                            onClick={() =>
                                                                onGrantPermissions(
                                                                    readiness.missingCodes,
                                                                )
                                                            }
                                                        >
                                                            <SparklesIcon
                                                                data-icon="inline-start"
                                                                aria-hidden="true"
                                                            />
                                                            一键补齐
                                                        </Button>
                                                    ) : null}
                                                </>
                                            )}
                                        </div>
                                    ) : null}
                                </div>

                                <div className="flex flex-wrap items-center gap-1.5">
                                    <span className="text-xs text-muted-foreground">
                                        入口要求
                                    </span>
                                    {readiness.requirements.map((req) => (
                                        <span
                                            key={req.code}
                                            className={cn(
                                                "inline-flex items-center gap-1 rounded-sm px-2 py-0.5 text-xs",
                                                req.granted
                                                    ? "bg-success-soft/75 text-success-soft-foreground"
                                                    : checked
                                                      ? "bg-warning-soft text-warning-soft-foreground"
                                                      : "bg-muted/70 text-muted-foreground",
                                            )}
                                        >
                                            {req.granted ? (
                                                <CheckIcon
                                                    className="size-3 shrink-0"
                                                    aria-hidden="true"
                                                />
                                            ) : checked ? (
                                                <CircleAlertIcon
                                                    className="size-3 shrink-0"
                                                    aria-hidden="true"
                                                />
                                            ) : null}
                                            <span>{req.label}</span>
                                        </span>
                                    ))}
                                </div>
                            </div>
                        </div>
                    </li>
                )
            })}
        </ul>
    )
}
