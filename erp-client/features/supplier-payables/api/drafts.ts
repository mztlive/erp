/** W12 供应商往来 · 临时 UI 草稿确认。正式幂等由服务端命令收据负责。 */

import type { SaveAllocationDraftInput } from "@/features/supplier-payables/types"

export async function saveAllocationDraft(
    input: SaveAllocationDraftInput,
): Promise<{ savedAt: string }> {
    void input.formSnapshot
    return { savedAt: new Date().toISOString() }
}
