import { useRef, useState } from "react"
import { useMutation } from "@tanstack/react-query"
import { useAppForm } from "@/components/form"
import { uploadEvidenceFileAsset } from "@/features/file-assets/api/file-assets"
import { classifyFormalCommandError } from "@/lib/formal-command"
import type { BatchDraftCommand } from "../api"
import { draftDefaults, draftSchema } from "../lib/draft"
import type {
    PaymentCandidate,
    PaymentRequest,
    RequestDraftInput,
} from "../types"
import { useSaveRequestBatchMutation, useSaveRequestMutation } from "./queries"

export function useDraftForm(
    candidates: PaymentCandidate[],
    purchaseId: string | undefined,
    request: PaymentRequest | undefined,
    onSaved: (rows: PaymentRequest[]) => void,
) {
    const batch = useSaveRequestBatchMutation()
    const edit = useSaveRequestMutation()
    const upload = useMutation({
        mutationFn: (files: File[]) =>
            Promise.all(files.map(uploadEvidenceFileAsset)),
    })
    const command = useRef<BatchDraftCommand | null>(null)
    const uploaded = useRef(new Map<File, string>())
    const [unknown, setUnknown] = useState(false)
    const form = useAppForm({
        defaultValues: draftDefaults(candidates, purchaseId, request),
        validators: { onSubmit: draftSchema(candidates) },
        onSubmit: async ({ value }) => {
            if (!command.current) {
                const requests: RequestDraftInput[] = []
                for (const group of value.groups) {
                    const candidate = candidates.find(
                        (item) => item.payable_account_id === group.ids[0],
                    )!
                    const account = candidate.bank_accounts.find(
                        (item) => item.bank_account_id === group.account,
                    )!
                    const missing = group.files.filter(
                        (file) => !uploaded.current.has(file),
                    )
                    const assets = await upload.mutateAsync(missing)
                    missing.forEach((file, index) =>
                        uploaded.current.set(file, assets[index].id),
                    )
                    requests.push({
                        supplier_id: candidate.supplier_id,
                        bank_account_id: account.bank_account_id,
                        bank_account_version: account.version,
                        desired_payment_date: group.date,
                        description: group.description.trim(),
                        attachment_ids: [
                            ...(request?.attachment_ids ?? []),
                            ...group.files.map((file) =>
                                uploaded.current.get(file)!,
                            ),
                        ],
                        lines: group.ids.map((id) => ({
                            payable_account_id: id,
                            amount: value.amounts[id].trim(),
                        })),
                    })
                }
                command.current = {
                    idempotency_key: `ppr-batch:${crypto.randomUUID()}`,
                    requests,
                }
            }
            try {
                const rows = request
                    ? [
                          await edit.mutateAsync({
                              id: request.id,
                              version: request.version,
                              data: command.current.requests[0],
                          }),
                      ]
                    : await batch.mutateAsync(command.current)
                command.current = null
                setUnknown(false)
                onSaved(rows)
            } catch (error) {
                const uncertain =
                    !request && classifyFormalCommandError(error) === "unknown"
                setUnknown(uncertain)
                if (!uncertain) command.current = null
                throw error
            }
        },
    })
    const recover = async () => {
        if (!command.current || batch.isPending) return
        const rows = await batch.mutateAsync(command.current)
        command.current = null
        setUnknown(false)
        onSaved(rows)
    }
    return {
        form,
        unknown,
        recover,
        pending: batch.isPending || edit.isPending || upload.isPending,
        error: batch.error ?? edit.error ?? upload.error,
    }
}
export type DraftForm = ReturnType<typeof useDraftForm>["form"]
