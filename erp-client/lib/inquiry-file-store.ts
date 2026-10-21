/** 询价命令与凭证使用结构化存储，保留 File 原始内容供刷新后核对。 */
function openInquiryFileStore(): Promise<IDBDatabase> {
    return new Promise((resolve, reject) => {
        const request = indexedDB.open("erp-inquiry-files-v1", 1)
        request.onupgradeneeded = () => {
            if (!request.result.objectStoreNames.contains("values"))
                request.result.createObjectStore("values")
        }
        request.onsuccess = () => resolve(request.result)
        request.onerror = () =>
            reject(request.error ?? new Error("本机询价文件存储不可用"))
        request.onblocked = () =>
            reject(new Error("本机询价文件存储被占用，请关闭旧页面后重试"))
    })
}

async function inquiryFileValue<T>(
    key: string,
    operation: "read" | "write" | "delete",
    value?: T,
): Promise<T | undefined> {
    const database = await openInquiryFileStore()
    return new Promise((resolve, reject) => {
        const transaction = database.transaction(
            "values",
            operation === "read" ? "readonly" : "readwrite",
        )
        const store = transaction.objectStore("values")
        const request =
            operation === "read"
                ? store.get(key)
                : operation === "write"
                  ? store.put(value, key)
                  : store.delete(key)
        let result: T | undefined
        request.onsuccess = () => {
            if (operation === "read") result = request.result as T | undefined
        }
        transaction.oncomplete = () => {
            database.close()
            resolve(result)
        }
        transaction.onerror = transaction.onabort = () => {
            database.close()
            reject(transaction.error ?? new Error("本机询价文件保存失败"))
        }
    })
}

export function readInquiryFileValue<T>(key: string): Promise<T | undefined> {
    return inquiryFileValue<T>(key, "read")
}

export async function writeInquiryFileValue<T>(
    key: string,
    value: T,
): Promise<void> {
    await inquiryFileValue(key, "write", value)
}

export async function removeInquiryFileValue(key: string): Promise<void> {
    await inquiryFileValue(key, "delete")
}
