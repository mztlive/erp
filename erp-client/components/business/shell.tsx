"use client"

import * as React from "react"
import { SearchIcon, WrenchIcon, type LucideIcon } from "lucide-react"

import {
    Alert,
    AlertAction,
    AlertDescription,
    AlertTitle,
} from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { Kbd } from "@/components/ui/kbd"
import {
    Sidebar,
    SidebarContent,
    SidebarFooter,
    SidebarHeader,
    SidebarInset,
    SidebarProvider,
    SidebarRail,
    SidebarTrigger,
} from "@/components/ui/sidebar"
import { cn } from "@/lib/utils"

type ButtonProps = React.ComponentProps<typeof Button>
type BadgeProps = React.ComponentProps<typeof Badge>
type SidebarProps = React.ComponentProps<typeof Sidebar>

interface GlobalSearchBase {
    ariaLabel: string
    placeholder?: string
    shortcut?: React.ReactNode
    name?: string
    disabled?: boolean
    onChange?: React.ChangeEventHandler<HTMLInputElement>
    onKeyDown?: React.KeyboardEventHandler<HTMLInputElement>
    onFocus?: React.FocusEventHandler<HTMLInputElement>
    onBlur?: React.FocusEventHandler<HTMLInputElement>
}

export type GlobalSearchConfig = GlobalSearchBase &
    (
        | {
              value: string
              defaultValue?: never
          }
        | {
              value?: never
              defaultValue?: string
          }
    )

export interface GlobalTopbarBadge {
    label: React.ReactNode
    variant?: BadgeProps["variant"]
}

export type GlobalTopbarAction = Omit<ButtonProps, "children" | "size"> & {
    actionKey: React.Key
    label: React.ReactNode
    icon?: LucideIcon
    badge?: GlobalTopbarBadge
}

export type GlobalTopbarProps = Omit<
    React.ComponentProps<"header">,
    "children"
> & {
    search?: GlobalSearchConfig
    leading?: React.ReactNode
    actions?: readonly GlobalTopbarAction[]
    trailing?: React.ReactNode
    showSidebarTrigger?: boolean
    sidebarTriggerLabel?: string
}

function GlobalTopbar({
    search,
    leading,
    actions = [],
    trailing,
    showSidebarTrigger = false,
    sidebarTriggerLabel = "打开导航",
    className,
    ...props
}: GlobalTopbarProps) {
    const showTrailing = Boolean(
        search || leading || actions.length > 0 || trailing,
    )

    return (
        <header
            data-slot="global-topbar"
            className={cn(
                // IDURAR：顶栏操作全部右对齐，无实心底/分割线；仅移动端显示菜单按钮
                "flex h-topbar shrink-0 items-center gap-3 bg-transparent px-4 pt-2 md:px-6",
                className,
            )}
            {...props}
        >
            {showSidebarTrigger ? (
                <SidebarTrigger
                    aria-label={sidebarTriggerLabel}
                    className="md:hidden"
                />
            ) : null}
            {showTrailing ? (
                <div className="ml-auto flex min-w-0 shrink-0 items-center gap-2 sm:gap-3">
                    {leading}
                    {search ? (
                        <InputGroup className="w-[min(100%,16rem)] border-border/40 bg-card/80 shadow-xs sm:w-64 md:w-72">
                            <InputGroupAddon>
                                <SearchIcon aria-hidden="true" />
                            </InputGroupAddon>
                            <InputGroupInput
                                type="search"
                                aria-label={search.ariaLabel}
                                placeholder={search.placeholder}
                                value={search.value}
                                defaultValue={search.defaultValue}
                                name={search.name}
                                disabled={search.disabled}
                                onChange={search.onChange}
                                onKeyDown={search.onKeyDown}
                                onFocus={search.onFocus}
                                onBlur={search.onBlur}
                            />
                            {search.shortcut ? (
                                <InputGroupAddon
                                    align="inline-end"
                                    className="hidden sm:flex"
                                >
                                    <Kbd>{search.shortcut}</Kbd>
                                </InputGroupAddon>
                            ) : null}
                        </InputGroup>
                    ) : null}
                    {actions.map((action) => {
                        const {
                            actionKey,
                            label,
                            icon: Icon,
                            badge,
                            variant = "outline",
                            ...buttonProps
                        } = action

                        return (
                            <Button
                                key={actionKey}
                                variant={variant}
                                size="sm"
                                className="border-border/40 bg-card/80 shadow-xs"
                                {...buttonProps}
                            >
                                {Icon ? (
                                    <Icon
                                        data-icon="inline-start"
                                        aria-hidden="true"
                                    />
                                ) : null}
                                {label}
                                {badge ? (
                                    <Badge variant={badge.variant}>
                                        {badge.label}
                                    </Badge>
                                ) : null}
                            </Button>
                        )
                    })}
                    {trailing}
                </div>
            ) : null}
        </header>
    )
}

