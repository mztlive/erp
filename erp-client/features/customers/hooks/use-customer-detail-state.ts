"use client"

import * as React from "react"
import { useRouter } from "next/navigation"

import { useCustomerCenterQuery } from "@/features/customers/hooks/queries"
import type { CustomerSectionId } from "@/features/customers/types"
import { resolveSection } from "@/features/customers/pages/customer-detail-helpers"

/**
 * 客户详情页交互状态：分区切换、原地编辑、未保存拦截与保存成功提示。
 * 数据请求仍由 useCustomerCenterQuery 承载，本 hook 只编排 UI 状态。
 */
export function useCustomerDetailState(
    customerId: string,
    section?: string,
) {
    const query = useCustomerCenterQuery(customerId)
    const router = useRouter()
    const activeSection = resolveSection(section)
    const [editing, setEditing] = React.useState(false)
    const [formDirty, setFormDirty] = React.useState(false)
    const [pendingSection, setPendingSection] =
        React.useState<CustomerSectionId | null>(null)
    const [savedNotice, setSavedNotice] = React.useState<{
        revisionNo: number
    } | null>(null)

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

    /** 编辑中且未保存时，切 Tab 先弹确认，避免静默丢输入。 */
    const handleSectionChange = React.useCallback(
        (next: CustomerSectionId) => {
            if (next === activeSection) return
            if (editing && formDirty) {
                setPendingSection(next)
                return
            }
            selectSection(next)
        },
        [activeSection, editing, formDirty, selectSection],
    )

    const startEditing = React.useCallback(() => setEditing(true), [])
    const cancelEditing = React.useCallback(() => {
        setEditing(false)
        setFormDirty(false)
    }, [])

    /** 保存成功：退出编辑并记录新版本提示。 */
    const completeEditing = React.useCallback(
        (revisionNo?: number) => {
            setEditing(false)
            setFormDirty(false)
            setSavedNotice({
                revisionNo: revisionNo ?? customer?.currentRevision.revisionNo ?? 0,
            })
        },
        [customer],
    )

    const dismissSavedNotice = React.useCallback(() => setSavedNotice(null), [])

    const dismissPendingSection = React.useCallback(
        () => setPendingSection(null),
        [],
    )

    const discardPendingAndSwitch = React.useCallback(() => {
        setPendingSection((next) => {
            if (next) {
                setEditing(false)
                setFormDirty(false)
                selectSection(next)
            }
            return null
        })
    }, [selectSection])

    return {
        query,
        customer,
        activeSection,
        editing,
        startEditing,
        cancelEditing,
        completeEditing,
        formDirty,
        setFormDirty,
        handleSectionChange,
        savedNotice,
        dismissSavedNotice,
        pendingSection,
        dismissPendingSection,
        discardPendingAndSwitch,
    }
}
