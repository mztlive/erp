"use client"
import Link from "next/link"
import { useRouter, useSearchParams } from "next/navigation"
import { useAppForm } from "@/components/form"
import { PageScaffold } from "@/components/business"
import { Button } from "@/components/ui/button"
import { CustomerSearchCombobox } from "@/features/entity-selectors/components/customer-search-combobox"
import { SalesOrderSearchCombobox } from "@/features/entity-selectors/components/sales-order-search-combobox"
import { InvoiceRequestPanel } from "./request-panel"
import { requestStatusLabels, type RequestStatus } from "../api"
/** 客户往来开票申请页；筛选条件显式提交并保存在 URL。 */
export function InvoiceRequestsPage() {
    const params = useSearchParams()
    const router = useRouter()
    const status = params.get("status") as RequestStatus | null
    const form = useAppForm({
        defaultValues: {
            q: params.get("q") ?? "",
            customerId: params.get("customerId") ?? "",
            salesOrderId: params.get("salesOrderId") ?? "",
            status: status ?? "",
        },
        onSubmit: ({ value }) => {
            const next = new URLSearchParams()
            for (const [key, entry] of Object.entries(value))
                if (entry.trim()) next.set(key, entry.trim())
            router.replace(
                `/finance/customer-accounts/invoice-requests?${next}`,
            )
        },
    })
    const query = {
        q: params.get("q") ?? undefined,
        customer_id: params.get("customerId") ?? undefined,
        sales_order_id: params.get("salesOrderId") ?? undefined,
        status: status && status in requestStatusLabels ? status : undefined,
    }
    return (
        <PageScaffold density="compact" className="space-y-6">
            <header>
                <h1 className="text-2xl font-semibold">客户往来</h1>
                <p className="mt-2 text-sm text-muted-foreground">
                    管理客户开票申请，跟踪审批与财务开票进度。
                </p>
            </header>
            <nav
                aria-label="客户往来工作视图"
                className="flex gap-5 overflow-x-auto border-b pb-3 text-sm"
            >
                {[
                    ["receivable", "应收"],
                    ["receipt", "回款"],
                    ["sales_invoice", "销项发票"],
                    ["unallocated", "待分配"],
                ].map(([view, label]) => (
                    <Link
                        id={`invoice-request-nav-${view}`}
                        key={view}
                        href={`/finance/customer-accounts?view=${view}`}
                        className="whitespace-nowrap text-muted-foreground"
                    >
                        {label}
                    </Link>
                ))}
                <span
                    aria-current="page"
                    className="whitespace-nowrap font-semibold"
                >
                    开票申请
                </span>
            </nav>
            <InvoiceRequestPanel
                key={params.toString()}
                query={query}
                toolbar={
                    <form
                        id="invoice-request-filters"
                        className="grid items-end gap-3 md:grid-cols-2 xl:grid-cols-5"
                        onSubmit={(e) => {
                            e.preventDefault()
                            void form.handleSubmit()
                        }}
                    >
                        <form.AppField name="q">
                            {(field) => (
                                <field.TextField
                                    id="invoice-request-search"
                                    label="搜索"
                                    placeholder="申请单号、抬头、事由"
                                />
                            )}
                        </form.AppField>
                        <form.AppField name="customerId">
                            {(field) => (
                                <div className="space-y-2 text-sm">
                                    <label htmlFor="invoice-request-filter-customer">
                                        客户
                                    </label>
                                    <CustomerSearchCombobox
                                        id="invoice-request-filter-customer"
                                        purpose="filter"
                                        scope="all_authorized"
                                        value={field.state.value}
                                        onValueChange={(id) =>
                                            field.handleChange(id ?? "")
                                        }
                                    />
                                </div>
                            )}
                        </form.AppField>
                        <form.AppField name="salesOrderId">
                            {(field) => (
                                <div className="space-y-2 text-sm">
                                    <label htmlFor="invoice-request-filter-order">
                                        销售单
                                    </label>
                                    <SalesOrderSearchCombobox
                                        id="invoice-request-filter-order"
                                        value={field.state.value}
                                        onValueChange={(id) =>
                                            field.handleChange(id ?? "")
                                        }
                                    />
                                </div>
                            )}
                        </form.AppField>
                        <form.AppField name="status">
                            {(field) => (
                                <div className="space-y-2 text-sm">
                                    <label htmlFor="invoice-request-filter-status">
                                        状态
                                    </label>
                                    <select
                                        id="invoice-request-filter-status"
                                        className="h-9 w-full rounded-md border bg-background px-3"
                                        value={field.state.value}
                                        onChange={(e) =>
                                            field.handleChange(e.target.value)
                                        }
                                    >
                                        <option value="">全部状态</option>
                                        {Object.entries(
                                            requestStatusLabels,
                                        ).map(([value, label]) => (
                                            <option key={value} value={value}>
                                                {label}
                                            </option>
                                        ))}
                                    </select>
                                </div>
                            )}
                        </form.AppField>
                        <div className="flex gap-2">
                            <Button
                                id="invoice-request-filter-apply"
                                type="submit"
                            >
                                搜索
                            </Button>
                            <Button
                                id="invoice-request-filter-reset"
                                type="button"
                                variant="outline"
                                onClick={() => {
                                    form.reset({
                                        q: "",
                                        customerId: "",
                                        salesOrderId: "",
                                        status: "",
                                    })
                                    router.replace(
                                        "/finance/customer-accounts/invoice-requests",
                                    )
                                }}
                            >
                                重置
                            </Button>
                        </div>
                    </form>
                }
                initialRequestId={params.get("requestId") ?? undefined}
                initialCreate={params.get("create") === "1"}
            />
        </PageScaffold>
    )
}