export type MaintenanceBannerTone = "info" | "warning" | "destructive"

export type MaintenanceBannerAction = Omit<ButtonProps, "children" | "size"> & {
    label: React.ReactNode
}

export type MaintenanceBannerProps = Omit<
    React.ComponentProps<typeof Alert>,
    "children" | "variant" | "title"
> & {
    title: React.ReactNode
    description?: React.ReactNode
    tone?: MaintenanceBannerTone
    icon?: LucideIcon
    action?: MaintenanceBannerAction
}

function MaintenanceBanner({
    title,
    description,
    tone = "warning",
    icon: Icon = WrenchIcon,
    action,
    className,
    ...props
}: MaintenanceBannerProps) {
    const actionButton = action
        ? (() => {
              const { label, variant = "outline", ...buttonProps } = action

              return (
                  <Button variant={variant} size="xs" {...buttonProps}>
                      {label}
                  </Button>
              )
          })()
        : null

    return (
        <Alert
            variant={tone}
            className={cn("rounded-none border-x-0 border-t-0", className)}
            {...props}
        >
            <Icon aria-hidden="true" />
            <AlertTitle className="lg:whitespace-nowrap">{title}</AlertTitle>
            {description ? (
                <AlertDescription className="lg:col-start-3 lg:row-start-1 lg:pr-20">
                    {description}
                </AlertDescription>
            ) : null}
            {actionButton ? <AlertAction>{actionButton}</AlertAction> : null}
        </Alert>
    )
}

export interface ErpAppShellProps {
    children: React.ReactNode
    sidebarContent: React.ReactNode
    sidebarHeader?: React.ReactNode
    sidebarFooter?: React.ReactNode
    topbar?: React.ReactNode
    maintenanceBanner?: React.ReactNode
    defaultSidebarOpen?: boolean
    sidebarOpen?: boolean
    onSidebarOpenChange?: (open: boolean) => void
    sidebarCollapsible?: SidebarProps["collapsible"]
    sidebarSide?: SidebarProps["side"]
    sidebarVariant?: SidebarProps["variant"]
    showSidebarRail?: boolean
    contentId?: string
    contentLabel?: string
    className?: string
}

function ErpAppShell({
    children,
    sidebarContent,
    sidebarHeader,
    sidebarFooter,
    topbar,
    maintenanceBanner,
    defaultSidebarOpen = true,
    sidebarOpen,
    onSidebarOpenChange,
    sidebarCollapsible = "none",
    sidebarSide = "left",
    sidebarVariant = "sidebar",
    showSidebarRail = false,
    contentId = "main-content",
    contentLabel,
    className,
}: ErpAppShellProps) {
    return (
        <SidebarProvider
            defaultOpen={defaultSidebarOpen}
            open={sidebarOpen}
            onOpenChange={onSidebarOpenChange}
            className={className}
        >
            <Sidebar
                collapsible={sidebarCollapsible}
                side={sidebarSide}
                variant={sidebarVariant}
            >
                {sidebarHeader ? (
                    <SidebarHeader>{sidebarHeader}</SidebarHeader>
                ) : null}
                <SidebarContent>{sidebarContent}</SidebarContent>
                {sidebarFooter ? (
                    <SidebarFooter>{sidebarFooter}</SidebarFooter>
                ) : null}
                {showSidebarRail && sidebarCollapsible !== "none" ? (
                    <SidebarRail />
                ) : null}
            </Sidebar>
            <SidebarInset
                id={contentId}
                aria-label={contentLabel}
                className="min-h-0 min-w-0 overflow-hidden"
            >
                {topbar}
                {maintenanceBanner}
                <div
                    data-slot="erp-shell-content"
                    className="flex min-h-0 flex-1 flex-col overflow-auto"
                >
                    {children}
                </div>
            </SidebarInset>
        </SidebarProvider>
    )
}

export { ErpAppShell, GlobalTopbar, MaintenanceBanner }
