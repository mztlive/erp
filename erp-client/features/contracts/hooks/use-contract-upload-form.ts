"use client"
import * as React from "react"
import { z } from "zod"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useAppForm } from "@/components/form"
import { useUploadContractPdfMutation } from "@/features/contracts/hooks/queries"
import { contractPdfError } from "@/features/contracts/lib/pdf"
import {
    fetchContractImport,
    confirmContractImport,
    fetchContractImports,
    runContractImport,
    previewContractImport,
    type ContractImportTask,
} from "@/features/contracts/api/upload"
import { contractImportContextError } from "@/features/contracts/lib/import-context"
import type { UploadContractPdfResult } from "@/features/contracts/types"

export type UseContractUploadFormOptions = {
    open: boolean
    onOpenChange: (open: boolean) => void
    initialCustomerId?: string
    revisionTarget?: { contractId: string; version: number }
    onSuccess?: (result: UploadContractPdfResult) => void
}

export function useContractUploadForm(options: UseContractUploadFormOptions) {
    const client = useQueryClient()
    const [selectedId, setSelectedId] = React.useState("")
    const [page, setPage] = React.useState(1)
    const [preview, setPreview] = React.useState({ id: "", url: "" })
    const previewUrl = preview.id === selectedId ? preview.url : ""
    const request = React.useRef<{
        file: File
        customer?: string
        target?: string
        key: string
    } | null>(null)
    const uploadMutation = useUploadContractPdfMutation()
    const refresh = async (task: ContractImportTask) => {
        client.setQueryData(["contract-imports", "detail", task.id], task)
        await client.invalidateQueries({
            queryKey: ["contract-imports", "list"],
        })
        if (task.status === "succeeded") {
            await client.invalidateQueries({ queryKey: ["contracts"] })
            await client.invalidateQueries({ queryKey: ["entity-selectors"] })
        }
    }
    const runMutation = useMutation({
        mutationFn: runContractImport,
        onSuccess: refresh,
        onSettled: () =>
            client.invalidateQueries({ queryKey: ["contract-imports"] }),
    })
    const confirmMutation = useMutation({
        mutationFn: confirmContractImport,
        onSuccess: refresh,
    })
    const list = useQuery({
        queryKey: [
            "contract-imports",
            "list",
            options.revisionTarget?.contractId,
            page,
        ],
        queryFn: () =>
            fetchContractImports(page, options.revisionTarget?.contractId),
        enabled: options.open,
        refetchInterval: (query) =>
            query.state.data?.items.some(
                (item) =>
                    item.status === "processing" || item.status === "ready",
            )
                ? 3000
                : false,
    })
    const detail = useQuery({
        queryKey: ["contract-imports", "detail", selectedId],
        queryFn: () => fetchContractImport(selectedId),
        enabled: options.open && Boolean(selectedId),
        refetchInterval: (query) =>
            query.state.data?.status === "processing" ||
            query.state.data?.status === "ready"
                ? 3000
                : false,
    })
    const observedSuccess = React.useRef("")
    React.useEffect(() => {
        const task = detail.data
        if (task?.status !== "succeeded" || observedSuccess.current === task.id)
            return
        observedSuccess.current = task.id
        void client.invalidateQueries({ queryKey: ["contracts"] })
        void client.invalidateQueries({ queryKey: ["entity-selectors"] })
        void client.invalidateQueries({
            queryKey: ["contract-imports", "list"],
        })
    }, [client, detail.data])
    const contextError = detail.data
        ? contractImportContextError(
              detail.data,
              options.initialCustomerId,
              options.revisionTarget,
          )
        : undefined
    const canAccept =
        detail.data?.status === "succeeded" &&
        Boolean(detail.data.result) &&
        !contextError
    const retry = () => {
        if (!detail.data || contextError) return
        runMutation.mutate(detail.data.id)
    }
    const previewMutation = useMutation({
        mutationFn: previewContractImport,
        onSuccess: (blob, id) =>
            setPreview({ id, url: URL.createObjectURL(blob) }),
    })
    React.useEffect(
        () => () => {
            if (preview.url) URL.revokeObjectURL(preview.url)
        },
        [preview.url],
    )
    const form = useAppForm({
        defaultValues: { pdfFile: null as File | null },
        validators: {
            onChange: z.object({
                pdfFile: z
                    .custom<File | null>()
                    .refine(
                        (file) => !contractPdfError(file),
                        "请上传有效的合同 PDF",
                    ),
            }),
        },
        onSubmit: async ({ value }) => {
            if (!value.pdfFile) return
            const target = JSON.stringify(options.revisionTarget)
            if (
                request.current?.file !== value.pdfFile ||
                request.current.customer !== options.initialCustomerId ||
                request.current.target !== target
            ) {
                request.current = {
                    file: value.pdfFile,
                    customer: options.initialCustomerId,
                    target,
                    key: crypto.randomUUID(),
                }
            }
            const task = await uploadMutation.mutateAsync({
                pdfFile: value.pdfFile,
                customerId: options.initialCustomerId,
                idempotencyKey: request.current.key,
                revisionTarget: options.revisionTarget,
            })
            setSelectedId(task.id)
            setPreview({ id: "", url: "" })
            await refresh(task)
            if (task.status === "ready") await runMutation.mutateAsync(task.id)
        },
    })
    const select = (id: string) => {
        setSelectedId(id)
        setPreview({ id: "", url: "" })
        runMutation.reset()
        confirmMutation.reset()
        previewMutation.reset()
        uploadMutation.reset()
    }
    const startNew = () => {
        if (uploadMutation.isPending || runMutation.isPending) return
        select("")
        form.reset()
        request.current = null
    }
    const accept = () => {
        const result = detail.data?.result
        if (detail.data?.status !== "succeeded" || !result || contextError)
            return
        options.onSuccess?.({
            contractId: result.id,
            contractNo: result.contract_no,
            revisionId: result.revision_id,
            revisionNo: result.revision_no,
            uploadedAt: new Date(result.created_at * 1000).toISOString(),
            fileName: result.file_name,
            reference: `CT-UP-${result.contract_no}`,
        })
        options.onOpenChange(false)
    }
    return {
        form,
        uploadMutation,
        runMutation,
        confirmMutation,
        list,
        detail,
        select,
        selectedId,
        page,
        setPage,
        previewMutation,
        previewUrl,
        accept,
        canAccept,
        contextError,
        retry,
        startNew,
    }
}
