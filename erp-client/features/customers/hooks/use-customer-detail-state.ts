"use client"

import * as React from "react"
import { useRouter } from "@/lib/navigation"

import { toast } from "@/components/ui/toast"
import { useCustomerCenterQuery } from "@/features/customers/hooks/queries"
import type { CustomerSectionId } from "@/features/customers/types"
import { resolveSection } from "@/features/customers/pages/customer-detail-helpers"

export type CustomerPendingNavigation =
    | { kind: "section"; section: CustomerSectionId }
    | { kind: "back" }
    | { kind: "href"; href: string; label: string }

/**
 * 客户详情页交互状态：分区切换、未保存离开确认（切 Tab / 返回 / 顶栏跨页链接）与保存成功反馈。
 * 数据始终消费最新 query.data，不再冻结编辑快照。
 */
export function useCustomerDetailState(customerId: string, section?: string) {
    const query = useCustomerCenterQuery(customerId)
    const router = useRouter()
    const activeSection = resolveSection(section)
    const [formDirty, setFormDirty] = React.useState(false)
    const [formResetEpoch, setFormResetEpoch] = React.useState(0)
    const [pendingNavigation, setPendingNavigation] =
        React.useState<CustomerPendingNavigation | null>(null)
    const customer = query.data

    const selectSection = React.useCallback(
        (next: CustomerSectionId) => {
            router.replace(
                next === "overview"
                    ? `/sales/customers/${customerId}`
                    : `/sales/customers/${customerId}?section=${next}`,
                { scroll: false },
            )
        },
        [customerId, router],
    )

    const handleSectionChange = React.useCallback(
        (next: CustomerSectionId) => {
            if (next === activeSection) return
            if (formDirty) {
                setPendingNavigation({ kind: "section", section: next })
                return
            }
            selectSection(next)
        },
        [activeSection, formDirty, selectSection],
    )

    const handleBack = React.useCallback(() => {
        if (formDirty) {
            setPendingNavigation({ kind: "back" })
            return
        }
        router.push("/sales/customers")
    }, [formDirty, router])

    const handleNavigateHref = React.useCallback(
        (href: string, label: string) => {
            if (formDirty) {
                setPendingNavigation({ kind: "href", href, label })
                return
            }
            router.push(href)
        },
        [formDirty, router],
    )

    const handleFormReset = React.useCallback(() => {
        setFormDirty(false)
    }, [])

    /** 保存成功：重置脏态、刷新详情并通过短暂 Toast 提示结果。 */
    const completeEditing = React.useCallback(
        (_revisionNo?: number) => {
            setFormDirty(false)
            void query.refetch()
            toast.add({
                title: "客户资料已保存",
                description: "客户资料已更新，历史单据记录不受影响。",
                type: "success",
                timeout: 4000,
            })
        },
        [query],
    )

    const dismissPendingNavigation = React.useCallback(
        () => setPendingNavigation(null),
        [],
    )

    const discardPendingAndNavigate = React.useCallback(() => {
        if (!pendingNavigation) return
        const target = pendingNavigation
        setPendingNavigation(null)
        setFormDirty(false)
        setFormResetEpoch((epoch) => epoch + 1)
        if (target.kind === "back") {
            router.push("/sales/customers")
            return
        }
        if (target.kind === "href") {
            router.push(target.href)
            return
        }
        selectSection(target.section)
    }, [pendingNavigation, router, selectSection])

    return {
        query,
        customer,
        activeSection,
        formDirty,
        formResetEpoch,
        setFormDirty,
        handleFormReset,
        completeEditing,
        handleSectionChange,
        handleBack,
        handleNavigateHref,
        pendingNavigation,
        dismissPendingNavigation,
        discardPendingAndNavigate,
    }
}
