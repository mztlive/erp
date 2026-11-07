/**
 * 浏览器 IndexedDB 单键值存储工厂。
 *
 * 业务侧各自使用独立数据库名与文案，避免刷新后丢失待确认命令的原始文件。
 */
export type IndexedDbValueStore = Readonly<{
    read: <T>(key: string) => Promise<T | undefined>
    write: <T>(key: string, value: T) => Promise<void>
    remove: (key: string) => Promise<void>
}>

export type IndexedDbValueStoreMessages = Readonly<{
    /** 本机存储不可用时的提示。 */
    unavailable: string
    /** 旧页面占用数据库时的提示。 */
    blocked: string
    /** 写入或删除失败时的提示。 */
    writeFailed: string
}>

function openDatabase(
    databaseName: string,
    messages: IndexedDbValueStoreMessages,
): Promise<IDBDatabase> {
    return new Promise((resolve, reject) => {
        const request = indexedDB.open(databaseName, 1)
        request.onupgradeneeded = () => {
            if (!request.result.objectStoreNames.contains("values"))
                request.result.createObjectStore("values")
        }
        request.onsuccess = () => resolve(request.result)
        request.onerror = () =>
            reject(request.error ?? new Error(messages.unavailable))
        request.onblocked = () => reject(new Error(messages.blocked))
    })
}

async function operate<T>(
    databaseName: string,
    messages: IndexedDbValueStoreMessages,
    key: string,
    operation: "read" | "write" | "delete",
    value?: T,
): Promise<T | undefined> {
    const database = await openDatabase(databaseName, messages)
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
            reject(transaction.error ?? new Error(messages.writeFailed))
        }
    })
}

/**
 * 创建单键值 IndexedDB 存储。
 *
 * @param databaseName 独立数据库名；不同业务不共享。
 * @param messages 打开、占用与写入失败时的用户可见文案。
 */
export function createIndexedDbValueStore(
    databaseName: string,
    messages: IndexedDbValueStoreMessages,
): IndexedDbValueStore {
    return {
        read: <T>(key: string) =>
            operate<T>(databaseName, messages, key, "read"),
        write: async <T>(key: string, value: T) => {
            await operate(databaseName, messages, key, "write", value)
        },
        remove: async (key: string) => {
            await operate(databaseName, messages, key, "delete")
        },
    }
}
