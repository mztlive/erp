"use client"

import { useRouter } from "next/navigation"

import { DiscardConfirmDialog } from "@/components/business"
import { SupplierDisableDialog } from "@/features/master-data/components/shared/disable-action-dialog"
import { SupplierSaveReasonDialog } from "@/features/master-data/components/supplier/supplier-save-reason-dialog"
import type { MasterDataCenterView } from "@/features/master-data/types"

export function SupplierEditorDialogs({
    isCreate,
    data,
    disableOpen,
    setDisableOpen,
    saveReasonOpen,
    setSaveReasonOpen,
    reasonDraft,
    setReasonDraft,
    reasonError,
    setReasonError,
    pending,
    onConfirm,
    discardOpen,
    setDiscardOpen,
    pendingNav,
    setPendingNav,
    router,
}: {
    isCreate: boolean
    data: MasterDataCenterView | null | undefined
    disableOpen: boolean
    setDisableOpen: (open: boolean) => void
    saveReasonOpen: boolean
    setSaveReasonOpen: (open: boolean) => void
    reasonDraft: string
    setReasonDraft: (reason: string) => void
    reasonError: string | null
    setReasonError: (error: string | null) => void
    pending: boolean
    onConfirm: () => void
    discardOpen: boolean
    setDiscardOpen: (open: boolean) => void
    pendingNav: string | null
    setPendingNav: (href: string | null) => void
    router: ReturnType<typeof useRouter>
}) {
    return (
        <>
            {!isCreate && data ? (
                <SupplierDisableDialog
                    open={disableOpen}
                    onOpenChange={setDisableOpen}
                    target={data}
                />
            ) : null}

            <SupplierSaveReasonDialog
                open={saveReasonOpen}
                onOpenChange={(open) => {
                    setSaveReasonOpen(open)
                    if (!open) setReasonError(null)
                }}
                isCreate={isCreate}
                reason={reasonDraft}
                onReasonChange={(reason) => {
                    setReasonDraft(reason)
                    if (reasonError) setReasonError(null)
                }}
                reasonError={reasonError}
                pending={pending}
                onConfirm={onConfirm}
            />

            <DiscardConfirmDialog
                open={discardOpen}
                onOpenChange={setDiscardOpen}
                title="放弃未保存的更改？"
                description="本次修改尚未保存，离开后将丢失。"
                confirmLabel="放弃更改"
                cancelLabel="继续编辑"
                onConfirm={() => {
                    setDiscardOpen(false)
                    if (pendingNav) {
                        setPendingNav(null)
                        router.push(pendingNav)
                    }
                }}
            />
        </>
    )
}
