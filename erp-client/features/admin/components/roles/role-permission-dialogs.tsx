"use client"

import * as React from "react"
import { CopyIcon } from "lucide-react"

import { OptionCombobox } from "@/components/business"
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
    PERMISSION_BY_CODE,
    permissionLabel,
    selectedItemsByGroup,
} from "@/features/admin/lib/permission-catalog"
import { permissionChanges } from "@/features/admin/lib/role-workbench"
import { hasPermission } from "@/lib/permissions"
import type { AdminRole } from "@/features/admin/types"

export function CopyRolePermissions({
    roles,
    disabled,
    currentCount,
    onCopy,
}: {
    roles: readonly AdminRole[]
    disabled: boolean
    currentCount: number
    onCopy: (codes: string[]) => void
}) {
    const [open, setOpen] = React.useState(false)
    const [sourceId, setSourceId] = React.useState<string | null>(null)
    const sources = roles.filter((role) => !role.permissions.includes("*:*"))
    const source = sources.find((role) => role.id === sourceId)
    const copyable = [
        ...new Set(
            source?.permissions.filter((code) =>
                PERMISSION_BY_CODE.has(code),
            ) ?? [],
        ),
    ]
    if (roles.length === 0) return null
    return (
        <>
            <Button
                id="governance-admin-role-form-copy-open"
                type="button"
                variant="ghost"
                size="sm"
                disabled={disabled || sources.length === 0}
                onClick={() => setOpen(true)}
            >
                <CopyIcon className="size-4" aria-hidden="true" />
                从其他角色复制
            </Button>
            <Dialog open={open} onOpenChange={setOpen}>
                <DialogContent closeButtonId="governance-admin-role-form-copy-close">
                    <DialogHeader>
                        <DialogTitle>复制角色权限</DialogTitle>
                        <DialogDescription>
                            复制将替换当前勾选的 {currentCount}{" "}
                            项权限，保存角色后才生效。全权角色不能作为复制来源。
                        </DialogDescription>
                    </DialogHeader>
                    <OptionCombobox
                        id="governance-admin-role-form-copy-source"
                        value={sourceId}
                        onValueChange={setSourceId}
                        options={sources.map((role) => ({
                            value: role.id,
                            label: role.name,
                        }))}
                        placeholder="选择来源角色"
                        aria-label="复制权限的来源角色"
                    />
                    {source && (
                        <p className="text-sm">
                            将使用「{source.name}」的 {copyable.length}{" "}
                            项可配置权限。特殊授权不参与复制，当前角色已有的特殊授权保持不变。
                        </p>
                    )}
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
}: {
    mode: PermissionReviewMode | null
    onClose: () => void
    selected: readonly string[]
    initial: readonly string[]
    preservedCodes: readonly string[]
    name: string
    initialName: string
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
    const titles: Record<PermissionReviewMode, string> = {
        changes: "本次变更",
        selected: "已授权权限",
        dangerous: "已授权的高风险权限",
        preserved: "保留的特殊授权",
    }
    const nameChanged = name.trim() !== initialName.trim()
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
                    <DialogDescription>
                        此处仅供核对。编辑结果在保存角色后生效；数据可见范围仍由数据范围配置决定。
                    </DialogDescription>
                </DialogHeader>
                <div className="max-h-[55dvh] space-y-5 overflow-y-auto">
                    {mode === "changes" && (
                        <>
                            {nameChanged && (
                                <p className="text-sm">
                                    角色名称：{initialName || "未设置"} → {name}
                                </p>
                            )}
                            <PermissionList
                                title={`新增权限（${added.length}）`}
                                codes={added}
                            />
                            <PermissionList
                                title={`移除权限（${removed.length}）`}
                                codes={removed}
                            />
                            {authorizationModeChanged && (
                                <p className="text-sm text-muted-foreground">
                                    授权方式已调整。切换为自选操作后，今后新增的操作需要另行授予。
                                </p>
                            )}
                            {!authorizationModeChanged &&
                                !nameChanged &&
                                added.length === 0 &&
                                removed.length === 0 && (
                                    <p className="text-sm text-muted-foreground">
                                        尚未修改角色。
                                    </p>
                                )}
                        </>
                    )}
                    {mode === "selected" && preservedCodes.includes("*:*") && (
                        <p>该角色拥有全部业务与系统操作权限。</p>
                    )}
                    {mode === "selected" && !preservedCodes.includes("*:*") && (
                        <PermissionList
                            title={`已授权 ${selectedActions.length} 项`}
                            codes={selectedActions}
                        />
                    )}
                    {mode === "dangerous" && (
                        <PermissionList
                            title="请逐项核对"
                            codes={selectedActions.filter(
                                (code) =>
                                    PERMISSION_BY_CODE.get(code)?.dangerous,
                            )}
                        />
                    )}
                    {(mode === "preserved" ||
                        (mode === "selected" &&
                            preservedCodes.length > 0 &&
                            !preservedCodes.includes("*:*"))) && (
                        <>
                            <p className="text-sm text-muted-foreground">
                                以下为当前保留的特殊授权。业务的「全部操作」可返回业务配置切换为「自选操作」；其他特殊授权须由权限管理员核对。
                            </p>
                            <ul className="space-y-2 text-sm">
                                {preservedCodes.map((code) => (
                                    <li key={code}>{permissionLabel(code)}</li>
                                ))}
                            </ul>
                        </>
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

function PermissionList({
    title,
    codes,
}: {
    title: string
    codes: readonly string[]
}) {
    const groups = selectedItemsByGroup(codes)
    return (
        <section className="space-y-3">
            <h3 className="text-sm font-medium">{title}</h3>
            {groups.length === 0 && (
                <p className="text-sm text-muted-foreground">无</p>
            )}
            {groups.map((group) => (
                <div key={group.name} className="border-l-2 border-border pl-3">
                    <p className="mb-1 text-xs font-medium text-muted-foreground">
                        {group.name}
                    </p>
                    <ul className="space-y-1 text-sm">
                        {group.items.map((item) => (
                            <li key={item.code}>
                                {permissionLabel(item.code)}
                            </li>
                        ))}
                    </ul>
                </div>
            ))}
        </section>
    )
}
