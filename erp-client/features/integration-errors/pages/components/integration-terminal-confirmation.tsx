import type { TerminalConfirm } from "../../components/terminal-action-dialog"
import { TerminalActionDialog } from "../../components/terminal-action-dialog"
import type { IntegrationResolutionItemView } from "../../types"

export function IntegrationTerminalConfirmation({
    confirm,
    item,
    pending,
    onConfirmKind,
    onClose,
}: {
    confirm: TerminalConfirm
    item: IntegrationResolutionItemView
    pending: boolean
    onConfirmKind: (kind: TerminalConfirm["kind"]) => Promise<void>
    onClose: () => void
}) {
    return (
        <TerminalActionDialog
            confirm={confirm}
            item={item}
            pending={pending}
            onConfirm={async () => {
                await onConfirmKind(confirm.kind)
                onClose()
            }}
            onCancel={onClose}
        />
    )
}
