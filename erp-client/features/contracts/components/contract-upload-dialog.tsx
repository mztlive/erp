"use client"

import { Dialog } from "@/components/ui/dialog"
import type { UseContractUploadFormOptions } from "../hooks/use-contract-upload-form"
import { ContractImportWorkspace } from "./contract-import-workspace"

export type ContractUploadDialogProps = UseContractUploadFormOptions

export function ContractUploadDialog(props: ContractUploadDialogProps) {
    return (
        <Dialog open={props.open} onOpenChange={props.onOpenChange}>
            <ContractImportWorkspace {...props} presentation="dialog" />
        </Dialog>
    )
}
