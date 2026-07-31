"use client"

import * as React from "react"
import {
  SearchIcon,
  WrenchIcon,
  type LucideIcon,
} from "lucide-react"

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
import { Separator } from "@/components/ui/separator"
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
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { cn } from "@/lib/utils"

type ButtonProps = React.ComponentProps<typeof Button>
type BadgeProps = React.ComponentProps<typeof Badge>
type SidebarProps = React.ComponentProps<typeof Sidebar>
type TabsProps = React.ComponentProps<typeof Tabs>

interface GlobalSearchBase {
  ariaLabel: string
  placeholder?: string
  shortcut?: React.ReactNode
  name?: string
  disabled?: boolean
  onChange?: React.ChangeEventHandler<HTMLInputElement>
  onKeyDown?: React.KeyboardEventHandler<HTMLInputElement>
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

export type GlobalTopbarAction = Omit<
  ButtonProps,
  "children" | "size"
> & {
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
  showSidebarTrigger = true,
  sidebarTriggerLabel = "切换导航栏",
  className,
  ...props
}: GlobalTopbarProps) {
  const showLeadingSeparator = showSidebarTrigger && Boolean(leading || search)
  const showTrailing = actions.length > 0 || trailing

  return (
    <header
      data-slot="global-topbar"
      className={cn(
        "flex h-topbar shrink-0 items-center gap-3 border-b bg-card px-3",
        className
      )}
      {...props}
    >
      {showSidebarTrigger ? (
        <SidebarTrigger aria-label={sidebarTriggerLabel} />
      ) : null}
      {showLeadingSeparator ? (
        <Separator orientation="vertical" className="h-4" />
      ) : null}
      {leading}
      {search ? (
        <InputGroup className="max-w-96">
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
          />
          {search.shortcut ? (
            <InputGroupAddon align="inline-end">
              <Kbd>{search.shortcut}</Kbd>
            </InputGroupAddon>
          ) : null}
        </InputGroup>
      ) : null}
      {showTrailing ? (
        <div className="ml-auto flex shrink-0 items-center gap-2">
          {actions.map((action) => {
            const {
              actionKey,
              label,
              icon: Icon,
              badge,
              variant = "ghost",
              ...buttonProps
            } = action

            return (
              <Button
                key={actionKey}
                variant={variant}
                size="sm"
                {...buttonProps}
              >
                {Icon ? <Icon data-icon="inline-start" aria-hidden="true" /> : null}
                {label}
                {badge ? (
                  <Badge variant={badge.variant}>{badge.label}</Badge>
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

export interface TaskTabBadge {
  label: React.ReactNode
  variant?: BadgeProps["variant"]
}

export interface TaskTabItem {
  value: string
  label: React.ReactNode
  icon?: LucideIcon
  badge?: TaskTabBadge
  disabled?: boolean
}

export type TaskTabsProps = Pick<
  TabsProps,
  "value" | "defaultValue" | "onValueChange"
> & {
  items: readonly TaskTabItem[]
  ariaLabel?: string
  trailing?: React.ReactNode
  className?: string
}

function TaskTabs({
  items,
  value,
  defaultValue,
  onValueChange,
  ariaLabel = "已打开的任务",
  trailing,
  className,
}: TaskTabsProps) {
  return (
    <div
      data-slot="task-tabs"
      className={cn(
        "flex h-tabs shrink-0 items-center gap-2 border-b bg-surface-sunken px-3",
        className
      )}
    >
      <Tabs
        value={value}
        defaultValue={value === undefined ? defaultValue ?? items[0]?.value : undefined}
        onValueChange={onValueChange}
        className="min-w-0 flex-1 gap-0"
      >
        <TabsList aria-label={ariaLabel} className="max-w-full justify-start overflow-x-auto">
          {items.map((item) => {
            const Icon = item.icon

            return (
              <TabsTrigger
                key={item.value}
                value={item.value}
                disabled={item.disabled}
              >
                {Icon ? <Icon aria-hidden="true" /> : null}
                <span className="truncate">{item.label}</span>
                {item.badge ? (
                  <Badge variant={item.badge.variant}>{item.badge.label}</Badge>
                ) : null}
              </TabsTrigger>
            )
          })}
        </TabsList>
      </Tabs>
      {trailing ? <div className="shrink-0">{trailing}</div> : null}
    </div>
  )
}

export type MaintenanceBannerTone = "info" | "warning" | "destructive"

export type MaintenanceBannerAction = Omit<
  ButtonProps,
  "children" | "size"
> & {
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
  const actionButton = action ? (() => {
    const {
      label,
      variant = "outline",
      ...buttonProps
    } = action

    return (
      <Button variant={variant} size="xs" {...buttonProps}>
        {label}
      </Button>
    )
  })() : null

  return (
    <Alert
      variant={tone}
      className={cn("rounded-none border-x-0 border-t-0", className)}
      {...props}
    >
      <Icon aria-hidden="true" />
      <AlertTitle>{title}</AlertTitle>
      {description ? (
        <AlertDescription>{description}</AlertDescription>
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
  taskTabs?: React.ReactNode
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
  taskTabs,
  maintenanceBanner,
  defaultSidebarOpen = true,
  sidebarOpen,
  onSidebarOpenChange,
  sidebarCollapsible = "icon",
  sidebarSide = "left",
  sidebarVariant = "sidebar",
  showSidebarRail = true,
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
        {sidebarHeader ? <SidebarHeader>{sidebarHeader}</SidebarHeader> : null}
        <SidebarContent>{sidebarContent}</SidebarContent>
        {sidebarFooter ? <SidebarFooter>{sidebarFooter}</SidebarFooter> : null}
        {showSidebarRail ? <SidebarRail /> : null}
      </Sidebar>
      <SidebarInset
        id={contentId}
        aria-label={contentLabel}
        className="min-w-0 overflow-hidden"
      >
        {topbar}
        {maintenanceBanner}
        {taskTabs}
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

export {
  ErpAppShell,
  GlobalTopbar,
  MaintenanceBanner,
  TaskTabs,
}
