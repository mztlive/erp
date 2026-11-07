/** 仅保存结果待确认命令的原始回款凭证，供刷新后按原命令重试。 */
import { createIndexedDbValueStore } from "@/lib/indexeddb-value-store"

const store = createIndexedDbValueStore("erp-receipt-files-v1", {
    unavailable: "本机回款凭证文件存储不可用",
    blocked: "本机回款凭证文件存储被占用，请关闭旧页面后重试",
    writeFailed: "本机回款凭证文件保存失败",
})

export function readReceiptFileValue<T>(key: string): Promise<T | undefined> {
    return store.read<T>(key)
}

export async function writeReceiptFileValue<T>(
    key: string,
    value: T,
): Promise<void> {
    await store.write(key, value)
}

export async function removeReceiptFileValue(key: string): Promise<void> {
    await store.remove(key)
}
