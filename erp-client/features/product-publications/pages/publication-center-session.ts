"use client"

import * as React from "react"
import { z } from "zod"

import { useAppForm } from "@/components/form"
import {
    publishSchema,
    type SessionEdit,
} from "@/features/product-publications/lib/publish-form"
import type {
    ProductPublicationView,
    SaleStatus,
} from "@/features/product-publications/types"

export type PublicationCenterFormValues = z.infer<typeof publishSchema>

export function usePublicationCenterForm(options: {
    onSubmitRequest: () => void
}) {
    return useAppForm({
        defaultValues: {
            name: "",
            specification: "",
            salesDescription: "",
            minimumPurchaseQuantity: "1",
            salesPriceGross: "",
            salesTaxRate: "0.13",
            categoryId: "",
            skuRevisionId: "",
            supplierOfferingRevisionId: "",
            baseUnitCode: "",
            salesRegionText: "",
            productCapabilitiesText: "",
            validFrom: "",
            validTo: "",
            media: [] as Array<{
                fileAssetId: string
                mediaRole: "MAIN" | "CAROUSEL" | "DETAIL"
                sortNo: number
                altText: string
            }>,
            saleStatus: "ON_SALE" as SaleStatus,
        },
        validators: { onChange: publishSchema },
        onSubmit: async () => {
            options.onSubmitRequest()
        },
    })
}

export type PublicationCenterFormApi = ReturnType<
    typeof usePublicationCenterForm
>

export function usePublicationCenterSession(options: {
    form: PublicationCenterFormApi
    onCloseConfirm: () => void
    onStartEdit: () => void
}) {
    const { form, onCloseConfirm, onStartEdit } = options
    const [sessionEdit, setSessionEdit] = React.useState<SessionEdit | null>(
        null,
    )
    const dirty = sessionEdit != null

    // Session-only: no localStorage / draft mutation; warn before unload
    React.useEffect(() => {
        if (!dirty) return
        const onBeforeUnload = (e: BeforeUnloadEvent) => {
            e.preventDefault()
            e.returnValue = "当前输入尚未提交，刷新后将丢失。"
        }
        window.addEventListener("beforeunload", onBeforeUnload)
        return () => window.removeEventListener("beforeunload", onBeforeUnload)
    }, [dirty])

    const startPrepareRevision = React.useCallback(
        (data: ProductPublicationView) => {
            const base = data.selectedRevision
            const edit: SessionEdit = {
                baselineRevisionId: base.revisionId,
                name: base.name,
                specification: base.specification,
                salesDescription: base.salesDescription,
                minimumPurchaseQuantity: base.minimumPurchaseQuantity,
                salesPriceGross: base.salesPriceGross,
                salesTaxRate: base.salesTaxRate,
                saleStatus:
                    base.saleStatus === "PAUSED" ? "PAUSED" : base.saleStatus,
                baseUnitCode: base.baseUnitCode,
                salesRegion:
                    base.salesRegion ??
                    base.salesRegionLabel
                        .split(/[、，,]/)
                        .map((entry) => entry.trim())
                        .filter(Boolean),
                categoryId: base.categoryId,
                skuRevisionId: base.skuRevisionId,
                supplierOfferingRevisionId: base.supplierOfferingRevisionId,
                productCapabilities: [...base.productCapabilities],
                validFrom: new Date().toISOString(),
                media: base.media.map((m) => ({ ...m })),
            }
            setSessionEdit(edit)
            form.reset({
                name: edit.name,
                specification: edit.specification,
                salesDescription: edit.salesDescription,
                minimumPurchaseQuantity: edit.minimumPurchaseQuantity,
                salesPriceGross: edit.salesPriceGross,
                salesTaxRate: edit.salesTaxRate,
                categoryId: edit.categoryId,
                skuRevisionId: edit.skuRevisionId,
                supplierOfferingRevisionId: edit.supplierOfferingRevisionId,
                baseUnitCode: edit.baseUnitCode,
                salesRegionText: edit.salesRegion.join("、"),
                productCapabilitiesText: edit.productCapabilities.join("、"),
                validFrom: edit.validFrom,
                validTo: edit.validTo ?? "",
                media: edit.media.map((media) => ({
                    fileAssetId: media.fileAssetId,
                    mediaRole: media.mediaRole,
                    sortNo: media.sortNo,
                    altText: media.altText,
                })),
                saleStatus: edit.saleStatus,
            })
            onStartEdit()
        },
        [form, onStartEdit],
    )

    const discardSession = () => {
        if (
            sessionEdit &&
            !window.confirm("放弃本次输入？未提交内容将丢失，不会保存草稿。")
        ) {
            return
        }
        setSessionEdit(null)
        onCloseConfirm()
    }

    return {
        sessionEdit,
        dirty,
        setSessionEdit,
        startPrepareRevision,
        discardSession,
    }
}
