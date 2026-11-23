"use client"

import * as React from "react"
import { ArrowRightIcon, CopyIcon, ShieldAlertIcon } from "lucide-react"

import { detailWorkspaceStyles, OptionCombobox } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import {
    actionLabel,
    PERMISSION_BY_CODE,
    permissionLabel,
    selectedItemsByGroup,
    summarizePermissions,
} from "@/features/admin/lib/permission-catalog"
import { diffWorkbenches } from "@/features/admin/lib/role-detail-view"
import { permissionChanges } from "@/features/admin/lib/role-workbench"
import { hasPermission } from "@/lib/permissions"
import { roleWorkbench } from "@/lib/role-workbenches"
import type { AdminRole } from "@/features/admin/types"

export function CopyRolePermissions({
    roles,
    disabled,
    currentCount,
    onRefreshRoles,
    onCopy,
}: {
    roles: readonly AdminRole[]
    disabled: boolean
    currentCount: number
    onRefreshRoles: () => Promise<unknown>
    onCopy: (codes: string[]) => void
}) {
    const [open, setOpen] = React.useState(false)
    const [sourceId, setSourceId] = React.useState<string | null>(null)
    const sources = roles.filter((role) => !role.permissions.includes("*:*"))
    const source = sources.find((role) => role.id === sourceId)
    const copyable = React.useMemo(
        () => [
            ...new Set(
                source?.permissions.filter((code) =>
                    PERMISSION_BY_CODE.has(code),
                ) ?? [],
            ),
        ],
        [source],
    )
    const sourceSummary = React.useMemo(
        () => (source ? summarizePermissions(copyable) : null),
        [source, copyable],
    )

    if (roles.length === 0) return null
    return (
        <>
            <Button
                id="governance-admin-role-form-copy-open"
                type="button"
                variant="ghost"
                size="sm"
                className={detailWorkspaceStyles.quietButton}
                disabled={disabled || sources.length === 0}
                onClick={() => setOpen(true)}
            >
                <CopyIcon data-icon="inline-start" aria-hidden="true" />
                从其他角色复制
            </Button>
            <Dialog open={open} onOpenChange={setOpen}>
                <DialogContent closeButtonId="governance-admin-role-form-copy-close">
                    <DialogHeader>
                        <DialogTitle>复制角色权限</DialogTitle>
                        <DialogDescription>
                            复制将替换当前勾选的{" "}
                            <span className="num">{currentCount}</span>{" "}
                            项权限，保存角色后才生效。超级管理员角色不能作为复制来源。
                        </DialogDescription>
                    </DialogHeader>
                    <OptionCombobox
                        id="governance-admin-role-form-copy-source"
                        value={sourceId}
                        onValueChange={setSourceId}
                        onRefresh={onRefreshRoles}
                        options={sources.map((role) => ({
                            value: role.id,
                            label: role.name,
                        }))}
                        placeholder="选择来源角色"
                        aria-label="复制权限的来源角色"
                    />
                    {source && sourceSummary ? (
                        <div className="space-y-2.5 rounded-lg border border-border/70 bg-surface-sunken px-3.5 py-3">
                            <p className="text-body-compact text-foreground">
                                将使用「{source.name}」的{" "}
                                <span className="num font-semibold">
                                    {copyable.length}
                                </span>{" "}
                                项可配置权限（覆盖{" "}
                                <span className="num font-medium">
                                    {sourceSummary.groups.length}
                                </span>{" "}
                                个模块）。
                            </p>
                            {sourceSummary.groups.length > 0 ? (
                                <div className="flex flex-wrap gap-1.5">
                                    {sourceSummary.groups
                                        .slice(0, 8)
                                        .map((group) => (
                                            <Badge
                                                key={group.name}
                                                variant="neutral"
                                                className="num"
                                            >
                                                {group.name} {group.count}
                                            </Badge>
                                        ))}
                                    {sourceSummary.groups.length > 8 ? (
                                        <span className="self-center text-xs text-muted-foreground">
                                            等 {sourceSummary.groups.length}{" "}
                                            个模块
                                        </span>
                                    ) : null}
                                </div>
                            ) : null}
                            <p className="text-xs text-muted-foreground">
                                整模块全选授权不参与复制，当前角色已有的整模块全选授权保持不变。
                            </p>
                        </div>
                    ) : null}
                    <DialogFooter>
                        <Button
                            id="governance-admin-role-form-copy-cancel"
                            type="button"
                            variant="outline"
                            onClick={() => setOpen(false)}
                        >
                            取消
                        </Button>
                        <Button
                            id="governance-admin-role-form-copy"
                            type="button"
                            disabled={!source || disabled}
                            onClick={() => {
                                onCopy(copyable)
                                setOpen(false)
                            }}
                        >
                            替换当前勾选
                        </Button>
                    </DialogFooter>
                </DialogContent>
            </Dialog>
        </>
    )
}

