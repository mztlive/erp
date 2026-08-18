"use client"

import * as React from "react"
import { useRouter } from "next/navigation"
import { ListTodoIcon } from "lucide-react"

import { GlobalTopbar } from "@/components/business"
import { WorkspaceAccountMenu } from "@/components/layout/workspace-account-menu"
import { hasAnyPermission, hasPermission } from "@/lib/permissions"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { useCustomerDirectoryQuery } from "@/features/customers/queries"
import { useWorkspaceInboxCountQuery } from "@/features/workspace/hooks/queries"

export function WorkspaceTopbar() {
    const router = useRouter()
    const profileQuery = useAccountProfileQuery()
    const permissions = profileQuery.data?.permissions
    const canSeeTodos = hasAnyPermission(permissions, ["work_item:list"])
    const canSearchCustomers = hasAnyPermission(permissions, ["customer:list"])
    const canSearchAllCustomers = hasPermission(
        permissions,
        "customer_scope:detail",
    )
    const [search, setSearch] = React.useState("")
    const [searchFocused, setSearchFocused] = React.useState(false)
    const customerSearchQuery = useCustomerDirectoryQuery(
        {
            scope: canSearchAllCustomers ? "all_authorized" : "assigned",
            status: "all",
            query: search.trim(),
            page: 1,
            pageSize: 5,
        },
        {
            // 无客户 list 权限或未输入关键字时不请求，避免侧栏壳层无谓 403
            enabled: canSearchCustomers && search.trim().length >= 2,
        },
    )
    const todoCountQuery = useWorkspaceInboxCountQuery()
    const todoCount = canSeeTodos ? todoCountQuery.data?.mine : undefined
    const customerMatches = React.useMemo(
        () =>
            canSearchCustomers && search.trim().length >= 2
                ? (customerSearchQuery.data?.items.slice(0, 5) ?? [])
                : [],
        [canSearchCustomers, customerSearchQuery.data?.items, search],
    )

    React.useEffect(() => {
        const focusGlobalSearch = (event: KeyboardEvent) => {
            if (
                (event.metaKey || event.ctrlKey) &&
                event.key.toLowerCase() === "k"
            ) {
                event.preventDefault()
                document
                    .querySelector<HTMLInputElement>(
                        'input[aria-label="全局搜索"]',
                    )
                    ?.focus()
            }
        }
        window.addEventListener("keydown", focusGlobalSearch)
        return () => window.removeEventListener("keydown", focusGlobalSearch)
    }, [])

    const submitSearch = React.useCallback(() => {
        const query = search.trim()
        if (!query) return
        const exactCustomer = customerMatches.find((customer) =>
            [customer.customerNo, customer.legalName, customer.shortName]
                .filter(Boolean)
                .some(
                    (value) =>
                        value?.toLocaleLowerCase() ===
                        query.toLocaleLowerCase(),
                ),
        )
        if (exactCustomer) {
            router.push(`/sales/customers/${exactCustomer.id}`)
            setSearchFocused(false)
            return
        }
        if (hasAnyPermission(permissions, ["sales_order:list"])) {
            router.push(`/sales/orders?search=${encodeURIComponent(query)}`)
        }
        setSearchFocused(false)
    }, [customerMatches, permissions, router, search])

    const openCustomer = React.useCallback(
        (customer: (typeof customerMatches)[number]) => {
            router.push(`/sales/customers/${customer.id}`)
            setSearch("")
            setSearchFocused(false)
        },
        [router],
    )

    const topbarActions = canSeeTodos
        ? [
              {
                  actionKey: "todos",
                  label: "待办",
                  icon: ListTodoIcon,
                  badge:
                      todoCount && todoCount > 0
                          ? {
                                label: String(todoCount),
                                variant: "secondary" as const,
                            }
                          : undefined,
                  onClick: () => router.push("/workspace/tasks"),
              },
          ]
        : []

    return (
        <div className="relative">
            <GlobalTopbar
                showSidebarTrigger
                search={{
                    ariaLabel: "全局搜索",
                    placeholder: "单号、客户、合同…",
                    shortcut: "⌘K",
                    value: search,
                    onChange: (event) => setSearch(event.target.value),
                    onFocus: () => setSearchFocused(true),
                    onBlur: () => setSearchFocused(false),
                    onKeyDown: (event) => {
                        if (event.key === "Escape") {
                            setSearchFocused(false)
                            event.currentTarget.blur()
                        } else if (event.key === "Enter") submitSearch()
                    },
                }}
                actions={topbarActions}
                trailing={<WorkspaceAccountMenu />}
            />
            {searchFocused &&
            search.trim().length >= 2 &&
            canSearchCustomers ? (
                <div
                    role="listbox"
                    aria-label="客户搜索结果"
                    className="absolute right-4 top-[calc(100%-0.25rem)] z-50 w-[min(24rem,calc(100vw-2rem))] rounded-xl border bg-popover p-1 text-popover-foreground shadow-md md:right-6"
                    onMouseDown={(event) => event.preventDefault()}
                >
                    <p className="px-2 py-1 text-xs font-medium text-muted-foreground">
                        客户
                    </p>
                    {customerSearchQuery.isFetching ? (
                        <p className="px-2 py-2 text-sm text-muted-foreground">
                            正在搜索…
                        </p>
                    ) : customerMatches.length > 0 ? (
                        customerMatches.map((customer) => (
                            <button
                                key={customer.id}
                                type="button"
                                role="option"
                                aria-selected="false"
                                className="flex w-full items-center justify-between gap-3 rounded-lg px-2 py-2 text-left text-sm hover:bg-accent focus-visible:bg-accent focus-visible:outline-none"
                                onClick={() => openCustomer(customer)}
                            >
                                <span className="min-w-0 truncate font-medium">
                                    {customer.shortName ?? customer.legalName}
                                </span>
                                <span className="num shrink-0 text-xs text-muted-foreground">
                                    {customer.customerNo}
                                </span>
                            </button>
                        ))
                    ) : (
                        <p className="px-2 py-2 text-sm text-muted-foreground">
                            无客户匹配；按 Enter 搜索销售单
                        </p>
                    )}
                </div>
            ) : null}
        </div>
    )
}
