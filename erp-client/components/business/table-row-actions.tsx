"use client"

import * as React from "react"
import Link from "next/link"
import { MoreHorizontalIcon, type LucideIcon } from "lucide-react"

import { Button } from "@/components/ui/button"
import {
    DropdownMenu,
    DropdownMenuContent,
    DropdownMenuItem,
    DropdownMenuSeparator,
    DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { cn } from "@/lib/utils"
import { tableActionText } from "@/lib/ui-text"

const actionButtonClassName =
    "gap-1.5 rounded-md px-2 text-[13px] leading-5 text-foreground/85 hover:text-foreground [&_svg]:size-3.5 [&_svg]:text-muted-foreground hover:[&_svg]:text-foreground focus-visible:[&_svg]:text-foreground"

/** 列表操作列的一个动作。页面声明权限、文案和回调，不自己拼按钮尺寸和菜单。 */
export type TableRowAction = {
    /** 落在最终按钮或菜单项上的原生 id。 */
    id: string
    label: React.ReactNode
    /** 由业务动作指定语义图标；共享组件统一图标尺寸与颜色。 */
    icon?: LucideIcon
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
    /** 进行中状态指示，存在时替换普通动作图标。 */
    leading?: React.ReactNode
}

function ActionLabel({ action }: { action: TableRowAction }) {
    const Icon = action.icon
    return (
        <>
            {action.leading ? (
                <span aria-hidden="true" className="inline-flex shrink-0">
                    {action.leading}
                </span>
            ) : Icon ? (
                <Icon aria-hidden="true" className="size-3.5" />
            ) : null}
            {action.label}
        </>
    )
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
                size="sm"
                variant={variant}
                className={actionButtonClassName}
                title={action.disabledReason}
                render={<Link href={action.href} />}
                onClick={(event) => {
                    event.stopPropagation()
                    action.onClick?.(event)
                }}
            >
                <ActionLabel action={action} />
            </Button>
        ) : (
            <Button
                id={action.id}
                type="button"
                size="sm"
                variant={variant}
                className={actionButtonClassName}
                ref={action.buttonRef}
                disabled={action.disabled}
                title={action.disabledReason}
                onClick={(event) => {
                    event.stopPropagation()
                    action.onClick?.(event)
                }}
            >
                <ActionLabel action={action} />
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
            className="min-h-8 gap-2 rounded-md text-[13px] [&_svg]:text-muted-foreground data-[variant=destructive]:[&_svg]:text-destructive"
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
            <ActionLabel action={action} />
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
 * 列表行操作：图标文字按钮，最多露出 2 个，其余进带文字的更多菜单。
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
                "flex flex-nowrap items-center justify-end gap-2",
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
                                size="sm"
                                variant="ghost"
                                className={actionButtonClassName}
                                aria-label={moreLabel}
                                onClick={(event) => event.stopPropagation()}
                            />
                        }
                    >
                        <MoreHorizontalIcon
                            aria-hidden="true"
                            className="size-3.5"
                        />
                        {tableActionText.more}
                    </DropdownMenuTrigger>
                    <DropdownMenuContent
                        align="end"
                        className="min-w-44 rounded-lg"
                    >
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