export type PermissionReviewMode =
    | "changes"
    | "selected"
    | "dangerous"
    | "preserved"

export function RolePermissionReview({
    mode,
    onClose,
    selected,
    initial,
    preservedCodes,
    name,
    initialName,
    workbenches = [],
    initialWorkbenches = [],
}: {
    mode: PermissionReviewMode | null
    onClose: () => void
    selected: readonly string[]
    initial: readonly string[]
    preservedCodes: readonly string[]
    name: string
    initialName: string
    workbenches?: readonly string[]
    initialWorkbenches?: readonly string[]
}) {
    const changes = permissionChanges(initial, selected)
    const added = changes
        .filter((change) => change.allowed)
        .map((change) => change.code)
    const removed = changes
        .filter((change) => !change.allowed)
        .map((change) => change.code)
    const authorizationModeChanged = [...initial, ...selected].some(
        (code) =>
            code.includes("*") &&
            initial.includes(code) !== selected.includes(code),
    )
    const selectedActions = [...PERMISSION_BY_CODE.keys()].filter((code) =>
        hasPermission(selected, code),
    )
    const workbenchDiff = diffWorkbenches(workbenches, initialWorkbenches)
    const titles: Record<PermissionReviewMode, string> = {
        changes: "本次变更核对",
        selected: "已授权权限明细",
        dangerous: "已授权的高风险权限",
        preserved: "整模块全选授权",
    }
    const nameChanged = name.trim() !== initialName.trim()
    const hasAnyChange =
        nameChanged ||
        workbenchDiff.count > 0 ||
        added.length > 0 ||
        removed.length > 0 ||
        authorizationModeChanged

    return (
        <Dialog
            open={mode !== null}
            onOpenChange={(open) => {
                if (!open) onClose()
            }}
        >
            <DialogContent
                closeButtonId="governance-admin-role-form-review-close"
                className="max-h-[85dvh] sm:max-w-2xl"
            >
                <DialogHeader>
                    <DialogTitle>
                        {mode ? titles[mode] : "权限明细"}
                    </DialogTitle>
                </DialogHeader>
                <div className="max-h-[56dvh] space-y-5 overflow-y-auto pr-1">
                    {mode === "changes" ? (
                        hasAnyChange ? (
                            <>
                                {nameChanged ? (
                                    <section className="space-y-1.5">
                                        <h3 className="font-semibold text-foreground">
                                            角色名称变更
                                        </h3>
                                        <p className="flex flex-wrap items-center gap-2 rounded-md border border-border/60 bg-surface-sunken px-3 py-2 text-body-compact">
                                            <span className="text-muted-foreground">
                                                {initialName || "未设置"}
                                            </span>
                                            <ArrowRightIcon
                                                className="size-3.5 text-muted-foreground"
                                                aria-hidden="true"
                                            />
                                            <span className="font-medium text-foreground">
                                                {name.trim() || "未设置"}
                                            </span>
                                        </p>
                                    </section>
                                ) : null}

                                {workbenchDiff.count > 0 ? (
                                    <section className="space-y-2">
                                        <h3 className="font-semibold text-foreground">
                                            岗位工作台变更
                                        </h3>
                                        <div className="flex flex-wrap gap-1.5">
                                            {workbenchDiff.added.map((id) => (
                                                <Badge
                                                    key={`add-${id}`}
                                                    variant="success"
                                                >
                                                    新增 ·{" "}
                                                    {roleWorkbench(id)?.name ??
                                                        id}
                                                </Badge>
                                            ))}
                                            {workbenchDiff.removed.map((id) => (
                                                <Badge
                                                    key={`rm-${id}`}
                                                    variant="destructive"
                                                >
                                                    移除 ·{" "}
                                                    {roleWorkbench(id)?.name ??
                                                        id}
                                                </Badge>
                                            ))}
                                        </div>
                                    </section>
                                ) : null}

                                {added.length > 0 ? (
                                    <PermissionChipGroups
                                        title="新增权限"
                                        count={added.length}
                                        tone="success"
                                        codes={added}
                                    />
                                ) : null}

                                {removed.length > 0 ? (
                                    <PermissionChipGroups
                                        title="移除权限"
                                        count={removed.length}
                                        tone="destructive"
                                        codes={removed}
                                    />
                                ) : null}

                                {authorizationModeChanged ? (
                                    <p className="text-xs text-muted-foreground">
                                        已切换为按单项操作授权，后续新增功能需手动勾选。
                                    </p>
                                ) : null}
                            </>
                        ) : (
                            <p className="py-4 text-xs text-muted-foreground">
                                尚未修改角色配置。
                            </p>
                        )
                    ) : null}

                    {mode === "selected" && preservedCodes.includes("*:*") ? (
                        <p className="text-body-compact text-foreground">
                            该角色拥有全部业务与系统操作权限。
                        </p>
                    ) : null}

                    {mode === "selected" && !preservedCodes.includes("*:*") ? (
                        <PermissionChipGroups
                            title="已授权操作"
                            count={selectedActions.length}
                            tone="neutral"
                            codes={selectedActions}
                        />
                    ) : null}

                    {mode === "dangerous" ? (
                        <PermissionChipGroups
                            title="请逐项核对高风险操作"
                            count={
                                selectedActions.filter(
                                    (code) =>
                                        PERMISSION_BY_CODE.get(code)?.dangerous,
                                ).length
                            }
                            tone="destructive"
                            codes={selectedActions.filter(
                                (code) =>
                                    PERMISSION_BY_CODE.get(code)?.dangerous,
                            )}
                        />
                    ) : null}

                    {(mode === "preserved" ||
                        (mode === "selected" &&
                            preservedCodes.length > 0 &&
                            !preservedCodes.includes("*:*"))) && (
                        <section className="space-y-2 border-t border-border/60 pt-4">
                            <h3 className="font-semibold text-foreground">
                                整模块全选授权
                            </h3>
                            <p className="text-xs text-muted-foreground">
                                以下为当前保留的整模块全选授权。关闭此窗口后，可在对应模块改按单项操作勾选；跨模块全选授权的操作需由超级管理员调整。
                            </p>
                            <div className="flex flex-wrap gap-1.5">
                                {preservedCodes.map((code) => (
                                    <Badge key={code} variant="info">
                                        {permissionLabel(code)}
                                    </Badge>
                                ))}
                            </div>
                        </section>
                    )}
                </div>
                <DialogFooter>
                    <Button
                        id="governance-admin-role-form-review-done"
                        type="button"
                        variant="outline"
                        onClick={onClose}
                    >
                        关闭
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}

function PermissionChipGroups({
    title,
    count,
    tone,
    codes,
}: {
    title: string
    count: number
    tone: "neutral" | "success" | "destructive"
    codes: readonly string[]
}) {
    const groups = selectedItemsByGroup(codes)
    const chipClass =
        tone === "success"
            ? "border-success-border/70 bg-success-soft/75 text-success-soft-foreground"
            : tone === "destructive"
              ? "border-destructive-border/70 bg-destructive-soft/75 text-destructive-soft-foreground"
              : "border-border/70 bg-surface-sunken text-foreground"

    return (
        <section className="space-y-3">
            <div className="flex items-center gap-2">
                <h3 className="font-semibold text-foreground">{title}</h3>
                <Badge
                    variant={
                        tone === "success"
                            ? "success"
                            : tone === "destructive"
                              ? "destructive"
                              : "secondary"
                    }
                    className="num"
                >
                    {count} 项
                </Badge>
            </div>
            {groups.length === 0 ? (
                <p className="text-xs text-muted-foreground">无</p>
            ) : (
                <div className="divide-y divide-border/60 border-y border-border/60">
                    {groups.map((group) => (
                        <div
                            key={group.name}
                            className="grid gap-x-4 gap-y-2 py-3 sm:grid-cols-[9rem_minmax(0,1fr)] sm:items-baseline"
                        >
                            <p className="text-xs font-medium text-muted-foreground">
                                {group.name}
                                <span className="num ml-1 opacity-75">
                                    ({group.items.length})
                                </span>
                            </p>
                            <ul className="flex min-w-0 flex-wrap gap-1.5">
                                {group.items.map((item) => (
                                    <li key={item.code} className="min-w-0">
                                        <span
                                            title={item.description}
                                            className={`inline-flex max-w-full items-center gap-1 rounded-md border px-2 py-0.5 text-xs ${chipClass}`}
                                        >
                                            <span className="truncate">
                                                {permissionLabel(
                                                    item.code,
                                                ).startsWith(`${group.name} · `)
                                                    ? actionLabel(item.action)
                                                    : permissionLabel(
                                                          item.code,
                                                      )}
                                            </span>
                                            {item.dangerous &&
                                            tone !== "destructive" ? (
                                                <ShieldAlertIcon
                                                    className="size-3 shrink-0 text-destructive"
                                                    aria-hidden="true"
                                                />
                                            ) : null}
                                        </span>
                                    </li>
                                ))}
                            </ul>
                        </div>
                    ))}
                </div>
            )}
        </section>
    )
}
