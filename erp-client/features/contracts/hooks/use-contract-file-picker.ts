"use client"

import { useRef, type ChangeEvent } from "react"
import { toast } from "@/components/ui/toast"
import { fileSelectionError } from "@/lib/file-selection"
import { contractPdfError } from "@/features/contracts/lib/pdf"

/** 在用户点击入口时直接选文件，校验通过后才进入导入窗口。 */
export function useContractFilePicker(onSelect: (file: File) => void) {
    const ref = useRef<HTMLInputElement>(null)
    return {
        selectFile: () => ref.current?.click(),
        inputProps: {
            ref,
            accept: "application/pdf,.pdf",
            hidden: true,
            onChange: (event: ChangeEvent<HTMLInputElement>) => {
                const files = Array.from(event.target.files ?? [])
                const file = files[0]
                event.target.value = ""
                if (!file) return
                const error =
                    fileSelectionError(files, { multiple: false }) ||
                    contractPdfError(file)
                if (error) {
                    toast.add({ title: error, type: "error" })
                    return
                }
                onSelect(file)
            },
        },
    }
}
