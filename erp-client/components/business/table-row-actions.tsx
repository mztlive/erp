"use client"

import * as React from "react"
import Link from "next/link"
import { MoreHorizontalIcon } from "lucide-react"

import { Button } from "@/components/ui/button"
import {
    DropdownMenu,
    DropdownMenuContent,
    DropdownMenuItem,
    DropdownMenuSeparator,
    DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { cn } from "@/lib/utils"

/** 列表操作列的一个动作。页面声明权限、文案和回调，不自己拼按钮尺寸和菜单。 */
export type TableRowAction = {
    /** 落在最终按钮或菜单项上的原生 id。 */
    id: string
    label: React.ReactNode
    onClick?: (event: React.MouseEvent) => void
    /** 有 href 且未禁用时，控件渲染为链接。 */
    href?: string
    disabled?: boolean
    /** 同时作为 title。禁用时再包一层，保证悬停能看到原因。 */
    disabledReason?: string
    hidden?: boolean
    /** 同行还有其它动作时收进菜单底部；仅此一个动作时仍直接露出。 */
    destructive?: boolean
    placement?: "inline" | "menu"
    /** 同行最多一个描边按钮，且只作用于露出的动作。 */
    emphasis?: "outline"
    /** 只挂到非链接的露出按钮，供关闭浮层后还焦。 */
    buttonRef?: React.Ref<HTMLButtonElement>
    /** 进行中状态的指示，不要用来给普通文字按钮加装饰图标。 */
    leading?: React.ReactNode
}

function isMenuBound(action: TableRowAction, soleVisible: boolean) {
    return (
        action.placement === "menu" ||
        (action.destructive === true && !soleVisible)
    )
}

function ActionControl({
    action,
    variant,
}: {
    action: TableRowAction
    variant: "ghost" | "outline"
}) {
    const button =
        action.href && !action.disabled ? (
            <Button
                id={action.id}
                type="button"
                size="xs"
                variant={variant}
                title={action.disabledReason}
                render={<Link href={action.href} />}
                onClick={(event) => {
                    event.stopPropagation()
                    action.onClick?.(event)
                }}
            >
                {action.leading}
                {action.label}
            </Button>
        ) : (
            <Button
                id={action.id}
                type="button"
                size="xs"
                variant={variant}
                ref={action.buttonRef}
                disabled={action.disabled}
                title={action.disabledReason}
                onClick={(event) => {
                    event.stopPropagation()
                    action.onClick?.(event)
                }}
            >
                {action.leading}
                {action.label}
            </Button>
        )
    if (!action.disabled || !action.disabledReason) return button
    return (
        <span title={action.disabledReason} className="inline-flex">
            {button}
        </span>
    )
}

function MenuAction({ action }: { action: TableRowAction }) {
    const item = (
        <DropdownMenuItem
            id={action.id}
            variant={action.destructive ? "destructive" : "default"}
            disabled={action.disabled}
            title={action.disabledReason}
            render={
                action.href && !action.disabled ? (
                    <Link href={action.href} />
                ) : undefined
            }
            onClick={(event) => {
                event.stopPropagation()
                action.onClick?.(event)
            }}
        >
            {action.label}
        </DropdownMenuItem>
    )
    if (!action.disabled || !action.disabledReason) return item
    return (
        <span title={action.disabledReason} className="block">
            {item}
        </span>
    )
}

/**
 * 列表行操作：轻量文字按钮，默认露出 1 个、最多 2 个，其余进横向三点菜单。
 * 破坏性动作在还有其它动作时放到菜单底部，并用分隔线隔开。
 */
export function TableRowActions({
    actions,
    moreId,
    moreLabel,
    maxInline = 2,
    className,
}: {
    actions: readonly TableRowAction[]
    moreId: string
    /** 例如「销售员 更多操作」。 */
    moreLabel: string
    maxInline?: number
    className?: string
}) {
    const visible = actions.filter((action) => !action.hidden)
    const soleVisible = visible.length === 1
    const inlineCandidates = visible.filter(
        (action) => !isMenuBound(action, soleVisible),
    )
    const inline = inlineCandidates.slice(0, Math.max(0, maxInline))
    const overflow = inlineCandidates.slice(Math.max(0, maxInline))
    const menu = [
        ...overflow,
        ...visible.filter((action) => isMenuBound(action, soleVisible)),
    ]
    const menuRegular = menu.filter((action) => !action.destructive)
    const menuDestructive = menu.filter((action) => action.destructive)
    const outlineId = inline.find((action) => action.emphasis === "outline")?.id

    return (
        <div
            data-slot="table-row-actions"
            className={cn(
                "flex flex-nowrap items-center justify-end gap-1",
                className,
            )}
        >
            {inline.map((action) => (
                <ActionControl
                    key={action.id}
                    action={action}
                    variant={action.id === outlineId ? "outline" : "ghost"}
                />
            ))}
            {menu.length > 0 ? (
                <DropdownMenu>
                    <DropdownMenuTrigger
                        id={moreId}
                        render={
                            <Button
                                type="button"
                                size="icon-xs"
                                variant="ghost"
                                aria-label={moreLabel}
                                onClick={(event) => event.stopPropagation()}
                            />
                        }
                    >
                        <MoreHorizontalIcon aria-hidden="true" />
                    </DropdownMenuTrigger>
                    <DropdownMenuContent align="end">
                        {menuRegular.map((action) => (
                            <MenuAction key={action.id} action={action} />
                        ))}
                        {menuRegular.length > 0 &&
                        menuDestructive.length > 0 ? (
                            <DropdownMenuSeparator />
                        ) : null}
                        {menuDestructive.map((action) => (
                            <MenuAction key={action.id} action={action} />
                        ))}
                    </DropdownMenuContent>
                </DropdownMenu>
            ) : null}
        </div>
    )
}
