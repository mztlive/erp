"use client"

import * as React from "react"
import Link from "next/link"
import { MoreHorizontalIcon, type LucideIcon } from "lucide-react"

import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import { Spinner } from "@/components/ui/spinner"
import {
    DropdownMenu,
    DropdownMenuContent,
    DropdownMenuItem,
    DropdownMenuSeparator,
    DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { cn } from "@/lib/utils"
import { tableActionText } from "@/lib/ui-text"

const inlineActionButtonClassName =
    "h-7 gap-1 rounded-md px-2 text-body-compact font-normal text-primary hover:bg-sidebar-accent/70 hover:text-primary active:bg-sidebar-accent"

const moreActionButtonClassName =
    "text-muted-foreground/80 hover:bg-muted/80 hover:text-foreground aria-expanded:bg-sidebar-accent/70 aria-expanded:text-sidebar-accent-foreground [&_svg]:size-3.5"

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
    /** 当前动作正在执行；禁用重复点击并显示转圈。 */
    loading?: boolean
    /** 同时作为 title。禁用时再包一层，保证悬停能看到原因。 */
    disabledReason?: string
    hidden?: boolean
    /** 默认收进菜单底部；可用 placement="inline" 显式露出。 */
    destructive?: boolean
    placement?: "inline" | "menu"
    /** 同行最多一个描边按钮，且只作用于露出的动作。 */
    emphasis?: "outline"
    /** 只挂到非链接的露出按钮，供关闭浮层后还焦。 */
    buttonRef?: React.Ref<HTMLButtonElement>
    /** 进行中状态指示，存在时替换普通动作图标。 */
    leading?: React.ReactNode
}

function ActionLabel({
    action,
    showIcon = true,
}: {
    action: TableRowAction
    showIcon?: boolean
}) {
    const Icon = action.icon
    return (
        <>
            {action.loading ? null : action.leading ? (
                <span aria-hidden="true" className="inline-flex shrink-0">
                    {action.leading}
                </span>
            ) : showIcon && Icon ? (
                <Icon aria-hidden="true" className="size-3.5" />
            ) : null}
            {action.label}
        </>
    )
}

function isMenuBound(action: TableRowAction, soleVisible: boolean) {
    if (action.placement === "inline") return false
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
    const className = cn(
        inlineActionButtonClassName,
        variant === "outline" && "border-primary/35 hover:border-primary/60",
        action.destructive &&
            action.placement === "inline" &&
            "text-destructive hover:bg-destructive-soft hover:text-destructive",
    )
    const button =
        action.href && !action.disabled && !action.loading ? (
            <Button
                id={action.id}
                type="button"
                size="sm"
                variant={variant}
                className={className}
                title={action.disabledReason}
                render={<Link href={action.href} />}
                onClick={(event) => {
                    event.stopPropagation()
                    action.onClick?.(event)
                }}
            >
                <ActionLabel action={action} showIcon={false} />
            </Button>
        ) : (
            <LoadingButton
                id={action.id}
                type="button"
                size="sm"
                variant={variant}
                className={className}
                ref={action.buttonRef}
                disabled={action.disabled}
                loading={action.loading}
                title={action.disabledReason}
                onClick={(event) => {
                    event.stopPropagation()
                    action.onClick?.(event)
                }}
            >
                <ActionLabel action={action} showIcon={false} />
            </LoadingButton>
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
            disabled={action.disabled || action.loading}
            aria-busy={action.loading || undefined}
            title={action.disabledReason}
            className="min-h-8 gap-2 rounded-md text-[13px] [&_svg]:text-muted-foreground data-[variant=destructive]:[&_svg]:text-destructive"
            render={
                action.href && !action.disabled && !action.loading ? (
                    <Link href={action.href} />
                ) : undefined
            }
            onClick={(event) => {
                event.stopPropagation()
                action.onClick?.(event)
            }}
        >
            {action.loading ? (
                <Spinner
                    aria-hidden="true"
                    role="presentation"
                    aria-label={undefined}
                />
            ) : null}
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
 * 列表行操作：飞书后台清爽蓝字操作链（最多露出 2 个）+ 纯图标「···」更多菜单。
 * 破坏性动作默认放到菜单底部；显式 placement="inline" 时直接露出。
 */
export function TableRowActions({
    actions,
    moreId,
    moreLabel,
    moreIconOnly = true,
    maxInline = 2,
    className,
}: {
    actions: readonly TableRowAction[]
    moreId: string
    /** 例如「销售员 更多操作」。 */
    moreLabel: string
    /** 默认纯图标「···」，保留完整 aria-label 与菜单动作名称；需要带字时可传 false。 */
    moreIconOnly?: boolean
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
                "flex flex-nowrap items-center justify-end gap-0.5",
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
                                size={moreIconOnly ? "icon-sm" : "sm"}
                                variant="ghost"
                                className={cn(
                                    moreActionButtonClassName,
                                    !moreIconOnly && "gap-1 px-2 font-normal",
                                )}
                                aria-label={moreLabel}
                                title={
                                    moreIconOnly
                                        ? tableActionText.more
                                        : undefined
                                }
                                onClick={(event) => event.stopPropagation()}
                            />
                        }
                    >
                        <MoreHorizontalIcon
                            aria-hidden="true"
                            className="size-3.5"
                        />
                        {moreIconOnly ? null : tableActionText.more}
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
