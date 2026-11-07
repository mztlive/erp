/** 询价命令与凭证使用结构化存储，保留 File 原始内容供刷新后核对。 */
import { createIndexedDbValueStore } from "@/lib/indexeddb-value-store"

const store = createIndexedDbValueStore("erp-inquiry-files-v1", {
    unavailable: "本机询价文件存储不可用",
    blocked: "本机询价文件存储被占用，请关闭旧页面后重试",
    writeFailed: "本机询价文件保存失败",
})

export function readInquiryFileValue<T>(key: string): Promise<T | undefined> {
    return store.read<T>(key)
}

export async function writeInquiryFileValue<T>(
    key: string,
    value: T,
): Promise<void> {
    await store.write(key, value)
}

export async function removeInquiryFileValue(key: string): Promise<void> {
    await store.remove(key)
}
