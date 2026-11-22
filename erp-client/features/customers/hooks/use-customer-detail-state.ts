"use client"

import * as React from "react"
import { useRouter } from "@/lib/navigation"

import { toast } from "@/components/ui/toast"
import { useCustomerCenterQuery } from "@/features/customers/hooks/queries"
import {
    resolveSection,
    type CustomerActiveSectionId,
} from "@/features/customers/pages/customer-detail-helpers"

export type CustomerPendingNavigation =
    | { kind: "section"; section: CustomerActiveSectionId }
    | { kind: "back" }
    | { kind: "href"; href: string; label: string }
    | { kind: "cancel-edit" }

/**
 * 客户详情页交互状态：默认阅读优先，按需进入原地编辑；
 * 统一管理分区切换、未保存离开确认（切 Tab / 退出编辑 / 返回 / 顶栏跨页链接）与保存反馈。
 */
export function useCustomerDetailState(customerId: string, section?: string) {
    const query = useCustomerCenterQuery(customerId)
    const router = useRouter()
    const activeSection = resolveSection(section)
    const [editing, setEditing] = React.useState(false)
    const [formDirty, setFormDirty] = React.useState(false)
    const [formResetEpoch, setFormResetEpoch] = React.useState(0)
    const [pendingNavigation, setPendingNavigation] =
        React.useState<CustomerPendingNavigation | null>(null)
    const customer = query.data

    const selectSection = React.useCallback(
        (next: CustomerActiveSectionId) => {
            router.replace(
                next === "overview"
                    ? `/sales/customers/${customerId}`
                    : `/sales/customers/${customerId}?section=${next}`,
                { scroll: false },
            )
        },
        [customerId, router],
    )

    const startEditing = React.useCallback(() => {
        if (activeSection !== "overview") {
            selectSection("overview")
        }
        setEditing(true)
    }, [activeSection, selectSection])

    const requestCancelEditing = React.useCallback(() => {
        if (formDirty) {
            setPendingNavigation({ kind: "cancel-edit" })
            return
        }
        setEditing(false)
        setFormDirty(false)
    }, [formDirty])

    const handleSectionChange = React.useCallback(
        (next: CustomerActiveSectionId) => {
            if (next === activeSection) return
            if (editing && formDirty) {
                setPendingNavigation({ kind: "section", section: next })
                return
            }
            if (editing) {
                setEditing(false)
                setFormDirty(false)
            }
            selectSection(next)
        },
        [activeSection, editing, formDirty, selectSection],
    )

    const handleBack = React.useCallback(() => {
        if (editing && formDirty) {
            setPendingNavigation({ kind: "back" })
            return
        }
        router.push("/sales/customers")
    }, [editing, formDirty, router])

    const handleNavigateHref = React.useCallback(
        (href: string, label: string) => {
            if (editing && formDirty) {
                setPendingNavigation({ kind: "href", href, label })
                return
            }
            router.push(href)
        },
        [editing, formDirty, router],
    )

    const handleFormReset = React.useCallback(() => {
        setFormDirty(false)
        setEditing(false)
    }, [])

    /** 保存成功：退出编辑、重置脏态、刷新详情并通过短暂 Toast 提示结果。 */
    const completeEditing = React.useCallback(
        (revisionNo?: number) => {
            setFormDirty(false)
            setEditing(false)
            void query.refetch()
            toast.add({
                title:
                    revisionNo != null
                        ? `客户资料已保存（v${revisionNo}）`
                        : "客户资料已保存",
                description:
                    "新资料仅对后续新业务生效，历史合同与销售单不受影响。",
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
        setEditing(false)
        setFormResetEpoch((epoch) => epoch + 1)
        if (target.kind === "cancel-edit") {
            return
        }
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
        editing,
        startEditing,
        requestCancelEditing,
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
