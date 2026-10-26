"use client"

import Link from "next/link"
import { ArrowUpRightIcon } from "lucide-react"

import { Button } from "@/components/ui/button"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import { navigateWorkspace } from "@/lib/workspace-tabs"

/** 工作台内打开供应商页签；独立页面打开新窗口，保留来源表单。 */
export function SupplierDeliveryTemplatesLink({
    id,
    supplierId,
    disabled = false,
}: {
    id: string
    supplierId: string
    disabled?: boolean
}) {
    const account = useAccountProfileQuery()
    const permissions = account.data?.permissions
    if (
        !hasPermission(permissions, "supplier:detail") ||
        !hasPermission(permissions, "supplier_offering:list")
    )
        return null
    const href = `/master-data/suppliers/${encodeURIComponent(supplierId)}?section=delivery`
    return (
        <Button
            id={id}
            type="button"
            variant="link"
            size="sm"
            disabled={disabled || !supplierId}
            render={
                <Link
                    href={href}
                    target="_blank"
                    rel="noopener noreferrer"
                    onClick={(event) => {
                        if (
                            !event.defaultPrevented &&
                            event.button === 0 &&
                            !event.metaKey &&
                            !event.ctrlKey &&
                            !event.shiftKey &&
                            !event.altKey &&
                            navigateWorkspace(href)
                        )
                            event.preventDefault()
                    }}
                />
            }
        >
            管理该供应商配送模板
            <ArrowUpRightIcon data-icon="inline-end" aria-hidden />
        </Button>
    )
}
